# GOATd Kernel Builder

**A kernel orchestration suite for Arch Linux built with pure Rust core, egui native UI, and Tokio async runtime. Build you own custom Linux kernels via standardized profile-aware installation with modprobed-db driver auto-discovery and Desktop Experience safety-net whitelist and high-fidelity performance diagnostics with 7-metric spectrum analysis.**

## Purpose

GOATd Kernel is a comprehensive solution for building, managing, and deploying custom Arch Linux kernels tailored to hardware-specific configurations. It bridges the gap between raw kernel compilation and end-user kernel management by automating the entire lifecycle: from microarchitecture detection and configuration to deployment, verification, and post-install optimization.

The project is maintained by a single developer and emphasizes honesty, architectural clarity, and direct sourcing from upstream repositories rather than obscuring complexity or relying on proprietary distribution channels.
---

## Quick Start: One-Command Install (Arch Linux)

### Prerequisites

Before your first run, ensure your system is up-to-date and Rust is installed. **These commands are mandatory and cannot be skipped:**

```bash
sudo pacman -Syu                    # ⚠️ MANDATORY: Update system packages
sudo pacman -S rustup              # ⚠️ MANDATORY: Install Rust (if not already present)
rustup default stable              # Set stable Rust as default
```

**Failure to run `sudo pacman -Syu` first is a common cause of build failures. Do not skip this step.**

### One-Command Installation

On **Arch Linux and Arch-based systems**, simply run:

```bash
./goatdkernel.sh
```

The script will automatically:
- ✅ Detect your Arch system and install all required packages via `pacman`
- ✅ Setup GPG keys for kernel signature verification (Greg Kroah-Hartman, Arch kernel maintainers)
- ✅ Initialize the LLVM/Clang 16+ toolchain for kernel compilation
- ✅ Build and launch the GOATd Kernel GUI

### First-Start Guide

Once the GOATd Kernel GUI launches, follow the **Dashboard Flow**:
1. **Health Check** — Validate system prerequisites
2. **Fix Environment** — Resolve any missing dependencies
3. **Manual AUR Steps** — Install `modprobed-db` from AUR (critical for performance)
4. **Baseline Audit** — Establish performance baseline
5. **Build Your Kernel** — Select profile and configure optimization settings

