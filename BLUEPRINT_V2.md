# MASTER ARCHITECTURAL SPECIFICATION (BLUEPRINT V2): THE CANONICAL TRUTH

## 1. System Vision & Philosophy
The GOATd Kernel Builder has transitioned from a **Static Preset** model to a **Dynamic Surgical Override** model with **Persistent Sched_ext Strategy Management**, **High-Fidelity Performance Diagnostics**, and **egui-based event-driven architecture**.
- **User Intent is Sovereign**: UI toggles in the `egui` interface generate explicit overrides that persist through the entire pipeline.
- **Hierarchical Truth**: Configuration is resolved via: **Hardware Truth > User Override > Profile Preset**.
- **The Unified Surgical Engine**: String manipulation and "patch-on-patch" debt have been purged. All file modifications are now owned exclusively by the `KernelPatcher`.
- **Multi-Phase Hard Enforcement**: The 171-module/LTO-Thin breakthrough is protected by five distinct phases: Phase 5 (Orchestrator), Phase G1 (Prebuild LTO), Phase G2 (Post-Modprobed), Phase G2.5 (Post-Setting-Config), and Phase E1 (Post-Oldconfig), ensuring Kconfig reversion is impossible.
- **CONFIG_CMDLINE Bake-In**: Critical kernel features (MGLRU, hardening, mitigations) are baked into the kernel binary via `CONFIG_CMDLINE`, ensuring runtime enforcement independent of bootloader parameters.
- **Persistent Scheduler Management**: Sched_ext (SCX) userspace BPF schedulers can be persistently configured via `/etc/scx_loader/config.toml` with system-wide one-click activation via Polkit.
- **Unified Privileged Logging**: Decoupled, non-blocking logging pipeline guarantees disk persistence even if UI channel is saturated, with automatic background task recovery.

---

## 2. System Hierarchy & Module Responsibilities

| Layer | Component | Responsibility | Technical Constraint |
|:---|:---|:---|:---|
| **UI** | [`AppUI`](src/ui/app.rs) | **Frontend Orchestrator**: Manages `egui` tab routing and event loops. | Reactive rendering using atomic dirty flags. |
| **Logic Broker** | [`AppController`](src/ui/controller.rs) | **Intent Broker**: Bridges UI signals to `AppState` updates and spawns `tokio` tasks. | Thread-safe interaction via `Arc<RwLock<>>`. |
| **State** | [`AppState`](src/config/mod.rs) | **In-Memory Source of Truth**: Stores hardware info, profiles, and transient UI settings. | Must be persistent-ready for JSON serialization. |
| **Rule Engine** | [`Finalizer`](src/config/finalizer.rs) | **Hierarchical Resolution**: Merges Hardware > Overrides > Profiles into the final `KernelConfig`. | **Pure function**. No side effects (no file I/O). |
| **Performance** | [`PerformanceDashboard`](src/ui/performance.rs) | **Real-Time Visualization**: High-fidelity rendering of latency, jitter, and thermal telemetry. | 100ms UI throttling to prevent over-rendering. |
| **Coordinator** | [`AsyncOrchestrator`](src/orchestrator/mod.rs) | **Phase Transition Manager**: Coordinates preparation, configuration, patching, building, and validation. | **NEVER** edits files directly. Delegates to Patcher/Executor. |
| **Surgical Engine** | [`KernelPatcher`](src/kernel/patcher.rs) | **The Unified Enforcer**: The ONLY module permitted to modify `PKGBUILD` or `.config`. | Must maintain atomicity and create backups before any edit. |
| **Execution** | [`Executor`](src/orchestrator/executor.rs) | **Pure Runner**: Executes `makepkg` and monitors progress. | Does not "know" about configuration. Only knows about files and processes. |

---

## 3. The Five-Phase Hard Enforcer Protection Pipeline

To ensure the 171-module/LTO-Thin breakthrough survives the kernel's aggressive Kconfig expansion, a **five-phase surgical protection pipeline** is enforced. See **Section 9** for detailed implementation code and mechanics.

**Quick Reference:**

