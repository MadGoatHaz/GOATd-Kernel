# Controller Modularization Map

This document tracks the analysis and modularization of `src/ui/controller.rs`.

## Analysis Passes

| Range | Logic Description | Status | Target Module |
|-------|-------------------|--------|---------------|
| 1-500 | Imports, `ComparisonResult`, `BuildEvent`, `AppController` struct, `new_async` initialization, state persistence, and version polling. | Analyzed | |
| 501-1000 | Profile/Configuration management, override handlers, SCX config application, and the primary `start_build` / `cancel_build` orchestration logic. | Analyzed | |
| 1001-1500 | Kernel artifact management, variant mapping, robust version resolution (DKMS fix logic), and `install_kernel_async` orchestration with unified privilege execution. | Analyzed | |
| 1501-2000 | Unified installation batch (pacman + headers discovery + DKMS), post-install verification, diagnostic guidance for failures, and package path/naming utility helpers. | Analyzed | |
| 2001-2500 | Kernel version extraction/normalization from filenames, and Performance Monitoring Lifecycle (thread management, real-time tuning, high-speed telemetry collection, and specialized collectors like MicroJitter). | Analyzed | |
| 2501-3000 | Performance Telemetry Hub: specialized collectors (syscall/context-switch), hardware poller isolation (blocking I/O), and the main `background_processor_task` (lock-free buffer drainage, noise floor calibration, and KPI calculation). | Analyzed | |
| 3001-3500 | Telemetry/Monitoring logic continuation: `system_monitor_task` (passive drainage, real-time KPI processing), thermal/jitter tracking, metric preservation, and UI synchronization with lock-free buffer management. | Analyzed | |
| 3501-4000 | `benchmark_runner_task` (async results collection), kernel context management (refresh logic with sysfs/procfs), CPU telemetry helpers (governor, freq, usage), and Slint UI monitoring trigger handlers with "Deep Reset" logic. | Analyzed | |
| 4001-4500 | Performance Monitoring Lifecycle: Stressor initialization, routing for Benchmark vs Continuous modes, and the 6-phase "GOATd Full Benchmark" orchestration (MicroJitter, ContextSwitch, Syscall saturation, and scoring). | Analyzed | |
| 4501-5000 | Monitoring session finalization (summary creation, snapshot history), continuous monitoring branch, `internal_stop_monitoring_impl` cleanup, `finalize_session_summary` consolidation, and UI handlers for stopping monitoring and toggling stressors. | Analyzed | |
| 5001-5500 | Performance metrics management, cycle timers, historical comparison (percentage deltas), record persistence (save/delete), background alert state, hardware/audit caching (30s TTL), and `apply_phase_stressors` entry. | Analyzed | |
| 5501-EOF | Completion of `apply_phase_stressors` (intensity-based stressor starts with error handling), thread-safe 60s TTL cached system health reporting (`get_cached_health_report`), and `AuditImpl` / `AuditTrait` implementation (mapping deep audits, jitter audits, and performance metrics to underlying systems). | Analyzed | |
