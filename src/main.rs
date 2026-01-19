use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::RwLock;

use goatd_kernel::ui::controller::{AppController, BuildEvent};
use goatd_kernel::ui::app::AppUI;
use goatd_kernel::{LogCollector, LogLine};
use goatd_kernel::log_collector::{get_global_logs_path, ensure_logs_dir_exists};
use goatd_kernel::system::performance::diagnostic_buffer::init_global_buffer;

#[tokio::main]
async fn main() -> goatd_kernel::Result<()> {
    // =========================================================================
    // LOGGING INITIALIZATION - MUST BE FIRST
    // =========================================================================
    // Initialize unified logging system via goatd_kernel::system
    // This is the single source of truth for log file creation and rotation
    goatd_kernel::system::initialize_logging();
    eprintln!("[Main] ✓ Unified logging system initialized via system::initialize_logging()");
    
    // =========================================================================
    // DIAGNOSTIC BUFFER INITIALIZATION - BEFORE MONITORING THREADS
    // =========================================================================
    // Initialize the global diagnostic buffer with capacity for 4096 messages
    // This must be initialized early to prevent panics in collector threads
    let _ = init_global_buffer(4096);
    eprintln!("[Main] ✓ Diagnostic buffer initialized (capacity=4096, non-blocking)");
    
    // =========================================================================
    // ROBUST LOG COLLECTOR - DECOUPLED FROM UI
    // =========================================================================
    let log_dir = match get_global_logs_path() {
        Ok(dir) => {
            ensure_logs_dir_exists(&dir)?;
            dir
        }
        Err(e) => {
            eprintln!("[Main] ERROR: Failed to get global logs path: {}", e);
            return Err(format!("Failed to determine logs directory: {}", e).into());
        }
    };
    let (log_ui_tx, mut log_ui_rx) = mpsc::channel::<LogLine>(1024);
    let log_collector = match LogCollector::new(log_dir, log_ui_tx) {
        Ok(collector) => {
            eprintln!("[Main] ✓ LogCollector initialized (decoupled, non-blocking)");
            Arc::new(collector)
        }
        Err(e) => {
            eprintln!("[Main] WARNING: LogCollector initialization failed: {}", e);
            return Err(format!("LogCollector initialization failed: {}", e).into());
        }
    };
    
    // CRITICAL: Wire LogCollector as the global logger for the `log` crate
    // This ensures all log::info!(), log::warn!(), log::error!() calls are piped to disk
    let max_level = log::LevelFilter::Info;
    if let Err(e) = log::set_boxed_logger(Box::new((*log_collector).clone()))
        .map(|()| log::set_max_level(max_level)) {
        eprintln!("[Main] WARNING: Failed to set LogCollector as global logger: {}", e);
    } else {
        eprintln!("[Main] ✓ LogCollector registered as global logger (all log::* macros piped to disk)");
    }
    
    // Log initialization confirmation immediately after setup
    log::info!("GOATd Kernel logging initialized");
    
    // =========================================================================
    // CONTROLLER AND CHANNELS SETUP
    // =========================================================================
    let (build_tx, build_rx) = tokio::sync::mpsc::channel::<BuildEvent>(65536);
    
    // =========================================================================
    // LOG UI DRAINING TASK - FORWARD LOGS TO BUILD EVENT CHANNEL
    // =========================================================================
    // Spawn an async task to drain log_ui_rx and forward to build_tx as BuildEvent
    let build_tx_clone = build_tx.clone();
    tokio::spawn(async move {
        let mut log_count = 0u32;
        while let Some(log_line) = log_ui_rx.recv().await {
            log_count += 1;
            
            // Route based on log type
            match log_line.log_type.as_str() {
                "parsed" => {
                    // High-level status updates
                    let _ = build_tx_clone.send(BuildEvent::StatusUpdate(log_line.message.clone())).await;
                }
                _ => {
                    // Regular detailed logs
                    let _ = build_tx_clone.send(BuildEvent::Log(log_line.message.clone())).await;
                }
            }
            
            // Throttle logging to prevent channel overwhelm
            if log_count % 1000 == 0 {
                eprintln!("[Main] Log drain task: processed {} messages", log_count);
            }
        }
        eprintln!("[Main] Log drain task: shutting down (log_ui_rx closed)");
    });
    let (cancel_tx, _cancel_rx) = tokio::sync::watch::channel(false);
    
    let controller = AppController::new_production(build_tx.clone(), cancel_tx.clone(), Some(log_collector.clone())).await;
    let controller = Arc::new(RwLock::new(controller));
    
    // =========================================================================
    // HARDWARE DETECTION - BACKGROUND TASK FOR STARTUP OPTIMIZATION
    // =========================================================================
    // Move hardware detection to background to avoid blocking UI thread on startup
    // CRITICAL FIX: Pass egui context to signal repaint upon hardware detection completion
    let hw_cache_clone = controller.clone()
        .read()
        .await
        .cached_hardware_info.clone();
    let hw_ts_clone = controller.clone()
        .read()
        .await
        .hardware_cache_timestamp.clone();
    
    tokio::spawn(async move {
        eprintln!("[Main] [HW] Starting background hardware detection");
        let mut detector = goatd_kernel::hardware::HardwareDetector::new();
        match detector.detect_all() {
            Ok(hw_info) => {
                eprintln!("[Main] [HW] ✓ Detected: CPU={}, RAM={}GB, GPU={:?}",
                           hw_info.cpu_model, hw_info.ram_gb, hw_info.gpu_vendor);
                // Update controller cache with detected hardware using individual field locks
                let _ = hw_cache_clone.write().map(|mut cache| {
                    *cache = Some(hw_info.clone());
                });
                let _ = hw_ts_clone.write().map(|mut ts| {
                    *ts = Some(std::time::Instant::now());
                });
                eprintln!("[Main] [HW] Done: Hardware info cached in controller");
                eprintln!("[Main] [HW] Hardware detection async task completed (repaint signals deferred to app.rs update loop)");
            }
            Err(e) => {
                eprintln!("[Main] [HW] WARNING: Hardware detection failed: {}", e);
            }
        }
    });
    
    // =========================================================================
    // STARTUP AUDIT - CONSOLIDATED IN APPCONTROLLER::NEW_ASYNC
    // =========================================================================
    // Note: Initial deep audit is now triggered only in AppController::new_async()
    // to avoid redundant audits. audit_on_startup configuration is respected there.
    
    // =========================================================================
    // INITIALIZE EGUI APP
    // =========================================================================
    let app_ui = AppUI::new(controller.clone(), Some(build_rx));
    
    // =========================================================================
    // LAUNCH EGUI
    // =========================================================================
    eprintln!("[Main] Application initialized and running");
    eprintln!("[Main] Launching egui frontend...");
    
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0]),
        ..Default::default()
    };
    
    let result = eframe::run_native(
        "GOATd Kernel",
        options,
        Box::new(move |_cc| {
            Box::new(app_ui)
        }),
    );
    
    // =========================================================================
    // SHUTDOWN - LIFECYCLE MANAGEMENT
    // =========================================================================
    // Wait for log collector to empty all pending messages before shutdown
    if let Err(e) = log_collector.wait_for_empty().await {
        eprintln!("[Main] WARNING: Failed to wait for log collector to empty: {}", e);
    } else {
        eprintln!("[Main] ✓ Log collector confirmed empty, all logs persisted to disk");
    }
    
    eprintln!("[Main] Application shutting down.");
    
    result.map_err(|e| e.into())
}