| Phase | Location | When | What | Why |
|-------|----------|------|------|-----|
| **5 (Orchestrator)** | [`apply_kconfig`](src/kernel/patcher.rs:671) | Pre-PKGBUILD | Regex-based removal + atomic Clang/LTO injection | Establishes clean baseline |
| **G1 (Prebuild)** | PKGBUILD `build()` | Before `make` | Surgically delete ALL LTO entries, atomically inject trio | Final gate before compilation |
| **G2 (Post-Modprobed)** | PKGBUILD `prepare()` | After `localmodconfig` | Extract 170 filtered modules, protect from `olddefconfig` reexpansion | Prevent Kconfig re-enabling thousands of modules |
| **G2.5 (Post-Setting)** | PKGBUILD `prepare()` | After Arch config copy | Detect overwrite, re-apply localmodconfig, restore CONFIG_CMDLINE | Fix "6199 modules restored" bug from Arch script |
| **E1 (Post-Oldconfig)** | After any `make oldconfig`/`syncconfig` | Dynamic reconfig | Re-apply LTO enforcement + `olddefconfig` | Survive Kconfig reversions to CONFIG_LTO_NONE |

**Enforcement Guarantee**: The authoritative trio (`CONFIG_LTO_CLANG=y`, `CONFIG_LTO_CLANG_THIN=y`, `CONFIG_HAS_LTO_CLANG=y`) is SURGICALLY enforced at five distinct checkpoints, ensuring the kernel CANNOT revert to GCC or LTO_NONE, regardless of Kconfig modifications.

**See Section 9 for detailed implementation, bash injection patterns, and phase sequencing logic.**

---

## 4. Kernel Audit & Introspection: Enhanced Sched_ext Detection

### **Multi-Layer Sched_ext (SCX) Introspection**

The Kernel Manager provides detailed introspection into the active scheduler and operational modes through a multi-layer detection discovery chain:

**Discovery Chain: Kernel State → Binary Name → Service Mode**

1. **Layer 1: Kernel State Detection (sysfs)**
   - **File**: `/sys/kernel/sched_ext/state`
   - **Detection**: Reads kernel's reported SCX state (enabled/disabled/valid)
   - **Reliability**: Most authoritative; reflects actual kernel scheduler loaded
   - **Result**: Determines if SCX is active at kernel level

2. **Layer 2: Binary Name Identification (ops)**
   - **Source**: `/sys/kernel/sched_ext/ops`
   - **Detection**: Identifies the specific SCX scheduler binary name (scx_bpfland, scx_lavd, scx_rustland, etc.)
   - **Reliability**: Second confirmation; tells us WHICH scheduler is running
   - **Result**: Provides scheduler strategy identification

3. **Layer 3: Service Mode Detection (scxctl)**
   - **Command**: `scxctl status` query
   - **Detection**: Queries `scx_loader` service for configured mode (auto, gaming, lowlatency, powersave, server)
   - **Reliability**: Tertiary confirmation; reflects user's configured preference
   - **Result**: Shows service-level mode configuration

**Files & Commands Used for Audit**:
- `/sys/kernel/sched_ext/state` — Kernel state reporting
- `/sys/kernel/sched_ext/ops` — Active operations binary name
- `scxctl status` — Service management query
- Fallback: Check running processes with `ps aux | grep scx_` if sysfs unavailable

**UI Display**:
- Show all three layers in Kernel Manager audit panel
- Display detected binary name (e.g., "scx_bpfland @ scx_loader.service")
- Indicate service mode if available
- Provide "one-click" button to toggle SCX without rebuild

---

## 5. Scheduler Orchestration Strategy

### **Modern SCX Loader (scx_loader.service) & TOML-Based Configuration**

The GOATd Kernel Builder supports the modern `scx_loader.service` standard for persistent userspace BPF scheduler (sched_ext/SCX) management with TOML-based configuration and system-wide activation via Polkit-elevated service orchestration.

**TOML Configuration Schema** (`/etc/scx_loader/config.toml`):
```toml
[scheduler]
# Active scheduler selection: auto, gaming, lowlatency, powersave, server
mode = "gaming"

# Per-mode scheduler override
[modes]
auto = "scx_bpfland"        # Automatic workload balancing
gaming = "scx_bpfland"       # Low-latency real-time gaming
lowlatency = "scx_lavd"      # Ultra-low latency execution
powersave = "scx_rustland"   # Power-efficient scheduling
server = "disabled"          # EEVDF kernel baseline (no SCX)
```

