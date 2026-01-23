# GOATd Kernel Project Scope

**Document Version**: 2.2
**Last Updated**: 2026-01-23 (Phase 44+ - Laboratory-Grade Hardening Achievement)
**Purpose**: Define the role, responsibility, and interface boundaries of every major component to prevent conceptual drift and ensure long-term maintainability. Tracks implemented features with checkmarks and defers non-implemented work to Future Development.

---

## Executive Summary

GOATd Kernel Builder is a **modular, multi-architecture kernel customization and build orchestration system** written in Rust with an egui UI frontend. The orchestrator (GOATd Kernel Builder) manages the complete lifecycle of building, optimizing, and deploying custom Linux kernels (GOATd Kernel) with **laboratory-grade hardening**, hardware-aware configurations, **resilient kernel building with 5-phase enforcement**, and real-time performance validation.

### ✅ Recent Achievement Milestones (Phase 44+)

**Laboratory-Grade Hardened Environment**:
- Hermetically sealed build environment with explicit environment variable management
- 5-Phase Build Protection Stack ensuring configuration survival across all makepkg stages
- Definitive Rust Headers Fix resolving AST-aware injection for DKMS out-of-tree driver compatibility
- Atomic Configuration Management with comprehensive backup and rollback mechanisms
- PYTHONDONTWRITEBYTECODE enforcement preventing bytecode cache contamination

**Resilient Kernel Building & System-Specific Optimization**:
- Achieved resilient kernel compilation with 171-module retention across diverse system configurations
- System-specific optimization through hardware-aware driver filtering (GPU exclusion logic, NVMe queue detection)
- Multi-path driver resolution (modprobed-db PRIMARY → whitelist SAFETY NET → default FALLBACK)
- 20-Point Review Cycle: Comprehensive audit of all core systems for production readiness
- rmeta/so glob fixes ensuring deterministic artifact caching and incremental build efficiency

**Orchestrator Wiring Consolidation**:
- Unified Surgical Engine (KernelPatcher as single source of truth)
- Hierarchical configuration resolution (Hardware > Override > Profile)
- Decoupled modular architecture with explicit responsibility boundaries
- Non-blocking async operations preventing UI starvation during heavy workloads

**Core Philosophy**:
- **Single Responsibility**: Each module has one clear purpose
- **Hardware Truth**: `modprobed-db` is the primary source of hardware reality; whitelist provides an optional Desktop Experience safety net that only works in conjunction with auto-discovery
- **LTO Flexibility**: All three LTO options (None, Thin, Full) available to all profiles. Each profile has a default (Thin for most, Full for Server) that applies when selected, but users can change to any LTO option before building
- **Async-First**: All long-running operations (build, detection, patching) use tokio for non-blocking I/O
- **Fail-Safe**: When selected by the user, the whitelist (if auto-discovery is enabled) provides an "expected desktop environment" regardless of auto-discovery completeness, guaranteeing normal desktop functionality and bootability

---

## Features Implemented ✅

### Core Build System
- [x] **5-Phase Orchestration**: Preparation, Configuration, Patching, Building, Validation with state machine enforcement
- [x] **Hardware Detection**: CPU, GPU, RAM, storage, boot mode, init system auto-detection at startup
- [x] **Async-First Architecture**: Tokio-based async runtime with non-blocking I/O for all long-running operations
- [x] **Real-time Build Progress**: Live streaming of build output with progress tracking (0-100%)
- [x] **Build Cancellation**: User-initiated cancellation via watch channel with graceful cleanup

