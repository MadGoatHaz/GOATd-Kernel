# Build Pipe Testing & Diagnostics

This document covers the automated build pipe testing infrastructure, including the timeout mechanisms, diagnostic logging, and the 10-phase implementation plan.

## Orchestrator Timeouts

The `AsyncOrchestrator` includes a `test_timeout` field of type `Option<Duration>`. This is primarily used in automated tests to prevent hanging builds and to trigger diagnostic output when a build exceeds expected time limits.

### How it Works
1. When creating an `AsyncOrchestrator` via `new()`, a `test_timeout` can be provided.
2. This timeout is passed down to the `executor::run_kernel_build` function.
3. If the build process (typically `makepkg`) exceeds this duration, the orchestrator terminates the process and returns a timeout error.

### Example Usage in Tests
```rust
let timeout = Duration::from_secs(300); // 5 minute timeout
let orch = AsyncOrchestrator::new(
    hardware, 
    config, 
    checkpoint_dir, 
    kernel_path, 
    None, 
    cancel_rx, 
    Some(log_collector), 
    Some(timeout)
).await?;
```

---

## LogCollector Diagnostics

To aid in debugging timed-out or failed builds, the `LogCollector` captures the most recent output lines in a sliding window buffer.

### Diagnostic Methods
- `get_last_output_lines(n: usize)`: Returns the last `n` lines of build output.
- `format_last_output_lines()`: Returns a pre-formatted string containing the last 10 lines, prefixed with `[LOG-CAPTURE]` for easy identification in test logs.

### Why it Matters
When a build times out in a headless CI environment, knowing the last few lines of output is critical for determining if the build was genuinely stuck, or just taking longer than expected (e.g., during a slow link step).

---

## 10-Phase Implementation Summary

The "Full Build Pipe" test features were implemented across 10 distinct phases to ensure system stability and modularity.

| Phase | Focus | Outcome |
|-------|-------|---------|
| 1 | Baseline Integration | Established the core async orchestration loop. |
| 2 | Diagnostic Buffer | Implemented the `last_output_lines` buffer in `LogCollector`. |
| 3 | Timeout Logic | Wired `test_timeout` through the Orchestrator to the Executor. |
| 4 | Cancellation Safety | Refined `cancel_rx` handling during long-running builds. |
| 5 | Resource Monitoring | Integration of thermal and CPU metrics during build. |
| 6 | Failure Recovery | Checkpoint system validation for resumed builds. |
| 7 | Mock Environment | Created lightweight mock kernel sources for rapid testing. |
| 8 | Build Pipe Verifier | Implemented comprehensive `tests/real_kernel_build_integration.rs`. |
| 9 | Stress Testing | Validated orchestrator behavior under heavy system load. |
| 10 | Contextual Logging | Finalized `format_last_output_lines` for automated reports. |

### Outcomes
- **Zero-Hang CI**: Automated tests now reliably fail with context rather than timing out the CI runner.
- **Forensic Capability**: Failures include the exact tail of the build log, even if the main log file is locked or incomplete.
- **Verification Velocity**: The mock kernel environment allows testing the *logic* of the 5-phase pipeline in seconds rather than hours.

---

## Extending Tests

To add a new build pipe test:
1. Create a new test file in `tests/`.
2. Use the `AsyncOrchestrator` with a short `test_timeout`.
3. If the test fails, call `log_collector.format_last_output_lines()` to display the failure context.
4. Reference `tests/real_kernel_build_integration.rs` for a template of a full-pipe test.