**Service Management**:
- **Service File**: `scx_loader.service` — systemd unit that loads configured scheduler on boot
- **Configuration Path**: `/etc/scx_loader/config.toml` — User-editable TOML file for mode selection
- **Polkit Authorization**: Non-root users can enable/disable SCX schedulers via Polkit-elevated daemon, no password required for authorized users

**Profile Integration** (5 Performance Modes):
- **Auto**: Automatic workload detection via `scx_bpfland` for dynamic system balancing
- **Gaming**: `scx_bpfland` for low-latency task switching (optimal for esports and real-time applications)
- **LowLatency**: `scx_lavd` for ultra-responsive interactive tasks
- **PowerSave**: `scx_rustland` with power-efficient task scheduling for extended battery life
- **Server**: Disabled (EEVDF kernel baseline) for deterministic throughput-optimized performance

**Kernel Baseline vs. SCX Strategy**:
- **Baseline Kernel**: Always uses EEVDF scheduler (Linux 6.7+ standard fair scheduler, no SCX required)
- **SCX Enhancement**: Optional userspace BPF scheduler provides specialized strategies for workload-specific tuning
- **User Control**: Users can toggle SCX on/off post-installation via UI or command-line without kernel rebuild

**UI Controls** ([`Kernel Manager`](src/ui/kernels.rs)):
- **SCX Configuration Section**: Integrated panel showing current SCX status, available modes, and one-click activation.
- **Mode Selection Dropdown**: Choose performance mode (Auto/Gaming/LowLatency/PowerSave/Server); UI updates `/etc/scx_loader/config.toml`.
- **Profile Integration**: Primary profile selection (in [`Build`](src/ui/build.rs)) automatically sets recommended SCX modes.
- **System-Wide Toggle**: Enable/disable SCX with a single button; Polkit handles privilege escalation transparently.

---

## 6. Performance Diagnostics Architecture

### **7-Metric Spectrum Architecture**
The diagnostic engine implements a comprehensive **Performance Spectrum** using a lock-free pipeline for nanosecond-precision collection of seven critical performance metrics.

| Metric | Measurement Unit | Critical Thresholds (Optimal → Poor) | Weight |
|:---|:---|:---|:---|
| **Latency** | Microseconds (µs) | 10µs → 500µs | 27% |
| **Consistency** | CV % (std_dev/mean) | 5% → 30% | 18% |
| **Jitter** | CV % (std_dev/mean) | 5% → 30% | 15% |
| **Throughput** | Ops/sec | 1.0M → 100k | 10% |
| **Efficiency** | Context-Switch µs | 1µs → 100µs | 10% |
| **Thermal** | Celsius (°C) | 40°C → 90°C | 10% |
| **SMI Res.** | Correlation Ratio | 0 SMIs → 10+ SMIs | 10% |

### **Normalization & Scoring Formulas**
All metrics are normalized to a 0.001 - 1.0 scale where 1.0 is optimal.

#### **Linear CV Normalization (Consistency & Jitter)**
`Score = 1.0 - (CV - 0.05) / 0.25`

Clamps to range `[0.001, 1.0]` where:
- **Optimal**: CV ≤ 5% → Score = 1.0 (Laboratory Grade)
- **Poor**: CV ≥ 30% → Score ≤ 0.001

This linear formula provides **Laboratory Grade consistency** validation by measuring the Coefficient of Variation (standard deviation / mean). The 5% baseline accounts for irreducible noise floor. Jitter uses short-term (30s) history, while Consistency uses long-term (1000-sample) rolling windows.

#### **High-End Latency Calibration (10µs - 500µs)**
`Score = 1.0 - ((rolling_p99_us - 10.0) / 490.0)`

Clamps to range `[0.001, 1.0]` where:
- **Optimal**: P99 ≤ 10µs → Score = 1.0
- **Poor**: P99 ≥ 500µs → Score ≤ 0.001

Optimized for next-generation microarchitectures (AMD 9800X3D, Intel 265K) with sub-microsecond responsiveness targets.

