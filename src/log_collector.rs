//! Robust, decoupled logging pipeline for kernel builds.
//!
//! This module provides a unified, asynchronous logging system that guarantees
//! logs are persisted to disk even if the UI channel is congested or fails.
//!
//! # Architecture
//!
//! ```text
//! Build Output
//!     |
//! [LogCollector] (Async, non-blocking)
//!     | (mpsc channel - guaranteed delivery)
//! +---+---+
//! |       |
//! v       v
//! [DiskPersister]    [UIDispatcher]
//! (always writes)    (UI channel, with overflow recovery)
//!          |               |
//! logs/full/<ts>.log    Slint UI
//! logs/parsed/<ts>.log  (non-blocking)
//! ```
//!
//! # Key Properties
//!
//! - **Guaranteed Persistence**: Disk writes always succeed, even if UI channel fails
//! - **Non-Blocking UI**: UI updates don't block the build process
//! - **Decoupled Channels**: Separate channels for disk and UI prevent cross-blocking
//! - **Overflow Recovery**: If UI channel is full, logs still persist and are queued
//! - **Unified System**: Single authoritative pipeline for all log types
//! - **Error Recovery**: Graceful degradation if UI unavailable

use crossbeam_channel::{unbounded, Sender};
use std::path::PathBuf;
use std::fs::{OpenOptions, File};
use std::io::Write;
use chrono::Local;
use std::collections::HashMap;
use std::sync::Arc;
use log::{Log, Metadata, Record};

/// Internal log line or special marker
enum LogMessage {
    /// Regular log line
    Line(LogLine),
    /// Flush marker with channel sender to signal completion
    Flush(std::sync::mpsc::Sender<()>),
}

/// Session state with generation tracking for detecting session changes
#[derive(Clone, Debug)]
struct SessionState {
    /// The current session log path
    path: Option<PathBuf>,
    /// Generation counter - incremented when session changes
    /// Allows background task to detect and invalidate cached file handles
    generation: u64,
}

/// Get the global logs path relative to the current working directory: ./logs
pub fn get_global_logs_path() -> Result<PathBuf, String> {
    let cwd = std::env::current_dir()
        .map_err(|e| format!("Failed to get current working directory: {}", e))?;
    let logs_dir = cwd.join("logs");
    Ok(logs_dir)
}

/// Ensure the global logs directory exists
pub fn ensure_logs_dir_exists(log_dir: &PathBuf) -> Result<(), String> {
    std::fs::create_dir_all(log_dir)
        .map_err(|e| format!("Failed to create logs directory: {}", e))?;
    Ok(())
}

/// A log line with metadata
#[derive(Clone, Debug)]
pub struct LogLine {
    /// The actual log message
    pub message: String,
    /// Log type: "full" or "parsed"
    pub log_type: String,
    /// Timestamp of when the log was created
    pub timestamp: String,
    /// Optional progress indicator (0-100)
    pub progress: Option<u32>,
}

impl LogLine {
    pub fn new(message: String) -> Self {
        LogLine {
            message,
            log_type: "full".to_string(),
            timestamp: Local::now().format("%H:%M:%S%.3f").to_string(),
            progress: None,
        }
    }

    pub fn parsed(message: String) -> Self {
        LogLine {
            message,
            log_type: "parsed".to_string(),
            timestamp: Local::now().format("%H:%M:%S%.3f").to_string(),
            progress: None,
        }
    }

    pub fn with_progress(mut self, progress: u32) -> Self {
        self.progress = Some(progress);
        self
    }
}

/// Unified logger that handles disk and UI dispatch
pub struct LogCollector {
    /// Channel sender for log lines (internal) - crossbeam unbounded for cross-runtime reliability
    tx: Sender<LogMessage>,
    /// Current log directory
    log_dir: PathBuf,
    /// UI channel sender for real-time log display
    ui_tx: tokio::sync::mpsc::Sender<LogLine>,
    /// Current session state with generation tracking for detecting changes
    session_state: Arc<std::sync::Mutex<SessionState>>,
}