### Kernel Profiles & Customization
- [x] **4 Kernel Profiles**: Gaming (BORE + Thin LTO + 1000Hz), Workstation (BORE + Thin LTO + Hardened), Server (EEVDF + Full LTO + 100Hz), Laptop (EEVDF + Thin LTO + 300Hz)
- [x] **LTO Support (Triple-Lock Enforcer)**: Full, Thin, and None options available to all profiles with profile defaults (Thin for most, Full for Server)
- [x] **3-Level Hardening**: Minimal, Standard, and Hardened levels selectable before build
- [x] **MGLRU (Multi-Gen LRU) Integration**: Baked-in memory management option for profiles
- [x] **Scheduler Selection**: BORE (low-latency) and EEVDF (throughput) with profile-appropriate defaults
- [x] **Compiler Enforcement**: Clang exclusively with rolling release model and `-O2 -march=native -mtune=native` standardization
- [x] **Timer Frequency Control**: Profile-enforced HZ settings (1000/300/100) with preemption model binding

### Driver Management
- [x] **Modprobed-db Integration**: Primary hardware truth with auto-discovery of loaded kernel modules
- [x] **Whitelist Safety Net**: Hardcoded minimal set (HID, USB, NVMe/AHCI, filesystems) for fail-safe boot when auto-discovery enabled
- [x] **Driver Exclusion Rules**: Blacklist filtering for proprietary/deprecated drivers
- [x] **Multi-Path Driver Selection**: Default → Modprobed-only → Modprobed+Whitelist merge logic

### UI & User Experience
- [x] **egui UI Frontend**: Immediate-mode UI with async event loop and callback wiring via eframe
- [x] **Dashboard Tab**: Real-time hardware detection display (CPU model, cores, GPU, RAM, storage, boot type)
- [x] **Build Tab**: Profile/LTO/hardening selectors, live log viewer, progress bar, elapsed timer
- [x] **Settings Tab**: Workspace configuration, theme mode, security level, audit options
- [x] **Kernel Manager Tab**: Installed kernel listing, built artifact scanning, install/uninstall/delete operations
- [x] **Performance Tab**: Real-time jitter audit, latency measurements, historical performance graphs
- [x] **Async-First UI Event Loop**: Non-blocking callback execution with Tokio integration

### System Auditing & Validation
- [x] **Deep Audit**: Comprehensive kernel inspection (version, compiler, LTO status, hardening, scheduler, module counts)
- [x] **Performance Spectrum**: 7-metric real-time diagnostics (Latency, Throughput, Jitter, Efficiency, Thermal, Consistency, SMI Res)
- [x] **LTO Symbol Verification**: Post-build verification of LTO artifacts in final binaries
- [x] **Kernel Config Validation**: Policy enforcement checks for conflicting options and hardware requirements

### Build Infrastructure
- [x] **PKGBUILD Patching**: Master Identity transformation (`linux` → `linux-goatd-<profile>`) with safe variable injection
- [x] **Multi-Kernel Coexistence**: Explicit omission of `conflicts=` to allow parallel installation with upstream linux
- [x] **LTO Shield Patches**: Critical section protection (AMDGPU, memory subsystems) from aggressive optimization
- [x] **ICF Removal**: Identical Code Folding removal for deterministic, reproducible builds
- [x] **Dry-Run Testing**: Build Pipeline Verification gate with PHASE G1 hard-enforcer validation (5-30 second feedback)
- [x] **Real Kernel Build Integration**: Production kernel build simulation with real sources and oldconfig reconciliation (2-10 minutes)

### Quality Assurance
- [x] **Build Pipeline Verification Test**: 7-gate dry-run validation (LTO, BORE, MGLRU, PKGBUILD, Clang, state machine, environment)
- [x] **Real Kernel Build Integration Test**: Production-grade LTO enforcement survival through oldconfig phase
- [x] **Comprehensive Test Suite**: Config validation, orchestrator state machine, hardware detection, integration tests
- [x] **Checkpoint System**: Build state snapshots for recovery (architecture in place, not yet persisted)

### Logging & Diagnostics
- [x] **Dual-Format Logging**: Full unstructured logs + parsed structured logs in `logs/` directory
- [x] **Real-time Log Streaming**: Build output to UI with progress correlation
- [x] **Error Reporting**: Detailed error messages with context and recovery hints
- [x] **Performance Diagnostics**: Thermal monitoring, stressor engine, SMI refinement for benchmarking

---