#### **1000-Sample Rolling Window (FIFO Diagnostic Methodology)**
All P99 and consistency metrics use a **1000-sample FIFO buffer** methodology:
- Maintains sliding window of latest 1000 measurements
- Enables **Data Recovery** visualization after transient spikes
- Replaces session-max aggregators with dynamic recency weighting
- Automatically drops oldest sample when buffer reaches capacity
- Provides UI-level responsiveness by reflecting recent performance state

#### **Deep Reset & Laboratory Conditions**
Upon benchmark trigger or user request:
- Atomic clearing of all rolling buffers and measurements
- MSR counter reset for SMI detection baseline
- Thermal sensor reinitialization
- Ensures clean laboratory conditions for reproducible testing

### **GOAT Score (0-1000) - Re-Balanced Weighting**
The GOAT Score is a weighted aggregate of the normalized spectrum with updated Latency/Consistency emphasis:

`GOAT Score = (L×0.27 + C×0.18 + J×0.15 + T×0.10 + E×0.10 + Th×0.10 + S×0.10) * 1000`

Where:
- **L** = Latency score (27% weight - emphasizes responsiveness)
- **C** = Consistency score (18% weight - emphasizes frame pacing purity)
- **J** = Jitter score (15% weight - emphasizes deviation bounds)
- **T** = Throughput score (10% weight)
- **E** = Efficiency score (10% weight)
- **Th** = Thermal score (10% weight)
- **S** = SMI Resilience score (10% weight)

### **6.2 GOATd Full Benchmark (60s Sequence)**
The system features a standardized 60-second performance evaluation sequence designed to stress different kernel subsystems in 10-second intervals.

| Phase | Name | Focus | Load Characteristics |
|:---|:---|:---|:---|
| 1 | **Baseline** | Idle Calibration | Zero-load baseline for noise floor detection. |
| 2 | **Computational Heat** | CPU Stress | Heavy AVX/AVX-512 vectorization stress. |
| 3 | **Memory Saturation**| RAM Throughput | Sequential and random memory access patterns. |
| 4 | **Scheduler Flood** | Task Switching | Rapid context switching and process spawning. |
| 5 | **Gaming Simulator** | Jitter Analysis | Mix of bursty loads with high-priority interruptions. |
| 6 | **The Gauntlet** | Maximum Stress | Simultaneous CPU, Memory, and IO saturation. |

---

## 7. Hardware-Aware Intelligence

### GPU Driver Exclusion Logic
The system automatically detects the system's GPU (`Nvidia`, `AMD`, `Intel`) and applies hardware-specific policies:
- **LTO Shielding**: AMD drivers (`amdgpu`, `amdkfd`) are shielded from LTO in their respective Makefiles to prevent functional breakages.
- **Surgical Stripping**: If an Nvidia GPU is detected, the `amdgpu` driver stack is explicitly added to `driver_exclusions` to reduce binary bloat.

### MGLRU Profile-Specific Tuning
MGLRU is tuned at the **Rule Engine** layer according to the selected profile:
- **Gaming/Workstation**: `enabled_mask=0x0007`, `min_ttl_ms=1000` (Balanced).
- **Laptop**: `enabled_mask=0x0007`, `min_ttl_ms=500` (Aggressive memory reclaim for power).
- **Server**: `enabled_mask=0x0000` (Disabled for deterministic performance).

---

## 8. CONFIG_CMDLINE Bake-In: Runtime Parameter Enforcement

The system forcefully enables kernel features at compile-time via `CONFIG_CMDLINE`, which overrides any runtime kernel parameters. This is **critical** for features like MGLRU that require both CONFIG options AND runtime parameters.

### Parameters Baked Into Kernel Binary

**Always Injected**:
- `nowatchdog` — Disable kernel watchdog timer for performance
- `preempt=full` — Enforce full kernel preemption at runtime

**Conditionally Injected**:
- `lru_gen.enabled=7` — if MGLRU is enabled (enables all MGLRU subsystems: file, anon, and related)
- `mitigations=off` — if hardening level is **Minimal** (performance optimization)

### Implementation Details