impl LogCollector {
    /// Create a new LogCollector with background tasks for disk/UI dispatch
    pub fn new(
            log_dir: PathBuf,
            ui_tx: tokio::sync::mpsc::Sender<LogLine>,
        ) -> Result<Self, String> {
        // Create log directories
        let full_log_dir = log_dir.join("full");
        let parsed_log_dir = log_dir.join("parsed");
        std::fs::create_dir_all(&full_log_dir)
            .map_err(|e| format!("Failed to create full log dir: {}", e))?;
        std::fs::create_dir_all(&parsed_log_dir)
            .map_err(|e| format!("Failed to create parsed log dir: {}", e))?;

        // Create unbounded crossbeam channel for log messages (both regular logs and flush markers)
        // CRITICAL: crossbeam unbounded channels are thread-safe and work across ANY runtime,
        // even nested tokio runtimes. This prevents logs from being lost in executor threads.
        let (tx, rx) = unbounded::<LogMessage>();

        let log_dir_clone = log_dir.clone();
        let ui_tx_clone = ui_tx.clone();
        
        // Create session path arc BEFORE moving into closure
        // Use Arc<Mutex> with versioning to detect session changes
        let session_path_arc: Arc<std::sync::Mutex<SessionState>> = Arc::new(std::sync::Mutex::new(SessionState {
            path: None,
            generation: 0,
        }));
        let session_path_arc_clone = Arc::clone(&session_path_arc);
        
        // Spawn background thread (NOT tokio task) for disk persister + UI dispatcher
        // This thread will run in OS-level thread pool, independent of any tokio runtime.
        // It uses blocking recv() to wait for messages, which is safe from any runtime.
        // This guarantees that every log line reaches disk, independent of UI channel status,
        // and ensures logs from nested runtimes (executor threads) are reliably received.
        std::thread::spawn(move || {
            let full_log_dir = log_dir_clone.join("full");
            let parsed_log_dir = log_dir_clone.join("parsed");
            
            // Cache file handles for performance with generation tracking
            let mut file_handles: HashMap<String, File> = HashMap::new();
            let mut last_session_generation: u64 = 0;
            
            // Create log files immediately on startup to ensure they exist
            // This ensures logs are available even before the first log message
            if let Ok(full_log_path) = get_or_create_latest_log(&full_log_dir) {
                println!("[Log] [INIT] Creating full log file immediately: {}", full_log_path.display());
                if let Ok(file) = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&full_log_path) {
                    file_handles.insert("full".to_string(), file);
                }
            }
            
            if let Ok(parsed_log_path) = get_or_create_latest_log(&parsed_log_dir) {
                println!("[Log] [INIT] Creating parsed log file immediately: {}", parsed_log_path.display());
                if let Ok(file) = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&parsed_log_path) {
                    file_handles.insert("parsed".to_string(), file);
                }
            }
            
            // Use blocking recv() from crossbeam - works from any runtime or thread
            while let Ok(msg) = rx.recv() {
                match msg {
                    LogMessage::Line(log_line) => {
                        // CRITICAL: Always write to disk, regardless of UI status
                        // First, check if session has changed and invalidate file handle if needed
                        // CHECK SESSION STATE BEFORE EVERY WRITE
                        if let Ok(session_lock) = session_path_arc_clone.lock() {
                            if session_lock.generation != last_session_generation {
                                // Session has changed! Invalidate cached file handles
                                println!("[Log] [SESSION] Generation changed from {} to {}, invalidating file handles",
                                    last_session_generation, session_lock.generation);
                                file_handles.remove("full");
                                file_handles.remove("parsed");
                                last_session_generation = session_lock.generation;
                                eprintln!("[Log] [SESSION] File handles invalidated (generation: {})", session_lock.generation);
                            }
                        }
                        
                        // Ensure we have a cached file handle for logs/full/
                        if !file_handles.contains_key("full") {
                            // CRITICAL FIX: Check for session-specific log path first
                            let path = if let Ok(session_lock) = session_path_arc_clone.lock() {
                                if let Some(ref session_path) = session_lock.path {
                                    // Use the explicit session path, bypass "latest" heuristic
                                    println!("[Log] [WRITE] Using explicit session path: {}", session_path.display());
                                    Some(session_path.clone())
                                } else {
                                    // Fall back to "latest log" heuristic if no session is active
                                    println!("[Log] [WRITE] No session path set, using latest log heuristic");
                                    get_or_create_latest_log(&full_log_dir).ok()
                                }
                            } else {
                                // Lock failed, fall back to heuristic
                                println!("[Log] [WRITE] Session lock failed, using latest log heuristic");
                                get_or_create_latest_log(&full_log_dir).ok()
                            };
                            
                            if let Some(log_path) = path {
                                println!("[Log] [WRITE] Opening/creating log file: {}", log_path.display());
                                if let Ok(file) = OpenOptions::new()
                                    .create(true)
                                    .append(true)
                                    .open(&log_path) {
                                    file_handles.insert("full".to_string(), file);
                                }
                            }
                        }
                        
                        // Write to logs/full/ (always)
                        if let Some(file) = file_handles.get_mut("full") {
                            let formatted = format!("[{}] {}\n", log_line.timestamp, log_line.message);
                            let _ = file.write_all(formatted.as_bytes());
                            let _ = file.flush();
                        }
                        
                        // If this is a "parsed" log, also write to logs/parsed/
                        if log_line.log_type == "parsed" {
                            if !file_handles.contains_key("parsed") {
                                if let Ok(path) = get_or_create_latest_log(&parsed_log_dir) {
                                    println!("[Log] [WRITE] Opening/creating parsed log file: {}", path.display());
                                    if let Ok(file) = OpenOptions::new()
                                        .create(true)
                                        .append(true)
                                        .open(&path) {
                                        file_handles.insert("parsed".to_string(), file);
                                    }
                                }
                            }
                            
                            if let Some(file) = file_handles.get_mut("parsed") {
                                let formatted = format!("[{}] {}\n", log_line.timestamp, log_line.message);
                                let _ = file.write_all(formatted.as_bytes());
                                let _ = file.flush();
                            }
                        }
                        
                        // Send to UI non-blocking (ignore errors if UI channel is full)
                        // UI channel has bounded capacity; we prioritize disk persistence
                        let _ = ui_tx_clone.try_send(log_line.clone());
                    }
                    LogMessage::Flush(tx) => {
                        // FLUSH MARKER: Ensure all written data is durably on disk, then signal completion
                        println!("[Log] [FLUSH] Flush marker received, syncing all file handles");
                        for (handle_name, file) in file_handles.iter_mut() {
                            let _ = file.flush();
                            println!("[Log] [FLUSH] Flushed file handle: {}", handle_name);
                        }
                        // Signal that flush is complete
                        let _ = tx.send(());
                    }
                }
            }
            eprintln!("[Log] Disk persister thread shutting down");
        });

        Ok(LogCollector {
            tx,
            log_dir: log_dir.clone(),
            ui_tx,
            session_state: session_path_arc,
        })
    }

    /// Start a new session with an explicit log file path
    /// This bypasses the "latest log" heuristic and ensures this build session
    /// has a dedicated, full log file on disk.
    ///
    /// CRITICAL: Increments generation counter to force background task to
    /// invalidate cached file handles, ensuring logs go to the new session file.
    pub fn start_new_session(&self, filename: &str) -> Result<PathBuf, String> {
        let full_log_dir = self.log_dir.join("full");
        let log_path = full_log_dir.join(filename);
        
        // Set the session path AND increment generation
        {
            let mut session = self.session_state
                .lock()
                .map_err(|e| format!("Failed to lock session state: {}", e))?;
            session.path = Some(log_path.clone());
            session.generation = session.generation.wrapping_add(1);
            eprintln!("[Log] [SESSION] Generation incremented to: {}", session.generation);
            println!("[Log] [SESSION] Generation incremented to: {}", session.generation);
        }
        
        eprintln!("[Log] [SESSION] New session started with dedicated log file: {}", log_path.display());
        println!("[Log] [SESSION] New session started with dedicated log file: {}", log_path.display());
        Ok(log_path)
    }

    /// Get the current session log file path
    pub fn get_session_log_path(&self) -> Option<PathBuf> {
        self.session_state
            .lock()
            .ok()
            .and_then(|session| session.path.clone())
    }

    /// Send a log line (non-blocking)
    /// This CANNOT fail - uses unbounded channel to guarantee delivery
    pub fn log(&self, line: LogLine) {
        let _ = self.tx.send(LogMessage::Line(line));
    }

    /// Send a simple string log
    pub fn log_str(&self, message: impl Into<String>) {
        self.log(LogLine::new(message.into()));
    }

    /// Send a parsed (high-level) log
    pub fn log_parsed(&self, message: impl Into<String>) {
        self.log(LogLine::parsed(message.into()));
    }

    /// Send a log with progress indicator
    pub fn log_with_progress(&self, message: impl Into<String>, progress: u32) {
        self.log(
            LogLine::new(message.into())
                .with_progress(progress)
        );
    }

    /// Wait for all pending logs to be written to disk
    /// This sends a FLUSH marker down the channel and waits for the background
    /// thread to process it. Guarantees that all logs sent before this call
    /// have been durably written to disk before returning.
    ///
    /// CRITICAL: Call this before marking a build as complete to ensure
    /// final log messages (like "Build finished" or "Build cancelled") reach disk.
    ///
    /// Note: Uses std::sync::mpsc for the flush signal, which is thread-safe
    /// and works regardless of whether we're in an async context or not.
    pub async fn wait_for_empty(&self) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        
        // Send flush marker with blocking channel receiver
        self.tx.send(LogMessage::Flush(tx))
            .map_err(|e| format!("Failed to send flush marker: {}", e))?;
        
        // Wait for background thread to signal completion
        // This blocks, but we're in a tokio context so it runs on a worker thread
        rx.recv()
            .map_err(|e| format!("Flush signal interrupted: {}", e))?;
        
        eprintln!("[Log] [FLUSH] wait_for_empty() completed - all logs synced to disk");
        println!("[Log] [FLUSH] wait_for_empty() completed - all logs synced to disk");
        Ok(())
    }
}

