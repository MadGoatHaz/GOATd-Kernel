# GOATd Kernel Builder: Developer Guide

This guide provides architectural overview, code organization, testing standards, and contribution guidelines for developers working on GOATd Kernel Builder.

---

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Project Structure](#project-structure)
3. [Toolchain Requirements](#toolchain-requirements)
4. [Development Setup](#development-setup)
5. [Core Concepts](#core-concepts)
6. [Testing & Quality Assurance](#testing--quality-assurance)
7. [Building & Deployment](#building--deployment)
8. [Contributing](#contributing)
9. [Debugging Tips](#debugging-tips)

---

## Architecture Overview

GOATd Kernel Builder is built on a **pure Rust core** with an **egui immediate-mode reactive UI** and a **tokio async runtime** for non-blocking I/O orchestration.

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

6. **Non-Blocking Async**: egui UI remains responsive via `tokio::spawn_blocking` for expensive operations.

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
│   ├── modprobed.rs           # Modprobed-DB integration
│   ├── whitelist.rs           # 22-driver safety-net whitelist
│   ├── exclusions.rs          # Hardware-aware driver exclusion policies
│   ├── validator.rs           # Configuration validation contracts
│   └── mod.rs                 # Config module exports
│
├── kernel/
│   ├── patcher.rs             # KernelPatcher (5-phase enforcement)
│   ├── manager.rs             # Kernel package & lifecycle management
│   ├── audit.rs               # Deep-dive kernel introspection
│   ├── git.rs                 # Git source management via git2
│   ├── lto.rs                 # LTO type enums & validation
│   ├── pkgbuild.rs            # PKGBUILD parsing & manipulation
│   ├── sources.rs             # Kernel source detection & validation
│   ├── parser.rs              # Config file parser
│   ├── validator.rs           # Kernel config validation
│   └── mod.rs                 # Kernel module exports
│
├── orchestrator/
│   ├── mod.rs                 # AsyncOrchestrator (build phase coordinator)
│   ├── executor.rs            # Executor (pure runner, makepkg invocation)
│   ├── state.rs               # BuildPhaseState (state machine)
│   ├── checkpoint.rs          # Build resumption & recovery
│   └── mod.rs                 # Orchestrator module exports
│
├── hardware/
│   ├── init.rs                # Hardware detection initialization
│   ├── cpu.rs                 # CPU feature detection, -march optimization
│   ├── gpu.rs                 # GPU vendor detection (NVIDIA, AMD, Intel)
│   ├── ram.rs                 # System RAM sizing
│   ├── storage.rs             # Storage type detection (NVMe, SATA, etc.)
│   ├── boot.rs                # Bootloader detection (GRUB, systemd-boot, rEFInd)
│   └── mod.rs                 # Hardware module exports
│
├── system/
│   ├── mod.rs                 # System OS abstractions
│   ├── scx.rs                 # SCX scheduler service integration
│   └── performance/
│       ├── collector.rs       # Lock-free latency collection (rtrb ring buffer)
│       ├── scoring.rs         # 7-metric spectrum scoring (Latency, Consistency, Jitter, etc.)
│       ├── diagnostic.rs      # MSR-based SMI detection
│       ├── thermal.rs         # Per-core thermal monitoring
│       ├── context_switch.rs  # Context-switch efficiency measurement
│       ├── jitter.rs          # Jitter analysis & detection
│       ├── stressor.rs        # Stress test stressor (CPU, Memory, Scheduler)
│       ├── history.rs         # Performance result persistence (JSON/CSV)
│       ├── tuner.rs           # Sysctl optimization
│       ├── watchdog.rs        # Background monitoring & alerts
│       └── mod.rs             # Performance module exports
│
├── models.rs                  # Shared data structures (Profile, LtoType, etc.)
├── error.rs                   # AppError (unified error type)
├── policy.rs                  # Global policies (debug logging, etc.)
└── log_collector.rs           # Privileged dual-write logging (file + UI ring buffer)

tests/
├── integration_tests.rs       # End-to-end workflow tests
├── config_tests.rs            # Config loading & validation
├── hardware_tests.rs          # Hardware detection tests
├── profile_pipeline_validation.rs  # Profile application verification
├── build_pipeline_tests.rs    # Build orchestration tests
├── performance_*.rs           # Performance diagnostic tests
└── modprobed_*.rs             # Modprobed-DB integration tests
```

---

## Toolchain Requirements

### Host Development System

- **Rust**: 2021 Edition (1.70+)
- **Cargo**: Latest stable channel
- **Git**: For version control and git2 library

### Kernel Build System

- **Clang**: 16.0.0+ (enforced globally via `_FORCE_CLANG=1`)
- **LLVM Tools**: llvm-ar, llvm-nm, llvm-objcopy, etc.
- **Pacman**: Arch package manager (for dependency resolution)
- **makepkg**: Arch kernel packaging tool

### Optional (for Performance Diagnostics)

- **sysfs**: For thermal and SMI monitoring
- **MSR-Tools**: For SMI/TSC introspection
- **stress-ng**: For synthetic workload generation

---

## Development Setup

### 1. Clone the Repository

```bash
git clone <repo-url>
cd GOATd\ Kernel
```

### 2. Verify Rust Toolchain

```bash
rustc --version
cargo --version
```

Update if needed:
```bash
rustup update
```

### 3. Install Clang 16+ (Host System)

For Arch Linux:
```bash
sudo pacman -S clang llvm lld
clang --version  # Verify 16.0.0 or later
```

### 4. Build in Debug Mode

```bash
cargo build
```

Output: `target/debug/goatd_kernel`

### 5. Build Release Binary

```bash
cargo build --release
```

Output: `target/release/goatd_kernel`

### 6. Run Tests

```bash
cargo test --all-targets
```

Full test suite: 465+ tests covering all critical paths.

---

## Core Concepts

### AppState: The Source of Truth

**File**: [`src/config/mod.rs`](../src/config/mod.rs)

Stores all application state in a single `Arc<RwLock<AppState>>` struct:

```rust
pub struct AppState {
    // Hardware detection
    pub hardware: HardwareInfo,
    
    // User selections
    pub selected_kernel_variant: String,
    pub selected_profile: String,
    
    // Profile customizations (user overrides)
    pub use_modprobed: bool,
    pub use_whitelist: bool,
    pub lto_type: LtoType,
    
    // User intent flags (prevent profile re-forcing)
    pub user_toggled_bore: bool,
    pub user_toggled_mglru: bool,
    pub user_toggled_polly: bool,
    pub user_toggled_lto: bool,
    
    // Build progress
    pub build_phase: BuildPhaseState,
    pub build_progress: f32,
    
    // Performance diagnostics
    pub perf_metrics: PerformanceMetrics,
}
```

**Key Pattern**: `RwLock<T>` allows many readers concurrently, exclusive writer when mutations occur. This is critical for the egui event loop and async tasks to synchronize state without blocking.

### Finalizer: The Rule Engine

**File**: [`src/config/finalizer.rs`](../src/config/finalizer.rs)

Implements the hierarchical configuration resolution:

```
Hardware Truth (detected CPU, GPU, RAM)
  ↓
User Overrides (user_toggled_* flags)
  ↓
Profile Presets (Gaming, Workstation, Server, Laptop defaults)
  ↓
Final KernelConfig (passed to KernelPatcher)
```

**Method**: `Finalizer::finalize_kernel_config(state: &AppState) -> KernelConfig`

### KernelPatcher: The Surgical Engine

**File**: [`src/kernel/patcher.rs`](../src/kernel/patcher.rs)

ONLY module permitted to modify `PKGBUILD` and `.config`. Implements 5-phase hard enforcement:

#### Phase 5 (Orchestrator)
- **When**: During `AsyncOrchestrator::patch()` phase
- **What**: Regex-based removal of ALL GCC/LTO variants + atomic Clang/LTO injection
- **Why**: Establishes clean baseline before PKGBUILD scripts start

#### Phase G1 (Prebuild LTO Hard Enforcer)
- **When**: Injected into PKGBUILD `build()`, runs immediately before `make bzImage`
- **What**: Final surgical removal + atomic injection of LTO settings
- **Why**: Last gate before kernel compilation; locks in LTO after all config steps complete

#### Phase G2 (Post-Modprobed Hard Enforcer)
- **When**: After `make localmodconfig` filters modules to ~170
- **What**: Extracts filtered modules, runs olddefconfig, hard-restores original 170 modules
- **Why**: Prevents Kconfig dependency re-expansion from un-filtering modules

#### Phase G2.5 (Post-Setting-Config Restorer)
- **When**: After Arch's `cp ../config .config` overwrites filtered config with unfiltered
- **What**: Detects overwrite, re-applies modprobed filtering, restores BORE/MGLRU/Polly settings
- **Why**: Recovers from destructive cp operation that overwrites all customizations

#### Phase E1 (Post-Oldconfig LTO Re-Enforcement)
- **When**: After any `make oldconfig` / `make syncconfig` in prepare/build phases
- **What**: Re-applies LTO enforcement if Kconfig reverts to CONFIG_LTO_NONE=y
- **Why**: Kernel's Kconfig system may revert LTO during reconfig; this prevents it

### AsyncOrchestrator: The Conductor

**File**: [`src/orchestrator/mod.rs`](../src/orchestrator/mod.rs)

Coordinates build phases via state machine:

```
Preparation → Configuration → Patching → Building → Validation → Completed
```

Each phase delegates to specialized modules (Executor, KernelPatcher, etc.) but NEVER edits files directly.

**Key Method**: `AsyncOrchestrator::run_async()` spawns a tokio task that coordinates all phases.

---

## Testing & Quality Assurance

### Test Suite Overview

**465+ tests** covering:

1. **Unit Tests** (307): Individual function/module behavior
2. **Integration Tests** (122): Cross-module interaction
3. **Doc Tests** (36): Documentation code examples

### Running Tests

```bash
# All tests
cargo test --all-targets

# Specific test file
cargo test --test integration_tests

# Specific test
cargo test test_phase_h1_triple_lock_lto_enforcer -- --nocapture

# Show test output (useful for debugging)
cargo test -- --nocapture --test-threads=1
```

### Key Test Files

1. **`build_pipeline_tests.rs`**: Build Pipeline Verification test (dry-run validation)
2. **`profile_pipeline_validation.rs`**: Profile application & LTO enforcement
3. **`config_tests.rs`**: Configuration loading, whitelist validation
4. **`performance_battle_tests.rs`**: GOAT Score calculation verification
5. **`modprobed_localmodconfig_validation.rs`**: Modprobed-DB integration

### Deep Pipe Verification Test

**Pattern**: Use `GOATD_DRY_RUN_HOOK` to halt build at specific phases for testing without 45-minute compilation:

```rust
#[test]
fn test_phase_h1_triple_lock_lto_enforcer() {
    // Create mock kernel environment
    // Initialize AsyncOrchestrator with production code (zero mocking)
    // Run complete pipeline: Prep → Config → Patch → Build (halted)
    // Validate 7 critical assertions
    
    assert!(config_has_lto_clang_thin);
    assert!(bore_scheduler_present);
    assert!(mglru_enabled);
    // ... etc
}
```

This test **replaces manual `makepkg` runs** for configuration testing—no 45-minute compilation needed.

### Quality Gates

Before submitting a PR:

1. ✅ **All tests pass**: `cargo test --all-targets`
2. ✅ **No compiler warnings**: `cargo check --all-targets`
3. ✅ **Clippy clean**: `cargo clippy -- -D warnings`
4. ✅ **Format verified**: `cargo fmt --check`
5. ✅ **Documentation**: Inline code comments for non-obvious logic

---

## Building & Deployment

### Building the Release Binary

```bash
cargo build --release
```

Binary location: `target/release/goatd_kernel`

### Running the Application

```bash
./target/release/goatd_kernel
```

The egui window will launch. Application will request sudo credentials for kernel build and installation operations.

### Installing as System Binary

To make available system-wide:

```bash
sudo cp target/release/goatd_kernel /usr/local/bin/
```

Then run from anywhere:
```bash
goatd_kernel
```

---

## Contributing

### Code Style

- **Rust Edition**: 2021
- **Formatting**: Use `cargo fmt` (automatic)
- **Linting**: Pass `cargo clippy -- -D warnings`
- **Comments**: Non-obvious logic must be documented
- **Error Handling**: Use `Result<T, AppError>` consistently

### Adding a New Optimization Profile

1. **Define Profile** in [`src/config/profiles.rs`](../src/config/profiles.rs):
   ```rust
   Profile {
       name: "Custom",
       lto_type: LtoType::Thin,
       default_scheduler: "eevdf",
       // ... other settings
   }
   ```

2. **Add UI Control** in [`src/ui/build.rs`](../src/ui/build.rs):
   ```rust
   ComboBox::from_label("Profile")
       .selected_text(state.selected_profile.as_str())
       .show_ui(ui, |ui| {
           ui.selectable_value(&mut state.selected_profile, "Custom", "Custom");
       });
   ```

3. **Add Tests** in [`tests/profile_pipeline_validation.rs`](../tests/profile_pipeline_validation.rs):
   ```rust
   #[test]
   fn test_custom_profile_application() {
       // Verify profile defaults apply correctly
   }
   ```

4. **Update Documentation** in [`docs/USER_GUIDE.md`](USER_GUIDE.md):
   - Add profile description to "Optimization Profiles" section

### Adding a Performance Metric

1. **Define Metric** in [`src/system/performance/scoring.rs`](../src/system/performance/scoring.rs):
   ```rust
   pub struct PerformanceMetric {
       pub name: &'static str,
       pub value: f64,
       pub weight: f64, // 0.0–1.0, sum of all weights = 1.0
   }
   ```

2. **Implement Collection** in appropriate module:
   - Latency: [`src/system/performance/collector.rs`](../src/system/performance/collector.rs)
   - Thermal: [`src/system/performance/thermal.rs`](../src/system/performance/thermal.rs)
   - Jitter: [`src/system/performance/jitter.rs`](../src/system/performance/jitter.rs)

3. **Add Scoring Formula** in `scoring.rs`:
   ```rust
   fn normalize_latency(p99_us: f64) -> f64 {
       1.0 - ((p99_us - 10.0) / 490.0).clamp(0.001, 1.0)
   }
   ```

4. **Test Normalization** in test suite

5. **Update GOAT Score** calculation in `scoring.rs`:
   ```rust
   let goat_score = (
       latency_norm * 0.27 +
       consistency_norm * 0.18 +
       // ... other metrics
   ) * 1000.0;
   ```

---

## Debugging Tips

### Enabling Debug Logging

Set environment variable:
```bash
RUST_LOG=debug cargo run
```

Logs appear in build console and persistent log at: `logs/full/<timestamp>_full.log`

### Inspecting Kernel Config

After build, inspect `.config` in `/tmp/goatd-build-*/` directory:

```bash
# Find the build directory
ls -la /tmp/goatd-build-*/

# View .config
cat /tmp/goatd-build-*/linux-*/linux-*/.config | grep CONFIG_LTO
```

### Testing LTO Enforcement

Use Deep Pipe verification test:

```bash
cargo test test_phase_h1_triple_lock_lto_enforcer -- --nocapture
```

### Checking Modprobed-DB Filtering

```bash
# Check if modprobed.db exists
ls -la ~/.config/modprobed.db

# View contents (list of detected drivers)
cat ~/.config/modprobed.db | head -20
```

### Debugging Profile Application

Add temporary debug logging in [`src/config/finalizer.rs`](../src/config/finalizer.rs):

```rust
eprintln!("[DEBUG] Applying profile: {}", profile_name);
eprintln!("[DEBUG] User overrides: BORE={}", state.user_toggled_bore);
eprintln!("[DEBUG] Final config: LTO={:?}", final_config.lto_type);
```

### Inspecting AppState

In UI code, print to stderr:

```rust
eprintln!("[APP_STATE] Profile: {:?}", state.selected_profile);
eprintln!("[APP_STATE] Hardware GPU: {:?}", state.hardware.gpu_vendor);
eprintln!("[APP_STATE] Build phase: {:?}", state.build_phase);
```

---

## Architecture Decision Records (ADRs)

### ADR 1: Why Pure Rust + egui?

**Decision**: Replace Slint→CXX-Qt→Slint journey with pure Rust + egui.

**Rationale**:
- Single language eliminates FFI complexity
- No C++ runtime dependencies
- Immediate-mode UI paradigm suits reactive state management
- `eframe` handles platform abstraction

### ADR 2: Why Multi-Phase Hard Enforcer?

**Decision**: Implement 5-phase enforcement pipeline (G1, G2, G2.5, E1, Phase 5).

**Rationale**:
- Kernel's Kconfig system is adversarial—actively reverts settings
- Single enforcement gate insufficient; multiple degradation vectors
- Each phase targets specific Kconfig regeneration point
- Surgical approach (remove + inject) prevents conflicts

### ADR 3: Why Whitelist Instead of Default Include?

**Decision**: Whitelist critical 22 drivers instead of including all by default.

**Rationale**:
- Modprobed-DB goal is aggressive filtering for speed
- Default-include undermines filtering effectiveness
- Whitelist + modprobed = best of both worlds
- Users understand why each driver is present

---

## References

- **Main Documentation**: [`BLUEPRINT_V2.md`](../BLUEPRINT_V2.md) (canonical technical spec)
- **Development History**: [`DEVLOG.md`](../DEVLOG.md) (42+ phases of development)
- **Project Scope**: [`PROJECTSCOPE.md`](../PROJECTSCOPE.md) (architectural scope)
- **User Guide**: [`docs/USER_GUIDE.md`](USER_GUIDE.md) (end-user documentation)

---

## Support & Discussion

For architectural questions or implementation clarifications, refer to the corresponding Phase entry in [`DEVLOG.md`](../DEVLOG.md) where the feature was originally designed and implemented.