The [`inject_baked_in_cmdline`](src/kernel/patcher.rs:556) method:
1. **Preserves existing CONFIG_CMDLINE** if already present (appends to it rather than replacing)
2. **Surgically removes** conflicting CONFIG_CMDLINE* entries (`CONFIG_CMDLINE_BOOL`, `CONFIG_CMDLINE_OVERRIDE`)
3. **Injects three critical lines** into `.config`:
   - `CONFIG_CMDLINE="<params>"` — The baked-in kernel command line
   - `CONFIG_CMDLINE_BOOL=y` — Enable command-line override (required for CONFIG_CMDLINE to take effect)
   - `CONFIG_CMDLINE_OVERRIDE=n` — Prevent the baked-in CMDLINE from being overridden at runtime

---

## 9. Five-Phase Hard Enforcer Protection Stack

The kernel configuration survives the Arch build system's aggressive Kconfig expansion through five distinct enforcement phases:

### **Phase G1: Prebuild LTO Hard Enforcer (PKGBUILD build())**
**When**: Immediately before the `make` command in `build()`
**What**: Surgical removal of ALL LTO-related entries, followed by atomic injection of authoritative LTO trio
**Why**: Final gate protection; at this point, all other config changes are finalized
**Code**: [`inject_prebuild_lto_hard_enforcer`](src/kernel/patcher.rs:1016)

The injected bash block:
```bash
# Remove ALL LTO variants (LTO_CLANG, LTO_CLANG_THIN, LTO_CLANG_FULL, HAS_LTO_CLANG, and commented variants)
sed -i '/^CONFIG_LTO_\|^CONFIG_HAS_LTO_\|^# CONFIG_LTO_\|^# CONFIG_HAS_LTO_/d' ".config"

# Atomically inject correct LTO (respecting lto_type parameter)
cat >> ".config" << 'EOF'
CONFIG_LTO_CLANG=y
CONFIG_LTO_CLANG_THIN=y  # or FULL if selected
CONFIG_HAS_LTO_CLANG=y
EOF

# Finalize with olddefconfig
make LLVM=1 LLVM_IAS=1 olddefconfig
```

### **Phase G2: Post-Modprobed Hard Enforcer (PKGBUILD prepare())**
**When**: After `make localmodconfig` filters to ~170 modules
**What**: Protects filtered modules from Kconfig dependency re-expansion
**Why**: Kconfig's `olddefconfig` re-enables thousands of modules that localmodconfig just filtered
**Code**: [`inject_post_modprobed_hard_enforcer`](src/kernel/patcher.rs:1444)

Strategy:
1. Extract the 170 filtered modules from `.config` BEFORE running olddefconfig
2. Run `make olddefconfig` to handle consistent Kconfig dependencies
3. Remove ALL non-filtered `CONFIG_*=m` entries, keep ONLY the original 170
4. Result: Filtered module set is preserved with correct dependencies

### **Phase G2.5: Post-Setting-Config Restorer (PKGBUILD prepare())**
**When**: After the Arch build script runs `cp ../config .config`
**What**: Re-detects and restores overwritten configuration
**Why**: Arch's standard config copy OVERWRITES our surgical modprobed filtering (6199 modules restored!)
**Code**: [`inject_post_setting_config_restorer`](src/kernel/patcher.rs:1286)

Restoration sequence:
1. Capture CONFIG_CMDLINE* parameters BEFORE the overwrite
2. Detect the overwrite via module count spike
3. Re-apply modprobed filtering immediately
4. Re-inject backed-up CONFIG_CMDLINE* and MGLRU configs

### **Phase E1: Post-Oldconfig LTO Re-Enforcement (PKGBUILD prepare/build)**
**When**: After any `make oldconfig` or `make syncconfig` call
**What**: Re-applies LTO enforcement to counter Kconfig reversion
**Why**: Kernel's Kconfig system may revert LTO settings to `CONFIG_LTO_NONE=y` during reconfig
**Code**: [`inject_post_oldconfig_lto_patch`](src/kernel/patcher.rs:2003)

