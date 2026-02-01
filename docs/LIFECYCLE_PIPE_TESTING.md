# Lifecycle Pipe Testing & Multi-Instance Logging Architecture

This document describes the hardened logging subsystem that enables parallel and sequential testing without global state conflicts through the **GlobalLogDispatcher** architecture.

---

## Overview

The GOATd Kernel logging system supports **multiple concurrent LogCollector instances** through a synchronized dispatcher registry. This enables:

- ✅ **Isolation**: Each test has a dedicated LogCollector and log files.
- ✅ **Parallel Testing**: Multiple test suites run concurrently without log cross-contamination.
- ✅ **Sequential Testing**: Series of tests reuse the same global logger without re-initialization errors.
- ✅ **Proper Cleanup**: Automatic unregistration via the Drop trait prevents memory leaks and stale references.
- ✅ **Synchronized Session Creation**: Async `start_new_session()` with oneshot ack guarantees ordering.

---

## Architecture: GlobalLogDispatcher

### Core Components

```
┌─────────────────────────────────────────────────────────────────┐
│                    GOATd Logging Pipeline                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  [Logging Macros: log::info!(), log_info!()]                   │
│         ↓                                                        │
│  [LogCollector #1]  [LogCollector #2]  [LogCollector #N]      │
│    (Test 1)          (Test 2)           (Test N)              │
│         ↓                ↓                   ↓                  │
│  ┌─────────────────────────────────────────────────┐           │
│  │  GlobalLogDispatcher Registry (Arc<Mutex>)      │           │
│  │  ┌────────────────────────────────────────────┐ │           │
│  │  │ ID 1 → Sender<LogMessage>  (crossbeam)   │ │           │
│  │  │ ID 2 → Sender<LogMessage>  (crossbeam)   │ │           │
│  │  │ ID N → Sender<LogMessage>  (crossbeam)   │ │           │
│  │  └────────────────────────────────────────────┘ │           │
│  └─────────────────────────────────────────────────┘           │
│         ↓         ↓         ↓                                   │
│  ┌─────────────────────────────────────────────────┐           │
│  │        Background Persister Threads             │           │
│  │        (std::thread, not tokio)                 │           │
│  └─────────────────────────────────────────────────┘           │
│    ↓              ↓              ↓                              │
│  [DiskPersister] [DiskPersister] [DiskPersister]              │
│   Test 1 logs    Test 2 logs     Test N logs                  │
│         ↓              ↓              ↓                         │
│  [Session State] [Session State] [Session State]              │
│  (generation)    (generation)    (generation)                 │
│         ↓              ↓              ↓                         │
│  /tmp/.../full/  /tmp/.../full/  /tmp/.../full/              │
│  *.log (files)   *.log (files)    *.log (files)               │
│                                                                │
└─────────────────────────────────────────────────────────────────┘
```

### Key Features

#### 1. **Global Dispatcher Registry**

```rust
// From src/log_collector.rs
static GLOBAL_LOG_DISPATCHER: std::sync::OnceLock<
    Arc<std::sync::Mutex<HashMap<u64, Sender<LogMessage>>>>
> = std::sync::OnceLock::new();
```

- **OnceLock**: Ensures thread-safe initialization on first use.
- **Arc<Mutex>**: Allows concurrent lookups by multiple threads/tasks.
- **HashMap<u64, Sender>**: Maps unique collector IDs to their message channels.

#### 2. **Unique Collector IDs**

Each LogCollector receives a unique ID on creation:

```rust
static COLLECTOR_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

let id = COLLECTOR_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
```

- **AtomicU64**: Guarantees zero ID collision across parallel tests.

---

## Orchestrator Phases

The lifecycle follows modularized build pipeline stages as defined in [`src/orchestrator/phases/mod.rs`](src/orchestrator/phases/mod.rs):

- **Phase 1: Preparation** (`prep`): Hardware validation and environment setup.
- **Future Phases**: Validation, patching, execution, and finalization.

---

## Testing Guidelines

### For Developers Writing Tests

#### Single-LogCollector Tests