## Future Development Pipeline

### Scheduled for Near-Term Implementation
- [ ] **Advanced Performance Dashboard**: Aggregated benchmarking with comparison modal and stressor engine integration

### Long-Term Enhancements (Phase 6+)
- [ ] **Build Cache Optimization**: Incremental build caching across kernel version updates

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                  egui UI Frontend (eframe)                  │
│              Async event loop + reactive state               │
└────────────────────────────────┬────────────────────────────┘
                                 │
                    AppController (UI Bridge)
                                 │
        ┌────────────────────────┼────────────────────────┐
        │                        │                        │
   Orchestrator            Hardware Detection        Config Manager
   (5-Phase Build)         (CPU/GPU/RAM/Storage)     (Profiles/LTO)
        │
   ┌────┴────┬────┬────┬────┐
   │          │    │    │    │
Prep      Config Patch Build Validate
```

---

## Module Definitions

### 1. **`src/main.rs` - Orchestration Entry Point**

**Purpose**: Bootstrap the application, initialize Tokio async runtime, and wire egui UI callbacks to the build system.

**Responsibilities**:
- Create and enter the Tokio `multi_thread` runtime context (critical for callback availability)
- Initialize logging infrastructure (full and parsed logs in `logs/` directory)
- Load and detect hardware at startup (CPU, GPU, RAM, storage, boot mode)
- Synchronize application state to UI (variants, profiles, LTO settings, whitelist flags)
- Wire all egui UI callbacks to `AppController` state mutations
- Manage build event loop (`BuildEvent` channels for progress, status, logs)

---

### 2. **`src/system/performance/` - Real-Time Diagnostics Engine**

**Purpose**: Provide laboratory-grade performance telemetry and scoring with "Signal & Pulse" high-fidelity visualization, including standardized GOATd Full Benchmark (60s) evaluation, high-density result comparison, and Performance Tier classification.

**Architecture**:
- **Data Pipeline**: Lock-free real-time ring buffers for nanosecond-precision samples.
- **Result Management**: Persistent history using timestamped JSON/CSV records with naming support and integrated comparison dashboard.
- **7-Metric Spectrum**: Latency, Consistency, Jitter, Throughput, Efficiency, Thermal, SMI Resilience (removed Power metric).
- **Formulas**:
  - **Linear CV Normalization**: `Score = 1.0 - (CV - 0.05) / 0.25` for Consistency & Jitter (2%-30% scale)
  - **High-End Latency Calibration**: `Score = 1.0 - ((rolling_p99_us - 10.0) / 490.0)` (10µs optimal, 500µs poor)
  - All scores clamp to `[0.001, 1.0]`
- **Methodology**:
  - **1000-Sample Rolling Window (FIFO)**: Maintains sliding window of latest 1000 measurements, enabling **Data Recovery** after transient spikes.
  - **Deep Reset Mechanism**: Atomic clearing of rolling buffers and MSR counters upon benchmark trigger for reproducible conditions.
- **GOAT Score Re-Balancing**:
  - **Formula**: `GOAT Score = (L×0.27 + C×0.18 + J×0.15 + T×0.10 + E×0.10 + Th×0.10 + S×0.10) * 1000`
  - **Latency (27%)**: Emphasizes responsiveness (primary user experience metric)
  - **Consistency (18%)**: Emphasizes frame pacing purity and jitter bounds
  - **Jitter (15%)**: Emphasizes deviation stability
  - **Other Metrics (40%)**: Throughput, Efficiency, Thermal, SMI Resilience (10% each)
- **GOATd Full Benchmark (60s Sequence)**:
  - **6-Phase Standardized Evaluation**: Baseline (idle calibration) → Computational Heat (AVX stress) → Memory Saturation (RAM throughput) → Scheduler Flood (context switching) → Gaming Simulator (jitter analysis) → The Gauntlet (combined maximum stress), with 10-second intervals per phase and visual pulsing indicators.
  - **Phase 2.1 Collector Integration**: Advanced collectors (Latency, Consistency, Jitter, SMI, Thermal) run continuously during all 6 phases, gathering comprehensive performance data under varied load conditions.
  - **Result Lifecycle**: Automated transition from naming prompt to high-density comparison dashboard upon completion.
- **High-Density Comparison UI**:
  - **Architecture**: 48px full-width metric cards with horizontal delta bars centered at zero.
  - **Visualization**: Bi-directional color-coded bars (Green = improvement, Red = regression) with percentage deltas and tooltips.
  - **Management**: Direct deletion of historical records from the comparison interface for session hygiene.
---

### 3. **`src/orchestrator/` - Async Build State & Execution Management**

**Purpose**: Manage the complete kernel build lifecycle across 5 sequential phases with stateful progress tracking and checkpoint support.

**5-Phase Orchestration Flow**:

| Phase | Progress | Responsibility |
|-------|----------|---|
| **Preparation** | 0-5% | Validate hardware requirements; verify source; cleanup artifacts |
| **Configuration** | 5-8% | Apply profile settings (compiler, scheduler, LTO, MGLRU); apply GPU exclusion policy |
| **Patching** | 8-10% | Patch PKGBUILD; apply LTO shield, ICF removal; inject hard enforcers (G1, G2, G2.5, E1) |
| **Building** | 10-90% | Execute `makepkg` with real-time output and sub-phase milestone detection |
| **Validation** | 90-95% | Verify kernel config; check artifacts; verify LTO symbols |

---

### 4. **`src/kernel/` - Patcher, LTO, and Audit Logic (Technical Core)**

**Purpose**: Provide low-level kernel manipulation, verification, and performance auditing.

#### **`kernel/patcher.rs`** - Kernel Patching & Transformation
- **Master Identity**: Transforms `pkgbase=linux` → `linux-goatd-<profile>`.
- **Hard Enforcer Stack**: Implements five-layer protection pipeline (G1, G2, G2.5, E1, Phase 5) to prevent Kconfig reversion.
- **CMDLINE Bake-In**: Injects `CONFIG_CMDLINE` for persistent runtime parameter enforcement (MGLRU, mitigations, nowatchdog).

#### **`kernel/audit.rs`** - Deep System Audits
- **Deep Audit**: Comprehensive kernel inspection (version, compiler, LTO status, hardening, scheduler).
- **Sched_ext Detection**: Multi-layer introspection (Kernel State → Binary Name → Service Mode).

---

### 5. **`src/config/` - Profiles, Whitelist, and Rule Engine**

**Purpose**: Manage kernel build profiles, hardware driver policies, and configuration resolution.

#### **`config/finalizer.rs`** - Rule Engine
- **Hierarchical Resolution**: Merges Hardware Truth > User Override > Profile Preset.
- **Pure Functional**: Side-effect-free resolution of all `CONFIG_` options and environment variables.

#### **`config/whitelist.rs`** - Desktop Experience Safety Net
- **Definition**: Guarantees bootability when auto-discovery is used by forcing 22 core drivers (HID, Storage, Filesystems, USB).
- **Dependency**: Only active when `modprobed-db` Auto-Discovery is selected.

---

## Compatibility & Known Limitations

**Supported Systems**:
- Arch Linux (primary target)
- x86_64 architecture (hardcoded in validation checks)
- systemd-boot or GRUB bootloaders

**Known Limitations**:
- GPU drivers must be selected via Profile/Modprobe (not in whitelist).
- Secure Boot requires manual UEFI key management.
- Jitter audit requires `CONFIG_SCHED_DEBUG=y`.

---

## Maintenance & Change Management

**To Prevent Conceptual Drift**:
1. **New modules**: Update this document immediately.
2. **Formulas**: All scoring changes must be reflected in the Diagnostics Engine section.
3. **Hierarchy**: Respect Hardware > Override > Profile resolution.

---

## Conclusion

GOATd Kernel balances **hardware specificity** with **fail-safe defaults**, enabling users to build minimal, optimized kernels with performance validation.