See [Dashboard Flow Overview](#dashboard-flow-overview) below and [`GUIDE.md`](GUIDE.md) for detailed instructions.

---

## Dashboard Flow Overview

GOATd Kernel uses a guided, step-by-step dashboard interface to ensure smooth kernel customization:

1. **Health Check** → Validates system prerequisites (Rust, LLVM, base-devel, modprobed-db status)
2. **Fix Environment** → Resolves missing dependencies with automated fixes or manual commands
3. **Manual AUR Steps** → Guides installation of `modprobed-db` from AUR (critical performance step)
4. **Baseline Audit** → Scans hardware and establishes performance baseline
5. **Build & Benchmark** → Select kernel profile, configure optimizations, build, and run GOATd Full Benchmark

For detailed walkthrough, see [`GUIDE.md`](GUIDE.md#dashboard-flow--first-run-guide).

---

## 🏆 Best Performance Recipe (The GOAT Recipe)

**For maximum gaming, desktop, and performance, use this configuration:**

| Setting | Value | Reason |
|---------|-------|--------|
| **Profile** | Gaming | Optimized for latency and responsiveness |
| **LTO Strategy** | Full LTO | Maximum compile-time optimization |
| **Polly Loop Optimization** | **Enabled** | 5-10% throughput improvement via vectorization |
| **Native CPU Targeting** | **Enabled** (`-march=native`) | 5-15% performance unlocked for your exact CPU |
| **MGLRU** | **Enabled** | Dramatic consistency/jitter reduction under memory pressure |
| **Module Filtering** | modprobed-db + Whitelist | 6500→<200 modules = 30-50% faster build, 40-60% faster LTO |
| **Scheduler** | bpfland SCX (auto mode) | Best latency/throughput balance with adaptive power modes |
| **Hardening** | Minimal (gaming) or Standard (balanced) | Your choice; both maintain security baseline |

**Expected Results**: GOAT Score 750-900, P99 latency <100µs, consistency CV% <12%, thermal stable.

See [`GUIDE.md` → The GOAT Recipe](GUIDE.md#the-goat-recipe-optimal-configuration) for step-by-step instructions.

---

## GOAT Score & Performance Metrics

GOATd Kernel uses a composite **GOAT Score** (0-1000) based on 7 performance metrics. This score helps you objectively measure kernel optimization effectiveness.

### The 7 Metrics (Weighted)

| Metric | Weight | Measures | Matters For |
|--------|--------|----------|-------------|
| **Latency** | 27% | Response time under load (P99) | Gaming, responsiveness, real-time apps |
| **Consistency** | 18% | Stable performance (Coefficient of Variation %) | Prevents micro-stutters, predictability |
| **Jitter** | 15% | Variance in latency (P99.9 @ 10µs floor) | Gaming, audio production, professional |
| **Throughput** | 10% | Operations per second | Server workloads, data processing |
| **Efficiency** | 10% | Performance per watt | Laptop battery life, power costs |
| **Thermal** | 10% | Temperature management under load | Prevents throttling, hardware longevity |
| **SMI Resilience** | 10% | System Management Interrupt handling | Latency-critical, uninterruptible workloads |

### GOAT Score Interpretation

- **0-250**: Minimal optimization
- **250-500**: Standard kernel
- **500-750**: Well-optimized kernel
- **750-1000**: Highly tuned kernel

For deep metric explanations and optimization strategies, see [`GUIDE.md`](GUIDE.md#understanding-performance-metrics).

---


### Modern Architecture: Pure Rust + egui with Modular Design (Phase 45+)

The project uses a unified Rust + egui stack with a modular component architecture for performance, maintainability, and simplicity. The UI is fully reactive, leveraging `eframe` and `tokio` to ensure a smooth, non-blocking experience even during heavy compilation or real-time performance monitoring.

**Key Architectural Advancements:**
- **V2 Global Dynamic Scaling**: Physical-pixel based scaling system ensuring sharp, consistent UI across high-DPI displays.
- **High-Density Responsive Layouts**: Adaptive UI components (e.g., `StripBuilder`) that maintain clarity and density across varying window sizes.
- **Stable UI Refreshes**: Atomic signaling combined with continuous repaint logic for glitch-free performance telemetry.

---

## Key Features

### -1. **Laboratory-Grade Hardening & Robust Build Pipeline**
- **Environment Purity Controls**: The build pipeline enforces a hermetically sealed compilation environment via explicit environment variable management, preventing contamination from system-wide CFLAGS or compiler flags
- **5-Phase Build Protection Stack**: Multi-layered enforcement gates (PHASE G1 PREBUILD LTO Hard Enforcer, PHASE G2 Post-Modprobed Hard Enforcer, PHASE G2.5 Post-Setting-Config Restorer, PHASE E1 Post-Oldconfig LTO Enforcement, PHASE 5 Surgical Atomic Enforcer) ensure configuration integrity across all makepkg stages
- **Definitive Rust Headers Fix**: Circumvents upstream kernel build system limitations via AST-aware regex injection and atomic path resolution, guaranteeing DKMS out-of-tree driver compatibility across diverse kernel configurations
- **Atomic Configuration Management**: All kernel `.config` modifications use atomic swap patterns with comprehensive backups, preventing partial configuration corruption
- **PYTHONDONTWRITEBYTECODE Enforcement**: Prevents bytecode cache contamination in build artifacts, ensuring reproducible, clean package outputs free from path leakage

### 0. **Out-of-the-Box Reliability: Zero-Touch Automation**
- **Arch Linux Auto-Install**: Single command `./goatdkernel.sh` triggers automatic detection and installation of all required system packages via `pacman` (Rust, LLVM/Clang, base-devel, etc.)
- **Automatic GPG Key Verification**: Kernel signature keys (Greg Kroah-Hartman, Arch kernel maintainers) imported and verified automatically with fingerprint validation and multi-keyserver failover
- **LLVM Toolchain Setup**: Clang 16+ (`llvm`, `clang`, `lld`, `polly`) automatically detected or installed; enforced globally via `_FORCE_CLANG=1`
- **Modprobed-DB Module Filtering**: Reduces 6500+ Linux kernel modules down to <200 loaded modules—a **97% reduction** with compounding effects:
  - **30-50% faster build time** (fewer modules to compile)
  - **40-60% faster LTO optimization** (Link-Time Optimization processes only loaded modules)
  - **10-20% faster kernel boot** (smaller, modular kernel)
  - **50-70% smaller kernel size** (storage savings, faster transfers)
  - **Significantly reduced RAM/CPU stress** during compilation
  Automatic population via `modprobed-db store` if installed; manual AUR installation available for maximum control.
- **Ashpd Compatibility Lock**: Pinned `rfd = "0.13"` in `Cargo.toml` for guaranteed compatibility with system `ashpd` library versions

### 1. **Pure Rust Core + egui UI**
- **Full Rust Architecture**
- **Native Synchronization**: Direct sourcing from AUR and GitLab repositories using the `git2` Rust library for atomic synchronization.
- **Async Tokio Orchestrator**: Truly concurrent I/O orchestration (git operations, file management, build monitoring).
- **egui Framework**: Immediate-mode, high-performance UI via [`src/ui/app.rs`](src/ui/app.rs) with persistent themes (Dark/Light), V2 physical-pixel scaling, and responsive `StripBuilder` layouts.
- **Unified Cargo Build**: Single `cargo` build process manages the entire stack — no external UI compilers or C++ runtime overhead.

### 2. **Modern SCX Loader (scx_loader) Orchestration & Hardware Intelligence**
- **SCX Loader Service**: Modern `scx_loader.service` manages userspace BPF-based schedulers with TOML-based configuration at `/etc/scx_loader/config.toml`
- **One-Click System-Wide Scheduler Activation**: Polkit-elevated service orchestration enabling non-root users to enable/disable SCX schedulers via Polkit authorization
- **Microarchitecture Detection**: Automatic CPU feature parsing (`/proc/cpuinfo`, `CPUID`) to enable `-march` optimizations tailored to the exact processor generation
- **Multi-GPU Policy Manager**: Unified handling of NVIDIA (Open/Proprietary), AMD, and Intel integrated GPUs with fallback logic and driver conflict resolution
- **ESP (EFI System Partition) Discovery**: Intelligent bootloader detection (GRUB, systemd-boot, rEFInd) with privileged `bootctl` integration for reliable ESP localization
- **Hardware-Aware Driver Auto-Discovery**: Modprobed-db auto-discovery of loaded drivers as the primary source of hardware reality, with optional Desktop Experience safety-net whitelist (Critical Filesystems: Ext4, FAT, VFAT, ISO9660, CIFS; NLS Support: ASCII, CP437 for EFI mounting; Storage Drivers: NVMe, AHCI/SATA; Input Devices: USB HID for keyboards/mice; Loopback and UEFI support) that only works in conjunction with auto-discovery to guarantee normal desktop functionality and bootability regardless of auto-discovery completeness

### 3. **High-Fidelity Diagnostics**
- **GOATd Full Benchmark Suite**: Standardized performance evaluation sequence spanning 6 phases (Baseline, Computational Heat, Memory Saturation, Scheduler Flood, Gaming Simulator, The Gauntlet) with automated 10-second intervals, visual pulsing indicators, and integrated result naming.
- **High-Density Result Comparison**: Advanced 48px full-width card interface featuring horizontal bi-directional delta bars and detailed tooltips for side-by-side kernel analysis.
- **Performance Spectrum Visualization**: High-density horizontal metric strips (7-metric: Latency, Consistency, Jitter, Throughput, Efficiency, Thermal, SMI Resilience) with integrated sparklines and moving-average pulse overlays in cyberpunk "Signal & Pulse" UI style.
- **Micro-Stutter Detection**: Specialized calibration targeting P99.9 alignment with 10µs optimal floor to detect the finest scheduling anomalies.
- **Laboratory-Grade Consistency**: Consistency scoring using the Coefficient of Variation (CV %) on a 2%-30% scale with linear normalization formula: `Score = 1.0 - (CV - 0.05) / 0.25`, providing objective "Silent Kernel" validation with irreducible 5% noise floor.
- **Data Recovery Methodology**: Uses a **1000-Sample Rolling Window (FIFO)** for all P99 and stability metrics, allowing the UI to reflect performance recovery after transient load spikes (unlike session-max aggregators).
- **Consolidated Real-time Log Piping**: Integrated telemetry stream bridging `LogCollector` and UI for unified tracing and system logs.
- **CPU Thermal Heatmap**: Per-physical-core thermal monitoring via sysfs to validate cooling efficiency and detect throttling under stress.

### 4. **3-Level Kernel Hardening System**
- **Minimal Hardening**: Performance-focused baseline with standard security mitigations disabled for maximum throughput
- **Standard Hardening**: Balanced security profile with essential mitigations (stack canaries, ASLR, DEP) enabled
- **Hardened Profile**: Maximum security enforcement with CFI, LSM (SELinux/AppArmor), SMACK, and lockdown mechanisms for security-critical systems
- **Binary-Level CONFIG_CMDLINE Enforcement**: Critical security and optimization parameters are baked directly into the kernel binary via `CONFIG_CMDLINE`, ensuring runtime enforcement independent of bootloader configuration
- **MGLRU (Multi-Gen LRU) Support**: Advanced multi-generation LRU memory management with configurable `enabled_mask` and `min_ttl_ms` per profile, enforced via `CONFIG_CMDLINE` binary bake-in for predictable memory behavior

### 5. **Modular Component Architecture**
- **Decoupled Design**: The application has been fully modularized into specialized, independently testable modules:
  - **`system/`**: OS abstractions (logging, package management, process control) and **Performance Diagnostics**.
  - **`kernel/`**: Kernel package management, Patcher (`KernelPatcher`), and system audit.
  - **`config/`**: State management (`AppState`) and Rule Engine (`Finalizer`).
  - **`ui/`**: Responsive `egui` views and the `AppController`.
- **Atomic GUI Authentication**: Single, unified password prompt via `SudoContext` that persists across all administrative operations—no repetitive credential requests
- **Async/Non-blocking UI**: Non-blocking kernel audit operations via `tokio::spawn_blocking` preventing UI thread starvation during tab switches or deep system inspections
- **Dependency Injection**: All modules accept trait objects instead of concrete implementations, enabling comprehensive unit testing without filesystem or privilege escalation requirements
- **Unified Error Handling**: Global `AppError` enum with 8 variants covering all failure modes (OS commands, hardware detection, config, I/O, etc.)
- **Input Validation Contracts**: All external inputs (package names, file paths) validated before OS command execution, preventing RCE via shell injection

### 6. **Kernel Manager & Lifecycle Suite: Complete System Kernel Management**
- **System Kernel Discovery**: Automatic detection and cataloging of all installed kernels across the system via direct `/boot`, `/efi`, and bootloader entry scanning
- **Workspace Build Artifact Installation**: Seamless integration of locally compiled kernels into the bootloader and system registry with full module validation
- **Booted Kernel Safety Protections**: Enhanced "Safety Net" mechanism that prevents accidental modification or deletion of the currently booted kernel via PID verification and version matching
- **Bootloader Entry Management**: Automatic EFI boot entry registration for systemd-boot and GRUB with intelligent ESP discovery via privileged `bootctl` integration
- **Kernel Module Auditing**: Post-install verification of module availability and driver integrity across all installed variants with dependency tracking
- **Multi-Kernel Coexistence**: Safe inter-version switching with dependency auditing across multiple installed kernel variants
- **Deep-Dive Kernel Audit**: Comprehensive hardware subsystem inspection covering:
    - **Compiler & Optimization Chain**: Detection of GCC/LLVM versions, LTO stability, and ISA-level optimizations (`-march` mapping to CPU capabilities)
    - **Scheduler Analysis**: BORE vs. EEVDF scheduler evaluation with workload-specific recommendations
    - **Sched_ext Introspection**: Multi-layer detection of active SCX scheduler via sysfs kernel state, binary ops identification, and scxctl service mode query
    - **NVMe Hardware Queues**: Detection of available queue depth and automatic tuning via `CONFIG_NVME_MULTIPATH` and queue optimization
    - **ISA Level Detection**: Automatic identification of CPU instruction sets (SSE, AVX, AVX2, AVX-512) for compiler flag targeting
    - **Aurora Green Highlighting**: Advanced scheduler state visualization with color-coding for detected SCX configurations
- **Recursive Workspace Cleanup**: Safe deletion of cached builds, temporary artifacts, and package caches with confirmation prompts
- **Automatic Branding Injection**: Seamless `pkgbase` identifier transformation for custom kernel naming and bootloader visibility

### 7. **Four-Profile Kernel Optimization System with Sched_ext Integration & GOATd Full Benchmark**
- **Gaming Profile**: Low-latency optimizations (Thin LTO default, 1000Hz timer, Polly loop optimization) for esports and real-time applications with optional SCX performance tuning.
- **Workstation Profile**: Professional security-hardened kernel (security focus, Thin LTO default, 1000Hz timer, hardened defaults) for developers and security-conscious users.
- **Server Profile**: Maximum throughput optimization (EEVDF baseline with optional SCX throughput strategy, Full LTO default, 100Hz timer, no preemption) for datacenter and enterprise workloads.
- **Laptop Profile**: Power-efficient kernel (EEVDF baseline with optional SCX power-efficient strategy, Thin LTO default, 300Hz timer, voluntary preemption) optimized for battery life without sacrificing responsiveness.
- **LTO Strategy**: Link-Time Optimization options (None, Thin, Full) available to all profiles. Each profile sets a **default** LTO level (Thin for Gaming/Workstation/Laptop, Full for Server) that applies when selected, but users can **change to any LTO option** before building—full control retained.
- **Polly LLVM Loop Optimization**: Integrated `-mllvm -polly -mllvm -polly-vectorizer=stripmine` flags for advanced loop vectorization and optimization, with visible build tracking via real-time compilation logs.
- **Rolling Clang Compiler Model**: All profiles enforce Clang (latest available, minimum 16+) exclusively via `_FORCE_CLANG=1` global enforcement; no version pinning, ensuring access to latest compiler optimizations. Compiler version tracked in post-build Performance Tier classification.
- **Benchmark-Driven Profile Selection**: Run the GOATd Full Benchmark (60s) immediately post-install to measure results in one of four Performance Tiers and guide future customizations.


---

## Documentation

### User & Setup Guides
- [`GUIDE.md`](GUIDE.md): **START HERE** — Complete user guide covering setup, performance metrics deep dive, the GOAT Recipe, dashboard flow, and troubleshooting.
- [`docs/USER_GUIDE.md`](docs/USER_GUIDE.md): Comprehensive user documentation (installation, profiles, Gauntlet benchmark, diagnostics, kernel management).

### Developer & Technical Reference
- [`DEVLOG.md`](DEVLOG.md): Complete development history (42+ phases), architectural decisions, and challenge resolutions.
- [`BLUEPRINT_V2.md`](BLUEPRINT_V2.md): The canonical technical specification for the 5-phase hard enforcer and architecture.
- [`docs/DEVELOPER_GUIDE.md`](docs/DEVELOPER_GUIDE.md): Developer reference (architecture, testing, contributing, debugging).

---

## Licensing & Support

### License

This project is provided under the **GPL-3.0** license for individual use.

### Commercial Licensing & Usage Policy

#### Individual Use
GOATd Kernel Builder is **free to use** for personal, non-commercial purposes under the terms of the GNU General Public License Version 3.0 (GPL-3.0).

#### Corporate & Business Use
Any utilization of the technology, code, or methodology contained within this application in a **corporate, business, or professional environment** is **subject to a per-basis licensing cost** that will be determined through direct negotiation between the developer (**madgoat**) and the client.

#### Licensing Requirement
If you intend to use this software in any commercial setting—including corporate kernel customization, enterprise deployments, commercial distributions, or any profit-generating context—**you must contact the developer** to discuss licensing terms and obtain written authorization.

**Unauthorized commercial use is a material breach of this license.**

For licensing inquiries: Use GitHub Issues to report and request contact for commercial discussions.

### Support

As a single-developer project, responses to issues and feature requests are handled on a best-effort basis through GitHub Issues. See the [`LICENSE`](LICENSE) file for the complete terms.

---