```rust
#[tokio::test]
async fn test_my_feature() -> Result<(), Box<dyn std::error::Error>> {
    let log_dir = std::env::temp_dir().join("my_test_logs");
    let (ui_tx, _ui_rx) = tokio::sync::mpsc::channel(256);
    
    let log_collector = Arc::new(
        LogCollector::new(log_dir, ui_tx)?
    );
    
    // Try to initialize, but handle gracefully if already done
    let _ = log_collector.clone().init_global_logger(log::LevelFilter::Info);
    
    // Create a session
    let session = log_collector.start_new_session("my_test.log").await?;
    
    log_collector.log_str("[MYTEST] Testing feature X");
    
    // ... test logic ...
    
    // Flush before verification
    log_collector.wait_for_empty().await?;
    
    // Read and verify logs
    let content = std::fs::read_to_string(&session)?;
    assert!(content.contains("[MYTEST]"));
    
    Ok(())
}
```

#### Multiple LogCollectors in Sequence

```rust
#[tokio::test]
async fn test_multiple_sessions() -> Result<(), Box<dyn std::error::Error>> {
    // Session 1
    {
        let log_collector = Arc::new(LogCollector::new(dir1, ui_tx.clone())?);
        let _ = log_collector.clone().init_global_logger(log::LevelFilter::Info);
        let path1 = log_collector.start_new_session("session1.log").await?;
        // ... test logic ...
        log_collector.wait_for_empty().await?;
    } // LogCollector drops, unregisters from dispatcher
    
    // Session 2 (same global logger, different LogCollector)
    {
        let log_collector = Arc::new(LogCollector::new(dir2, ui_tx.clone())?);
        let _ = log_collector.clone().init_global_logger(log::LevelFilter::Info);
        let path2 = log_collector.start_new_session("session2.log").await?;
        // ... test logic ...
        log_collector.wait_for_empty().await?;
    } // LogCollector drops, unregisters from dispatcher
    
    Ok(())
}
```

### Key Patterns

| Pattern | Usage | Notes |
|---------|-------|-------|
| **Graceful Logger Init** | `let _ = log_col.init_global_logger(...);` | Ignore errors if already set |
| **Async Session Creation** | `log_col.start_new_session("name.log").await?` | Always await; ensures execution order |
| **Flush Before Verify** | `log_col.wait_for_empty().await?` | Guarantees disk sync completion |
| **Per-Test Cleanup** | Scope-exit Drop | Automatic unregistration |

---

## Thread Safety Summary

| Component | Thread Safety | Mechanism |
|-----------|---------------|-----------|
| GlobalLogDispatcher | ✅ Yes | Arc<Mutex<HashMap>> |
| OnceLock initialization | ✅ Yes | OnceLock (single-initialization pattern) |
| COLLECTOR_ID_COUNTER | ✅ Yes | AtomicU64 with SeqCst ordering |
| LogMessage channel | ✅ Yes | Crossbeam unbounded (thread-safe) |
| SessionState | ✅ Yes | Arc<Mutex<>> |
| Background persister | ✅ Yes | Single-threaded IO per collector |

---

## Troubleshooting

### Issue: "Logger already initialized"
A previous test already called `init_global_logger()`. Handle gracefully:
```rust
let init_result = log_collector.clone().init_global_logger(log::LevelFilter::Info);
if init_result.is_err() {
    // Already initialized
}
```

### Issue: Logs not appearing in session file
Session not created, or logs written before session setup. Always create the session first.

---

## References

- [`src/log_collector.rs`](relative/src/log_collector.rs) - Full implementation
- [`src/orchestrator/phases/mod.rs`](relative/src/orchestrator/phases/mod.rs) - Phase definitions
- [`tests/lifecycle_pipe_integration.rs`](relative/tests/lifecycle_pipe_integration.rs) - Complete test suite
- [`docs/BUILD_PIPE_TESTING.md`](relative/docs/BUILD_PIPE_TESTING.md) - Build pipe diagnostics

---

**Architecture Phase**: Hardened Multi-Instance Dispatcher (Final)
