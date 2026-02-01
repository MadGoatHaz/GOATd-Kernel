# GOATd Kernel Builder: Developer Guide (v0.2.1)

This guide provides the architectural overview, code organization, testing standards, and implementation details for GOATd Kernel Builder.

---

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Project Structure](#project-structure)
3. [Toolchain Requirements](#toolchain-requirements)
4. [Dependency Management](#dependency-management)
5. [Development Setup](#development-setup)
6. [Core Concepts](#core-concepts)
7. [Testing & Quality Assurance](#testing--quality-assurance)
8. [Building & Deployment](#building--deployment)
9. [Debugging Tips](#debugging-tips)

---

## Architecture Overview

I've built GOATd Kernel Builder on a **pure Rust core** with an **egui immediate-mode reactive UI** and a **tokio async runtime** for non-blocking I/O orchestration.

### High-Level Layers

```
┌─────────────────────────────────────────────────────┐
│           egui Reactive UI (Immediate-Mode)         │
│  Dashboard | Build | Performance | Kernel Mgr       │
├─────────────────────────────────────────────────────┤
│        AppController (Intent Broker)                │
│  Bridges UI events to AppState updates via tokio   │
├─────────────────────────────────────────────────────┤
│    AppState (In-Memory Source of Truth)            │
│  Hardware info, profiles, build config, UI state   │
├─────────────────────────────────────────────────────┤
│   AsyncOrchestrator (Phase Transition Manager)      │
│  Coordinates prep → config → patch → build → verify │
├─────────────────────────────────────────────────────┤
│     KernelPatcher (Unified Surgical Engine)         │
│   Only module permitted to modify PKGBUILD/.config │
├─────────────────────────────────────────────────────┤
│     System Layer (OS Wrappers & Commands)           │
│  Package management, process control, file I/O      │
├─────────────────────────────────────────────────────┤
│    Hardware Detection & Performance Diagnostics     │
│  CPU features, GPU detection, thermal monitoring    │
└─────────────────────────────────────────────────────┘
```

### Architectural Principles

1. **User Intent is Sovereign**: UI toggles persist through `user_toggled_*` flags in `AppState`, preventing profile defaults from overriding user choices.

2. **Hierarchical Truth**: Configuration resolved via: **Hardware Truth > User Override > Profile Preset**.

3. **Unified Surgical Engine**: ONLY `KernelPatcher` modifies `PKGBUILD` and `.config`. Orchestrator delegates; never edits files directly.

4. **Multi-Phase Hard Enforcement**: 5-phase protection pipeline (G1, G2, G2.5, E1, Phase 5) ensures kernel configuration survives all aggressive Kconfig regeneration.

5. **Modular Responsibility**: Each layer has a single, clear purpose.

6. **Non-Blocking Async**: the egui UI remains responsive via `tokio::spawn_blocking` for expensive operations.

---

## Project Structure

```
src/
├── main.rs                    # Entry point, egui app lifecycle
├── lib.rs                     # Library root, public API exports
│
├── ui/
│   ├── app.rs                 # AppUI orchestrator, tab routing
│   ├── controller.rs          # AppController, intent broker
│   ├── dashboard.rs           # Dashboard tab (system overview)
│   ├── build.rs               # Build tab (kernel compilation UI)
│   ├── kernels.rs             # Kernel Manager tab
│   ├── performance.rs         # Performance dashboard with 7-metric spectrum
│   ├── settings.rs            # Settings tab (global preferences)
│   ├── widgets.rs             # Custom egui widgets
│   ├── threading.rs           # UI heartbeat & invalidation callbacks
│   └── mod.rs                 # UI module exports
│
├── config/
│   ├── mod.rs                 # AppState (source of truth)
│   ├── loader.rs              # Configuration persistence (JSON)
│   ├── profiles.rs            # 4-profile system (Gaming, WS, Server, Laptop)
│   ├── finalizer.rs           # Finalizer (rule engine, Hardware > Override > Profile)
│   ├── validator.rs           # AppState validation logic
│   ├── exclusions.rs          # GPU driver auto-exclusion logic
│   ├── modprobed.rs           # Modprobed-DB integration
│   └── whitelist.rs           # Desktop Experience Whitelist (22 critical drivers)
│
├── kernel/
│   ├── manager.rs             # Kernel lifecycle (build, install, remove)
│   ├── sources.rs             # KernelSourceDB (git/PKGBUILD URLs)
│   ├── pkgbuild.rs            # PKGBUILD parsing and versioning
│   ├── parser.rs              # .config parsing
│   ├── patcher/               # Surgical config/PKGBUILD modification
│   └── mod.rs                 # Kernel module exports
│
├── hardware/
│   ├── cpu.rs                 # CPU feature detection (AVX-512, etc.)
│   ├── gpu.rs                 # GPU vendor detection (NVIDIA/AMD/Intel)
│   ├── storage.rs             # Drive & FS detection
│   ├── boot.rs                # Bootloader detection (GRUB/systemd-boot)
│   └── mod.rs                 # Hardware detection engine
│
├── system/
│   ├── performance/           # Gauntlet benchmark & live telemetry
│   ├── scx.rs                 # Sched_ext (SCX) BPF management
│   ├── verification.rs        # GPG & SHA256 verification
│   ├── paths.rs               # Global path management
│   └── mod.rs                 # System-level OS wrappers
│
├── orchestrator/
│   ├── executor.rs            # Async build pipeline orchestrator
│   ├── state.rs               # Orchestrator-specific state machine
│   └── mod.rs                 # Orchestration module
│
├── models.rs                  # Shared data structures (KernelConfig, Profile)
├── log_collector.rs           # Centralized logging subsystem
└── error.rs                   # Unified error handling (AppError)
```

---

## AppState Lifecycle (`src/config/mod.rs`)

The `AppState` struct is the central source of truth for the entire application. It is held in an `Arc<RwLock<AppState>>` to allow thread-safe access from both the UI and background workers.

### Core Fields (v0.2.1)

```rust
pub struct AppState {
    // Build Settings
    pub selected_variant: String,
    pub selected_profile: String,
    pub selected_lto: String,
    pub selected_scx_profile: String,
    pub selected_scx_mode: String,
    pub kernel_hardening: HardeningLevel,
    pub secure_boot: bool,
    pub use_modprobed: bool,
    pub use_whitelist: bool,
    pub use_polly: bool,
    pub use_mglru: bool,

    // Override Tracking Flags (Persist user intent)
    pub user_toggled_lto: bool,
    pub user_toggled_polly: bool,
    pub user_toggled_mglru: bool,
    pub user_toggled_bore: bool,
    pub user_toggled_hardening: bool,

    // Global Settings
    pub theme_mode: String,
    pub theme_idx: usize,
    pub ui_font_size: f32,
    pub check_for_updates: bool,
    pub verify_signatures: bool,
    pub startup_audit: bool,
    pub audit_on_startup: bool, // Deprecated/Alias sync
    pub native_optimizations: bool,

    // Performance & Monitoring
    pub perf_background_enabled: bool,
    pub perf_alert_threshold_us: f32,
    pub debug_logging: bool,
    pub tokio_tracing: bool,
    pub auto_scroll_logs: bool,
    pub show_fps: bool,

    // Paths
    pub workspace_path: String,
    pub kernel_source_path: String,

    // ... and others
}
```

### Intent Persistence Logic

When a user manually changes a setting in the UI (e.g., toggles LTO), the corresponding `user_toggled_*` flag is set to `true`. The `Finalizer` (in `src/config/finalizer.rs`) checks these flags before applying profile defaults. If a toggle flag is active, the user's choice is preserved regardless of what the selected Profile (Gaming, Server, etc.) recommends.

---

## Development Setup

### 1. Toolchain
- **Rust**: `rustup default stable`
- **LLVM/Clang**: `sudo pacman -S llvm clang lld polly` (on Arch)

### 2. Running in Development Mode
```bash
cargo run
```

### 3. Running Tests
```bash
# Unit tests
cargo test

# Integration tests (requires privileged access for some)
cargo test --test '*'
```

---

## Debugging Tips

### Enabling Debug Logging
Set environment variable:
```bash
RUST_LOG=debug cargo run
```

### Inspecting AppState
In UI code, I often print to stderr for quick checks:
```rust
eprintln!("[APP_STATE] Profile: {:?}", state.selected_profile);
eprintln!("[APP_STATE] User overrides: LTO={}", state.user_toggled_lto);
```

### Inspecting Kernel Config
After a build attempt, you can inspect the generated `.config` in the `/tmp/goatd-build-*/` directory to verify that your optimizations were correctly injected by the enforcer pipeline.

---

## References
- **User Guide**: [`docs/USER_GUIDE.md`](USER_GUIDE.md)
- **Technical Spec**: [`BLUEPRINT_V2.md`](../BLUEPRINT_V2.md)
- **History**: [`DEVLOG.md`](../DEVLOG.md)