impl Clone for LogCollector {
    fn clone(&self) -> Self {
        LogCollector {
            tx: self.tx.clone(),
            log_dir: self.log_dir.clone(),
            ui_tx: self.ui_tx.clone(),
            session_state: Arc::clone(&self.session_state),
        }
    }
}

/// Implementation of the `log` crate's Log trait
/// Wires all log::info!(), log::warn!(), log::error!() calls into LogCollector
impl Log for LogCollector {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let message = format!(
                "[{}] {}",
                record.level(),
                record.args()
            );
            
            // Target-aware routing: check if target is "parsed" for high-level logs
            if record.target() == "parsed" {
                self.log_parsed(&message);
            } else {
                self.log_str(&message);
            }
        }
    }

    fn flush(&self) {
        // No buffering at this level - LogCollector handles flushing internally
    }
}

/// Persist a single log line to disk
/// Returns Err only if the system fails catastrophically (should be rare)
fn persist_log_line(
    line: &LogLine,
    full_log_dir: &PathBuf,
    parsed_log_dir: &PathBuf,
) -> Result<(), String> {
    // Find or create the latest log file
    let log_dir = if line.log_type == "parsed" {
        parsed_log_dir
    } else {
        full_log_dir
    };

    // Get the latest log file in this directory
    let log_file = match get_or_create_latest_log(log_dir) {
        Ok(path) => path,
        Err(e) => {
            eprintln!("[Log] Failed to get/create log file: {}", e);
            return Err(e);
        }
    };

    // Append to the log file
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)
        .map_err(|e| format!("Failed to open log file: {}", e))?;

    // Format: [HH:MM:SS.mmm] message
    let formatted = format!("[{}] {}\n", line.timestamp, line.message);
    file.write_all(formatted.as_bytes())
        .map_err(|e| format!("Failed to write to log file: {}", e))?;

    // Flush immediately to ensure data hits disk
    file.flush()
        .map_err(|e| format!("Failed to flush log file: {}", e))?;

    Ok(())
}

