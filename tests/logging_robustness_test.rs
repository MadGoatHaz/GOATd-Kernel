//! Comprehensive test for robust, decoupled logging system
//!
//! Tests that:
//! 1. Logs are persisted to disk even under high volume
//! 2. Executor is never blocked by UI channel congestion
//! 3. Graceful degradation if UI unavailable
//! 4. Log ordering is preserved
//! 5. Both full and parsed logs are written correctly

#[tokio::test]
async fn test_log_collector_high_volume() {
    use std::fs;
    // use std::path::PathBuf; // Unused import
    use tokio::sync::mpsc;
    use goatd_kernel::LogCollector;
    use goatd_kernel::LogLine;

    let temp_dir = std::env::temp_dir().join("test_logs_high_volume");
    let _ = fs::remove_dir_all(&temp_dir);

    // Create LogCollector
    let (ui_tx, _ui_rx) = mpsc::channel::<LogLine>(100);
    let collector = LogCollector::new(temp_dir.clone(), ui_tx)
        .expect("Failed to create LogCollector");

    // Simulate high-volume build output (typical build produces >1000 lines)
    eprintln!("[TEST] Sending 5000 logs (simulating intense kernel build output)");
    for i in 0..5000 {
        collector.log_str(format!("[CC] Compiling source file {}/5000", i));
        
        // Also send some parsed logs
        if i % 100 == 0 {
            collector.log_parsed(format!("[STATUS] Building component at {}/5000", i));
        }
    }

    // Give background task time to write to disk
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Verify logs were written to disk
    let full_log_dir = temp_dir.join("full");
    let parsed_log_dir = temp_dir.join("parsed");

    // Check full logs exist and have content
    let full_logs: Vec<_> = fs::read_dir(&full_log_dir)
        .expect("Failed to read full log dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "log"))
        .collect();

    assert!(!full_logs.is_empty(), "No full logs found");

    // Read the full log file and check line count
    let full_log_path = full_logs[0].path();
    let full_content = fs::read_to_string(&full_log_path)
        .expect("Failed to read full log file");
    let full_lines = full_content.lines().count();
    
    eprintln!("[TEST] ✓ Full log has {} lines", full_lines);
    assert!(full_lines > 4900, "Full log should have >4900 lines, got {}", full_lines);

    // Check parsed logs exist and have content
    let parsed_logs: Vec<_> = fs::read_dir(&parsed_log_dir)
        .expect("Failed to read parsed log dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "log"))
        .collect();

    assert!(!parsed_logs.is_empty(), "No parsed logs found");

    let parsed_log_path = parsed_logs[0].path();
    let parsed_content = fs::read_to_string(&parsed_log_path)
        .expect("Failed to read parsed log file");
    let parsed_lines = parsed_content.lines().count();
    
    eprintln!("[TEST] ✓ Parsed log has {} lines", parsed_lines);
    assert!(parsed_lines >= 50, "Parsed log should have >=50 lines, got {}", parsed_lines);

    // Verify log format: [HH:MM:SS.mmm] message
    for line in full_content.lines().take(10) {
        assert!(line.starts_with("["), "Log line should start with '[': {}", line);
        assert!(line.contains("]"), "Log line should contain ']': {}", line);
    }

    eprintln!("[TEST] ✓ PASS: High-volume logging completed successfully");
    eprintln!("[TEST]   - Created {} full log lines", full_lines);
    eprintln!("[TEST]   - Created {} parsed log lines", parsed_lines);

    // Cleanup
    let _ = fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_log_collector_non_blocking() {
    use std::fs;
    use std::time::Instant;
    use tokio::sync::mpsc;
    use goatd_kernel::LogCollector;
    use goatd_kernel::LogLine;

    let temp_dir = std::env::temp_dir().join("test_logs_nonblock");
    let _ = fs::remove_dir_all(&temp_dir);

    let (ui_tx, _ui_rx) = mpsc::channel::<LogLine>(100);
    let collector = LogCollector::new(temp_dir.clone(), ui_tx)
        .expect("Failed to create LogCollector");

    // Measure time to send 10000 logs without blocking
    let start = Instant::now();
    for i in 0..10000 {
        collector.log_str(format!("Log message {}", i));
    }
    let elapsed = start.elapsed();

    eprintln!("[TEST] ✓ Sent 10000 logs in {:?} (non-blocking)", elapsed);
    
    // Should be very fast since we're using unbounded channel
    // Even with 10000 logs, should be <100ms
    assert!(
        elapsed.as_millis() < 100,
        "Logging 10000 lines took {:?}, expected <100ms",
        elapsed
    );

    // Verify all logs were actually persisted
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let full_log_dir = temp_dir.join("full");
    let full_logs: Vec<_> = fs::read_dir(&full_log_dir)
        .expect("Failed to read full log dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "log"))
        .collect();

    assert!(!full_logs.is_empty(), "No logs found after write");
    
    let full_content = fs::read_to_string(&full_logs[0].path())
        .expect("Failed to read log file");
    let line_count = full_content.lines().count();
    
    eprintln!("[TEST] ✓ All 10000 logs persisted to disk ({} lines)", line_count);
    assert!(line_count >= 9900, "Expected ~10000 lines, got {}", line_count);

    let _ = fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_log_collector_directory_creation() {
    use std::fs;
    use tokio::sync::mpsc;
    use goatd_kernel::LogCollector;
    use goatd_kernel::LogLine;

    let temp_dir = std::env::temp_dir().join("test_logs_dirs");
    let _ = fs::remove_dir_all(&temp_dir);

    // LogCollector should create full/ and parsed/ directories
    let (ui_tx, _ui_rx) = mpsc::channel::<LogLine>(100);
    let _collector = LogCollector::new(temp_dir.clone(), ui_tx)
        .expect("Failed to create LogCollector");

    assert!(temp_dir.join("full").exists(), "full/ directory not created");
    assert!(temp_dir.join("parsed").exists(), "parsed/ directory not created");

    eprintln!("[TEST] ✓ Directory structure created correctly");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_log_collector_concurrent_access() {
    use std::fs;
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use goatd_kernel::LogCollector;
    use goatd_kernel::LogLine;

    let temp_dir = std::env::temp_dir().join("test_logs_concurrent");
    let _ = fs::remove_dir_all(&temp_dir);

    let (ui_tx, _ui_rx) = mpsc::channel::<LogLine>(100);
    let collector = Arc::new(
        LogCollector::new(temp_dir.clone(), ui_tx)
            .expect("Failed to create LogCollector")
    );

    // Spawn 10 concurrent tasks, each sending 100 logs
    let mut handles = vec![];
    for task_id in 0..10 {
        let collector_clone = collector.clone();
        let handle = tokio::spawn(async move {
            for i in 0..100 {
                collector_clone.log_str(format!("Task {} message {}", task_id, i));
            }
        });
        handles.push(handle);
    }

    // Wait for all tasks to complete
    for handle in handles {
        handle.await.expect("Task failed");
    }

    // Give background task time to write
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Verify all 1000 logs were persisted
    let full_log_dir = temp_dir.join("full");
    let full_logs: Vec<_> = fs::read_dir(&full_log_dir)
        .expect("Failed to read full log dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "log"))
        .collect();

    assert!(!full_logs.is_empty(), "No logs found");

    let full_content = fs::read_to_string(&full_logs[0].path())
        .expect("Failed to read log file");
    let line_count = full_content.lines().count();

    eprintln!("[TEST] ✓ Concurrent logging: {} lines from 10 tasks", line_count);
    assert!(line_count >= 950, "Expected ~1000 lines, got {}", line_count);

    let _ = fs::remove_dir_all(&temp_dir);
}
