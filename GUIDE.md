# GOATd Kernel Guide (v0.2.1)

Welcome to the GOATd Kernel dual-track manual. This document provides both user-centric operational instructions and developer-centric logic details.

---

## 🟢 User Track: Operation & Optimization

### 1. Installation & Setup
The project is managed via a central wrapper script: `goatdkernel.sh`.

- **Bootstrap**: Run `./goatdkernel.sh` to initialize the environment.
- **Dependencies**: The system requires `rust`, `cargo`, `git`, and `base-devel`.
- **Polkit Integration**: GOATd Kernel uses Polkit for privileged operations (build, provision, install).
    - Policy file: `assets/com.goatd.kernel.policy`
    - Actions: `com.goatd.kernel.build`, `com.goatd.kernel.provision`, `com.goatd.kernel.install`.
    - Authorization happens automatically via `pkexec` when the GUI triggers protected backend tasks.

### 2. Kernel Variant Selection
Navigate to the **Kernels** tab to select your base source:
- **Mainline RC**: The absolute latest upstream Release Candidate code. Recommended for users who want the bleeding edge of Linux development and are willing to test experimental features.
- **Linux / LTS**: Standard Arch Linux kernel options for daily stability and reliability.
- **Hardened**: Focused on security with additional sanity checks and restricted features.
- **Zen (Experimental)**: *Note: The Zen variant is currently omitted from the UI selection as it is undergoing maintenance and is not production-ready in this build.*

### 3. Performance Dashboard Interpretation
The **Performance** tab provides real-time telemetry:
- **KPI Cards**: High-level metrics for quick health checks.
- **Performance Spectrum**: Seven horizontal metric strips visualizing system behavior across different dimensions (jitter, thermal, context switches, etc.).
- **Scoring**: The system uses `system::performance::scoring` to provide an objective health score based on your hardware profile.

---

## 🛠️ Developer Track: Architecture & Logic

### 1. Kernel Source Management (`src/kernel/sources.rs`)
The `KernelSourceDB` is the central registry for kernel variants.
- **`KernelVariant` Enum**: Maps UI selections to upstream git URLs.
- **`KernelSource` Struct**: Defines the metadata for each variant (URL, branch, etc.).
- **Discovery**: The `KernelSourceDB` handles polling for the latest versions from remote PKGBUILDs.

### 2. UI & Backend Loop (`src/ui/controller.rs` ↔ `src/ui/app.rs`)
GOATd Kernel utilizes an event-driven bridge between the Egui frontend and the Tokio-powered backend.
- **`AppController`**: The primary interface for the UI to trigger backend actions. It manages shared state via `Arc<RwLock<AppState>>`.
- **`BuildEvent` Loop**: Background build tasks emit `BuildEvent` notifications. The UI listens for these events to update progress bars and status logs in real-time.
- **State Synchronization**: `AppUI::update` (in `src/ui/app.rs`) periodically checks for backend state changes to ensure the UI remains reactive.

### 3. Hardware Detection (`src/hardware/mod.rs`)
The `HardwareDetector` provides a unified interface for system discovery.
- **Pattern**: Implements `HardwareDetector` with internal caching to avoid redundant syscalls.
- **Extension**: To add new hardware support (e.g., specialized NPU detection), extend the `HardwareDetector` struct and its implementation in `src/hardware/`.

### 4. Sched-ext (SCX) Infrastructure (`src/system/scx.rs`)
The `SCXManager` and `PersistentSCXManager` handle BPF-based schedulers.
- **`SchedulerMode`**: Maps logical modes (Gaming, PowerSave) to specific BPF scheduler configurations.
- **Provisioning**: Atomic command chaining ensures schedulers are correctly loaded and verified before activation.

---

## 🔍 Verification
- **Build Logic**: Verified against `src/ui/build.rs` and `src/ui/controller.rs`.
- **Hardware Logic**: Matches `src/hardware/mod.rs` implementation.
- **Source Logic**: Aligned with `KernelSourceDB` in `src/kernel/sources.rs`.