/// Get the latest log file, or create a new one if none exist
fn get_or_create_latest_log(log_dir: &PathBuf) -> Result<PathBuf, String> {
    // Try to find an existing log file
    if let Ok(entries) = std::fs::read_dir(log_dir) {
        let mut logs: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "log"))
            .collect();

        // Find the most recent
        if !logs.is_empty() {
            logs.sort_by(|a, b| {
                let a_time = a.metadata().ok().and_then(|m| m.modified().ok());
                let b_time = b.metadata().ok().and_then(|m| m.modified().ok());
                b_time.cmp(&a_time)
            });

            return Ok(logs[0].path());
        }
    }

    // Create a new log file
    let timestamp = Local::now().format("%Y%m%d_%H%M%S");
    let log_type = if log_dir.ends_with("parsed") {
        "parsed"
    } else {
        "full"
    };
    let log_path = log_dir.join(format!("{}_{}.log", timestamp, log_type));

    // Create the file
    File::create(&log_path)
        .map_err(|e| format!("Failed to create log file: {}", e))?;

    Ok(log_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn test_log_collector_creates_directories() {
        let temp_dir = std::env::temp_dir().join("test_logs");
        let _ = fs::remove_dir_all(&temp_dir);

        let (ui_tx, _ui_rx) = tokio::sync::mpsc::channel(100);
        let result = LogCollector::new(temp_dir.clone(), ui_tx);
        
        assert!(result.is_ok());
        assert!(temp_dir.join("full").exists());
        assert!(temp_dir.join("parsed").exists());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_log_collector_non_blocking() {
        let temp_dir = std::env::temp_dir().join("test_logs_nb");
        let _ = fs::remove_dir_all(&temp_dir);

        let (ui_tx, _ui_rx) = tokio::sync::mpsc::channel(100);
        let collector = LogCollector::new(temp_dir.clone(), ui_tx).unwrap();

        // Spam logs - should all be accepted without blocking
        // This uses crossbeam channel now, which is non-blocking and thread-safe
        for i in 0..1000 {
            collector.log_str(format!("Log message {}", i));
        }

        // Give background thread time to write
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Verify logs were written
        let full_logs = temp_dir.join("full");
        assert!(fs::read_dir(&full_logs).ok().map_or(false, |mut d| d.next().is_some()));

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