Enforcement:
```bash
# Surgically remove ALL LTO entries again
sed -i '/^CONFIG_LTO_\|^CONFIG_HAS_LTO_\|^# CONFIG_LTO_\|^# CONFIG_HAS_LTO_/d' ".config"

# Append correct LTO back AFTER removal (ensures final priority)
cat >> ".config" << 'EOF'
CONFIG_LTO_CLANG=y
CONFIG_LTO_CLANG_THIN=y  # or FULL
CONFIG_HAS_LTO_CLANG=y
EOF

# Re-finalize
make LLVM=1 LLVM_IAS=1 olddefconfig
```

### **Phase 5 (Orchestrator): Surgical Atomic Enforcer**
**When**: During the Orchestrator's `patch()` phase, in [`apply_kconfig`](src/kernel/patcher.rs:671)
**What**: Regex-based removal of ALL GCC/LTO variants followed by atomic Clang/LTO injection
**Why**: Establishes clean baseline before PKGBUILD scripts even start

This phase is the **first** hard enforcement, setting the foundation before Phases G1-E1 activate.

---

## 10. Development Roadmap & Future Vision (Implemented Features vs. Planned)

### ✅ Fully Implemented Features

**Core Architecture**:
- ✅ **egui Event-Driven UI** — Replaced Slint with egui for responsive, reactive rendering
- ✅ **AppUI + AppController + AppState** — Clean separation: UI rendering, intent brokering, persistent state
- ✅ **Privileged Logging Pipeline** — Decoupled disk/UI dispatch with unbounded channel guarantee
- ✅ **Thread-Safe State Management** — `Arc<RwLock<AppState>>` for parallel reads, exclusive writes

**Build System**:
- ✅ **Hard Enforcer Protection Stack** — Four-phase enforcement (G1, G2, G2.5, E1) prevents all Kconfig reversion
- ✅ **CONFIG_CMDLINE Bake-In** — Runtime parameters forced into kernel binary for persistence
- ✅ **Modprobed-db Integration** — Automatic hardware-aware module filtering to ~170 modules
- ✅ **Kernel Whitelist Protection** — Critical filesystem/device drivers always built for bootability
- ✅ **Polly Loop Optimization** — LLVM Polly flags (`-mllvm -polly -mllvm -polly-vectorizer=stripmine`) injected with strip-mine vectorizer
- ✅ **Secure Boot Hardening** — Module signature enforcement, EFI_SECURE_BOOT, LOCKDOWN_LSM
- ✅ **SELinux/AppArmor Configuration** — Mandatory access control (MAC) framework injection

**Performance Diagnostics**:
- ✅ **Real-Time Latency Collection** — Lock-free `rtrb` ring buffer for nanosecond-precision samples
- ✅ **7-Metric Performance Spectrum** — Latency, Throughput, Jitter, Efficiency, Thermal, Consistency, SMI Res.
- ✅ **SMI Detection** — Model Specific Register (MSR) introspection for firmware interference detection
- ✅ **Per-Core Thermal Monitoring** — Real-time sysfs-based heat maps under stress
- ✅ **Benchmark Comparison Engine** — 48px high-density comparison dashboard with bi-directional delta bars and tooltips
- ✅ **Historical Performance Persistence** — Standardized JSON/CSV records with custom naming and integrated lifecycle management
- ✅ **Jitter Audit Session** — Background monitoring with alert thresholds and cycle-timer modes

**Scheduler Management**:
- ✅ **Modern scx_loader.service** — TOML-based configuration for sched_ext strategies
- ✅ **Multi-Layer SCX Detection** — Kernel state, binary name, and service mode introspection
- ✅ **One-Click SCX Toggle** — Polkit-elevated service management without password prompts
- ✅ **Profile-Integrated SCX** — Scheduler modes tied to performance profiles (Gaming, LowLatency, PowerSave, Server)

**Hardware Integration**:
- ✅ **GPU Driver LTO Shielding** — AMD driver exclusion from LTO via Makefile patching
- ✅ **MGLRU Profile Tuning** — Dynamic `enabled_mask` and `min_ttl_ms` per profile
- ✅ **Hardware-Aware Defaults** — CPU, GPU, RAM detection for baseline configuration

---

## 11. The "Constitution"
This document is the authoritative reference for the GOATd Kernel project. Any PR that violates the separation of concerns or bypasses the Surgical Engine must be rejected.
