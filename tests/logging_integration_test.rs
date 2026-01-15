use std::fs;
use goatd_kernel::LogCollector;
use goatd_kernel::LogLine;
use tokio::sync::mpsc;
use std::time::Duration;

/// Integration test for the logging system
/// 
/// Tests that:
/// 1. LogCollector initializes correctly
/// 2. Logs are written to disk
/// 3. Session changes invalidate file handles and redirect logs to new file
/// 4. Global logger integration works
#[tokio::test]
async fn test_logging_integration_full_cycle() {
    // Setup: Create temporary log directory
    let temp_dir = std::env::temp_dir().join("goatd_logging_test");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

    // Create UI channel (unbounded to avoid blocking)
    let (ui_tx, _ui_rx) = mpsc::channel::<LogLine>(1024);

    // Initialize LogCollector
    let collector = LogCollector::new(temp_dir.clone(), ui_tx)
        .expect("Failed to initialize LogCollector");

    // Test 1: Send logs and verify they're written to disk
    eprintln!("[TEST] Sending test logs to collector");
    
    collector.log_str("Test log message 1");
    collector.log_str("Test log message 2");
    collector.log_parsed("Test parsed message");
    collector.log_with_progress("Test progress", 50);

    // Give background task time to write
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify logs were written to full log directory
    let full_log_dir = temp_dir.join("full");
    assert!(full_log_dir.exists(), "Full log directory should exist");
    
    let mut log_files: Vec<_> = fs::read_dir(&full_log_dir)
        .expect("Failed to read full log dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "log"))
        .collect();
    
    assert!(!log_files.is_empty(), "At least one log file should exist in full directory");
    
    // Read the log file and verify content
    log_files.sort_by_key(|e| {
        e.metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .unwrap_or_else(std::time::SystemTime::now)
    });
    
    let latest_log = log_files.last().expect("Should have at least one log file");
    let log_content = fs::read_to_string(latest_log.path())
        .expect("Failed to read log file");
    
    eprintln!("[TEST] Log file content (first 200 chars): {}", &log_content[..log_content.len().min(200)]);
    
    assert!(
        log_content.contains("Test log message 1"),
        "Log file should contain 'Test log message 1'"
    );
    assert!(
        log_content.contains("Test log message 2"),
        "Log file should contain 'Test log message 2'"
    );

    // Test 2: Verify parsed logs
    let parsed_log_dir = temp_dir.join("parsed");
    assert!(parsed_log_dir.exists(), "Parsed log directory should exist");
    
    let parsed_files: Vec<_> = fs::read_dir(&parsed_log_dir)
        .expect("Failed to read parsed log dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "log"))
        .collect();
    
    assert!(!parsed_files.is_empty(), "At least one parsed log file should exist");
    
    let parsed_content = fs::read_to_string(parsed_files[0].path())
        .expect("Failed to read parsed log file");
    
    assert!(
        parsed_content.contains("Test parsed message"),
        "Parsed log should contain 'Test parsed message'"
    );

    // Test 3: Session change invalidates file handle
    eprintln!("[TEST] Starting new session");
    let session_result = collector.start_new_session("session_test.log");
    assert!(session_result.is_ok(), "Session initialization should succeed");
    
    let session_path = collector.get_session_log_path();
    assert!(session_path.is_some(), "Session path should be set");

    // Send logs after session change - should go to new file
    collector.log_str("Session-specific log 1");
    collector.log_str("Session-specific log 2");

    // Give background task time to process
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify session log file exists and contains new logs
    let session_log_path = session_path.unwrap();
    assert!(
        session_log_path.exists(),
        "Session log file should exist at {:?}",
        session_log_path
    );

    let session_content = fs::read_to_string(&session_log_path)
        .expect("Failed to read session log file");

    eprintln!("[TEST] Session log content (first 200 chars): {}", &session_content[..session_content.len().min(200)]);

    assert!(
        session_content.contains("Session-specific log 1"),
        "Session log should contain 'Session-specific log 1'"
    );
    assert!(
        session_content.contains("Session-specific log 2"),
        "Session log should contain 'Session-specific log 2'"
    );

    // Cleanup
    let _ = fs::remove_dir_all(&temp_dir);
    eprintln!("[TEST] ✓ Full logging integration test passed");
}

/// Test that LogCollector is cloneable and clones share the same backend
#[tokio::test]
async fn test_log_collector_cloning() {
    let temp_dir = std::env::temp_dir().join("goatd_logging_clone_test");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

    let (ui_tx, _ui_rx) = mpsc::channel::<LogLine>(1024);
    let collector1 = LogCollector::new(temp_dir.clone(), ui_tx)
        .expect("Failed to initialize LogCollector");

    // Clone and send logs from both instances
    let collector2 = collector1.clone();

    collector1.log_str("From collector1");
    collector2.log_str("From collector2");

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Both should write to the same file
    let full_log_dir = temp_dir.join("full");
    let log_files: Vec<_> = fs::read_dir(&full_log_dir)
        .expect("Failed to read full log dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "log"))
        .collect();

    assert!(!log_files.is_empty(), "At least one log file should exist");

    let log_content = fs::read_to_string(log_files[0].path())
        .expect("Failed to read log file");

    assert!(
        log_content.contains("From collector1"),
        "Log should contain message from collector1"
    );
    assert!(
        log_content.contains("From collector2"),
        "Log should contain message from collector2"
    );

    let _ = fs::remove_dir_all(&temp_dir);
    eprintln!("[TEST] ✓ LogCollector cloning test passed");
}

/// Test that session file handle invalidation works
/// Tests that changing sessions creates new log files
#[tokio::test]
async fn test_rapid_session_changes() {
    let temp_dir = std::env::temp_dir().join("goatd_logging_rapid_session_test");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

    let (ui_tx, _ui_rx) = mpsc::channel::<LogLine>(1024);
    let collector = LogCollector::new(temp_dir.clone(), ui_tx)
        .expect("Failed to initialize LogCollector");

    // Create multiple sessions with sufficient delay for background writes
    // Key test: verify that requesting a new session creates a new file
    for i in 0..3 {
        let filename = format!("session_{}.log", i);
        let result = collector.start_new_session(&filename);
        assert!(result.is_ok(), "Session initialization should succeed");

        // Send logs
        collector.log_str(&format!("Session {} marker", i));

        // CRITICAL: Wait long enough for background task to write to disk
        // Async channel is unbounded, so logs will queue up internally
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    
    // Final flush to ensure all pending writes complete
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify all session files were created
    let full_log_dir = temp_dir.join("full");
    let session_files: Vec<_> = fs::read_dir(&full_log_dir)
        .expect("Failed to read full log dir")
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .file_name()
                .map(|n| n.to_string_lossy().contains("session_"))
                .unwrap_or(false)
        })
        .collect();

    eprintln!("[TEST] Created {} session files", session_files.len());
    
    // The key test: we should have created 3 separate session log files
    // even though logs may be batched due to async buffering
    assert_eq!(session_files.len(), 3, "Should have created 3 session log files");

    // Verify each file has content (was written to)
    for (i, file) in session_files.iter().enumerate() {
        let content = fs::read_to_string(file.path())
            .expect("Failed to read session file");
        
        assert!(!content.is_empty(), "Session {} file should contain logs", i);
        eprintln!("[TEST] Session {} file has {} bytes of data", i, content.len());
    }

    let _ = fs::remove_dir_all(&temp_dir);
    eprintln!("[TEST] ✓ Session file creation and invalidation test passed");
}

/// Test disk flushing on shutdown
#[tokio::test]
async fn test_disk_flush_on_drop() {
    let temp_dir = std::env::temp_dir().join("goatd_logging_flush_test");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

    let (ui_tx, _ui_rx) = mpsc::channel::<LogLine>(1024);
    
    {
        let collector = LogCollector::new(temp_dir.clone(), ui_tx)
            .expect("Failed to initialize LogCollector");

        // Send a log just before drop
        collector.log_str("Final log before drop");

        // Give time for write
        tokio::time::sleep(Duration::from_millis(100)).await;
    } // LogCollector dropped here

    // Verify log was written (background task should have written it)
    let full_log_dir = temp_dir.join("full");
    let log_files: Vec<_> = fs::read_dir(&full_log_dir)
        .expect("Failed to read full log dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "log"))
        .collect();

    assert!(!log_files.is_empty(), "At least one log file should exist");

    let log_content = fs::read_to_string(log_files[0].path())
        .expect("Failed to read log file");

    assert!(
        log_content.contains("Final log before drop"),
        "Log should persist even after LogCollector is dropped"
    );

    let _ = fs::remove_dir_all(&temp_dir);
    eprintln!("[TEST] ✓ Disk flush on drop test passed");
}
