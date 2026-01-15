# GOATd Kernel Builder: User Guide

Welcome to GOATd Kernel Builder—a comprehensive toolkit for building, customizing, and managing Linux kernels tailored to your hardware and workload. This guide walks you through installation, basic usage, and advanced features.

---

## Table of Contents

1. [Installation & Setup](#installation--setup)
2. [Quick Start](#quick-start)
3. [Optimization Profiles](#optimization-profiles)
4. [The Gauntlet Benchmark](#the-gauntlet-benchmark)
5. [Performance Diagnostics](#performance-diagnostics)
6. [Kernel Management](#kernel-management)
7. [Sched_ext Scheduler Management](#sched_ext-scheduler-management)
8. [Advanced Build Options](#advanced-build-options)
9. [Troubleshooting](#troubleshooting)

---

## Installation & Setup

### Prerequisites

- **Arch Linux** (or compatible distribution with pacman)
- **Rust 2021 Edition** (for development; pre-compiled binaries available)
- **Clang 16+** (for kernel compilation)
- **sudo privileged access** (for kernel build and installation)

### Installation: Running GOATd Kernel Builder

**Automatic Setup (Arch Linux - Recommended)**:

Simply run:
```bash
./goatdkernel.sh
```

The launcher script will automatically:
- Detect that you're on Arch Linux (via `pacman`)
- Install all required system packages (`rust`, `base-devel`, `llvm`, `clang`, etc.)
- Setup GPG keys for kernel signature verification
- Initialize `modprobed-db` if installed (auto-discovery of loaded drivers)
- Build the Rust binary and launch the GUI

**Manual Setup (Other Distributions)**:

1. Ensure Rust is installed:
   ```bash
   rustc --version
   cargo --version
   # If not installed, visit https://rustup.rs/
   ```

2. Install LLVM/Clang 16+ (manual requirement for non-Arch systems):
   ```bash
   # Ubuntu/Debian
   sudo apt-get install clang llvm lld
   
   # Fedora
   sudo dnf install clang llvm lld
   
   # macOS
   brew install llvm
   ```

3. Build and launch the application:
   ```bash
   cd /path/to/GOATd\ Kernel
   ./goatdkernel.sh
   # Or manually: cargo build --release && ./target/release/goatd_kernel
   ```

4. The **egui** interface will launch in a native window. You'll be prompted for sudo credentials for administrative operations.

5. Grant **Polkit authorization** for SCX scheduler management (one-time setup):
   ```bash
   # Polkit will prompt the first time you attempt SCX operations
   ```

---

### Modprobed-DB Auto-Initialization

**What it does**: On **Arch Linux systems**, the launcher script automatically initializes `modprobed-db` if it's installed.

**Auto-Initialization Process**:
1. Script detects if `modprobed-db` command is available
2. If present, runs: `modprobed-db store`
3. This populates `~/.config/modprobed.db` with all currently loaded kernel modules
4. Enables automatic hardware-aware module filtering on your next kernel build

**Manual Initialization** (if needed):
```bash
modprobed-db store
```

**Database Location**: `~/.config/modprobed.db`

**Result**: Your next kernel will include ONLY the drivers for hardware you're actively using (~170 instead of 6,000+ modules), reducing build time by 70%.

---

### LLVM Toolchain Requirements

**Why LLVM matters**: GOATd enforces **Clang-based compilation exclusively** for all kernels.

**Why this is important**:
- **Polly Loop Optimization**: Advanced vectorization via `-mllvm -polly` flags for better CPU cache utilization
- **LTO (Link-Time Optimization)**: Full whole-program optimization requires LLVM toolchain
- **Modern ISA Support**: Proper `-march` targeting for AVX-512, other modern CPU features
- **Consistent Optimization**: No ambiguity from GCC vs. Clang differences

**Automatic Detection & Installation**:
- On Arch Linux, `./goatdkernel.sh` automatically installs: `llvm`, `clang`, `lld`, `polly`
- If not on Arch, you **must** install these packages manually before building

**Verification**:
```bash
clang --version  # Should be 16.0.0 or later
llvm-ar --version
llvm-objcopy --version
```

---

## Quick Start

1. **Open the Build Tab**: Select your target kernel (e.g., `linux`, `linux-zen`, `linux-hardened`).
2. **Choose a Profile**: Select one of the four optimization profiles:
   - **Gaming**: Low-latency optimizations for real-time performance
   - **Workstation**: Security-hardened configuration for developers
   - **Server**: Maximum throughput for datacenter workloads
   - **Laptop**: Battery-optimized with power-efficient scheduling

3. **Configure Options**:
   - **Modprobed-DB**: Enable automatic hardware-aware module filtering (~170 modules instead of 6,000+)
   - **LTO Level**: Choose None, Thin, or Full (profile defaults apply, but you can override)
   - **Desktop Experience Whitelist**: Ensure critical drivers (Ext4, BTRFS, USB HID, etc.) are always included

4. **Click Build**: The system will:
   - Detect your hardware (CPU, GPU, storage, bootloader)
   - Apply profile optimizations
   - Filter kernel modules (if enabled)
   - Compile the kernel (10–50 minutes depending on profile and hardware)

5. **Installation**: After build completion, navigate to the **Kernel Manager** tab to install the new kernel.

---

## Optimization Profiles

### Gaming Profile

**Purpose**: Optimized for low-latency gaming, esports, and real-time applications.

**Default Settings**:
- **Scheduler**: EEVDF (kernel baseline) + optional SCX userspace BPF scheduler (e.g., `scx_bpfland`)
- **LTO**: Thin (balanced compile time vs. optimization)
- **Timer Frequency**: 1000 Hz (maximum responsiveness)
- **Preemption**: Full preemption for instant task switching
- **MGLRU**: Enabled with aggressive memory reclaim (`enabled_mask=0x0007, min_ttl_ms=1000`)
- **Compiler Flags**: Polly loop optimization (`-mllvm -polly`)
- **Estimated Build Time**: 10–15 minutes

**Ideal For**:
- Competitive gaming (low input lag, frame consistency)
- Audio production and real-time synthesis
- Interactive workloads where responsiveness is critical

**Example**: Activating SCX with Gaming profile:
```
Profile: Gaming
SCX Scheduler Mode: Gaming (auto-selects scx_bpfland)
Result: Kernel uses EEVDF baseline + scx_bpfland for responsive workload detection
```

---

### Workstation Profile

**Purpose**: Balanced configuration for professional development and general computing with strong security.

**Default Settings**:
- **Scheduler**: EEVDF baseline (standard fair scheduler)
- **LTO**: Thin
- **Timer Frequency**: 1000 Hz
- **Preemption**: Full preemption
- **Hardening**: Standard (stack canaries, ASLR, DEP enabled)
- **Compiler Flags**: Polly loop optimization
- **Estimated Build Time**: 12–18 minutes

**Ideal For**:
- Software developers
- General desktop computing
- Balanced security and performance needs

---

### Server Profile

**Purpose**: Throughput-optimized kernel for datacenter, enterprise, and high-load scenarios.

**Default Settings**:
- **Scheduler**: EEVDF baseline (no SCX by default; maximum determinism)
- **LTO**: Full (maximum optimization, longer compile time)
- **Timer Frequency**: 100 Hz (reduced scheduling overhead)
- **Preemption**: None (server mode, no preemption)
- **Compiler Flags**: Polly loop optimization
- **Estimated Build Time**: 30–50 minutes (Full LTO is slower)

**Ideal For**:
- Web servers and application servers
- Databases and data processing
- High-throughput datacenter workloads where latency consistency is less critical

---

### Laptop Profile

**Purpose**: Power-efficient kernel that extends battery life while maintaining responsiveness.

**Default Settings**:
- **Scheduler**: EEVDF baseline + optional SCX power-efficient strategy (e.g., `scx_rustland`)
- **LTO**: Thin
- **Timer Frequency**: 300 Hz (reduced timer interrupts, lower power draw)
- **Preemption**: Voluntary (balance between responsiveness and power)
- **MGLRU**: Enabled with aggressive reclaim for faster page allocation under memory pressure
- **Compiler Flags**: Polly loop optimization
- **Estimated Build Time**: 10–15 minutes

**Ideal For**:
- Laptops and mobile workstations
- Ultrabooks and low-power systems
- Extended battery life without sacrificing usability

---

## The Gauntlet Benchmark

**What is it?** A standardized 60-second performance evaluation sequence that stresses different kernel subsystems in 10-second intervals, providing a **GOAT Score** (0–1000) that classifies your kernel's quality.

### Running the Benchmark

1. **Navigate to the Performance Tab** in the egui interface.
2. **Start The Gauntlet**: Click "Run Benchmark" and select "The Gauntlet (60s)".
3. **Wait 60 seconds**: The system will execute 6 consecutive 10-second stress phases:
   - **Phase 1 (Baseline)**: Idle calibration (zero-load baseline)
   - **Phase 2 (Computational Heat)**: Heavy AVX/AVX-512 vectorization stress
   - **Phase 3 (Memory Saturation)**: Sequential and random memory access
   - **Phase 4 (Scheduler Flood)**: Rapid context switching and process spawning
   - **Phase 5 (Gaming Simulator)**: Bursty loads with high-priority interruptions
   - **Phase 6 (The Gauntlet)**: Simultaneous CPU, Memory, and I/O saturation

### Interpreting Results

#### GOAT Score Tiers

| Score Range | Tier | Classification | Description |
|---|---|---|---|
| 900–1000 | 🐐 **GOAT** | Platinum Grade | Laboratory-grade scheduling purity; zero-stutter kernel |
| 750–899 | 🥇 **Gold** | Professional | Exceptional responsiveness and reliable frame pacing |
| 500–749 | 🥈 **Silver** | Balanced | Solid versatile performance with minor variances |
| 0–499 | 🥉 **Bronze** | Baseline | Detectable latency jitter or thermal limitations |

#### Understanding the 7-Metric Spectrum

The Gauntlet measures seven critical metrics:

1. **Latency** (27% weight): P99.9 response time (optimal: 10µs)
   - Measures how quickly the kernel responds to interrupts and task scheduling
   - Lower is better; target <50µs for gaming

2. **Consistency** (18% weight): Coefficient of Variation (CV%) across all samples (optimal: 2%)
   - Measures stability and predictability of latency
   - "Silent Kernel" validation; lower jitter = better frame pacing

3. **Jitter** (15% weight): Standard deviation of latency (optimal: 1µs)
   - Measures the spread of response times
   - Critical for gaming and audio applications

4. **Throughput** (10% weight): Operations per second (optimal: 1.0M+ ops/sec)
   - Measures maximum sustainable workload capacity

5. **Efficiency** (10% weight): Context-switch latency (optimal: 1µs)
   - Measures scheduling overhead when switching between tasks

6. **Thermal** (10% weight): CPU temperature under stress (optimal: 40–60°C)
   - Monitors cooling efficiency and thermal throttling

7. **SMI Resilience** (10% weight): System Management Interrupt detection (optimal: 0 SMIs)
   - Detects firmware interference; higher = more firmware jitter

### Example Results

After running The Gauntlet on a Gaming profile build:

```
GOAT Score: 876 (Gold Tier)
├─ Latency:     P99.9 = 22µs ✅ (Score: 0.96)
├─ Consistency: CV = 3.2% ✅ (Score: 0.88)
├─ Jitter:      σ = 2.1µs ✅ (Score: 0.92)
├─ Throughput:  1.2M ops/sec ✅ (Score: 0.98)
├─ Efficiency:  2.3µs/switch ✅ (Score: 0.91)
├─ Thermal:     52°C ✅ (Score: 0.85)
└─ SMI Res:     1 SMI detected ✅ (Score: 0.80)
```

This result indicates a **Gold-tier kernel** suitable for professional gaming and real-time workloads.

---

## Performance Diagnostics

### Real-Time Monitoring

The **Performance Dashboard** provides live telemetry during your kernel's operation:

- **Max Latency**: Highest recorded response time
- **P99.9 Latency**: 99.9th percentile (captures outliers)
- **Average Latency**: Mean response time
- **Jitter (σ)**: Standard deviation of latency samples
- **CPU Thermal Heatmap**: Per-core temperature under stress
- **Consistency (CV%)**: Coefficient of Variation for stability analysis

### Kernel Battle Test

Compare the performance of multiple kernels side-by-side:

1. **Select Baseline Kernel**: Choose a reference kernel to compare against
2. **Select Test Kernel**: Choose the kernel you want to measure
3. **Run Battle Test**: Click "Compare" to execute identical stress profiles on both
4. **View Results**: Delta bars show performance delta (% improvement or regression)

Example output:
```
Baseline: linux-zen-stock (GOAT Score: 742)
Test:     linux-zen-goatd-gaming (GOAT Score: 876)
Delta:    +18% improvement (Gold tier vs Silver tier)
```

---

## Kernel Management

The **Kernel Manager** tab provides tools for discovering, installing, and maintaining system kernels.

### System Kernel Discovery

GOATd automatically scans and catalogs all installed kernels:

1. **Navigate to Kernel Manager Tab**
2. **View Installed Kernels**: See all available kernel variants with version numbers and installation status
3. **Boot Status Indicator**: Identifies which kernel is currently running (🔒 for booted kernel, protected from deletion)

### Installing a Compiled Kernel

After building a custom kernel:

1. **Recent Builds Section**: Shows your newly compiled kernels
2. **Click Install**: Stages the kernel to `/boot/efi` (systemd-boot) or `/boot` (GRUB)
3. **Post-Install Verification**: Validates:
   - Bootloader entry created successfully
   - All kernel modules accessible
   - DKMS drivers (e.g., nvidia-open-dkms) linked correctly

### Managing Multiple Kernels

- **Boot Selection**: Set a kernel as default in GRUB/systemd-boot without rebuilding
- **Safe Deletion**: Prevents accidental deletion of the currently booted kernel
- **Deep Audit**: Inspect detailed kernel configuration, compiler version, LTO settings, and scheduler status

---

## Sched_ext Scheduler Management

GOATd supports modern **userspace BPF-based schedulers** (sched_ext/SCX) that run OUTSIDE the kernel, allowing runtime switching without recompilation.

### What is Sched_ext?

- **Baseline Kernel Scheduler**: All kernels use the fair **EEVDF** scheduler (Linux 6.7+ default)
- **SCX Enhancement**: Optional userspace BPF programs provide specialized strategies:
  - `scx_bpfland`: Automatic workload balancing (Gaming, Workstation)
  - `scx_lavd`: Ultra-low latency for interactive tasks (Laptop)
  - `scx_rustland`: Power-efficient scheduling (Laptop, Server)

### SCX Scheduler Modes

Navigate to the **Scheduler Tab** in the UI for convenient one-click management:

| Mode | Scheduler | Use Case | Profile Default |
|---|---|---|---|
| **Auto** | `scx_bpfland` | Automatic workload detection | Gaming, Workstation |
| **Gaming** | `scx_bpfland` | Low-latency real-time | Gaming |
| **LowLatency** | `scx_lavd` | Ultra-responsive interactive | Workstation |
| **PowerSave** | `scx_rustland` | Extended battery life | Laptop |
| **Server** | (disabled EEVDF) | Maximum throughput | Server |

### One-Click Activation

1. **Open Scheduler Tab**
2. **Select Mode**: Choose from the dropdown (Gaming, LowLatency, etc.)
3. **Toggle SCX**: Click "Enable SCX" (Polkit will prompt for authorization once)
4. **Immediate Effect**: SCX scheduler loads via `scx_loader.service` without rebooting

### Configuration File

Under the hood, SCX configuration is stored at:
```
/etc/scx_loader/config.toml
```

Manual edits (for advanced users):
```toml
[scheduler]
mode = "gaming"  # or: auto, lowlatency, powersave, server

[modes]
gaming = "scx_bpfland"
lowlatency = "scx_lavd"
powersave = "scx_rustland"
server = "disabled"
```

---

## Advanced Build Options

### Modprobed-DB (Hardware-Aware Module Filtering)

**What it does**: Automatically detects which drivers your hardware uses and filters the kernel to include ONLY those modules, reducing the kernel from ~6,000+ modules to ~170.

**Benefits**:
- **Faster compilation**: 70% faster builds (10–15 minutes vs. 45+ minutes)
- **Smaller binary**: ~20–30 MB savings
- **Performance**: Kernel is optimized for YOUR hardware, not a generic system

**Enable it**:
1. Check "Modprobed-DB" in Build settings
2. Ensure `~/.config/modprobed.db` exists on your system:
   ```bash
   # modprobed-db automatically maintains this file with drivers you've used
   # If it doesn't exist, it will be created after your first use
   ```

### Desktop Experience Whitelist

**What it does**: Ensures that critical drivers for bootability and desktop functionality are ALWAYS included, even with aggressive modprobed-db filtering.

**Included Drivers** (22 essential):
- **HID** (keyboards, mice): `hid`, `hid-generic`, `evdev`, `usbhid`
- **Storage** (boot devices): `nvme`, `ahci`, `libata`, `scsi`
- **Filesystems** (root FS): `ext4`, `btrfs`, `vfat`, `exfat`, `nls_cp437`, `nls_iso8859_1`
- **USB Support** (external drives): `usb_core`, `usb_storage`, `xhci_hcd`, `ehci_hcd`, `ohci_hcd`

**Enable it**:
1. Check "Modprobed-DB" first (prerequisite)
2. Check "Desktop Experience Whitelist"
3. Result: modprobed modules + whitelist drivers = safe, fast kernel

### LTO (Link-Time Optimization) Levels

**What it does**: Applies whole-program optimization to the kernel, trading compile time for runtime performance.

**Options**:
- **None**: Baseline (fastest compile, baseline performance)
- **Thin**: Balanced—recommended for Gaming/Workstation/Laptop (10–15 min overhead)
- **Full**: Aggressive optimization for maximum performance (30–50 min overhead), used by Server profile

**How it works**:
1. Profile has a **default** LTO level (e.g., Gaming=Thin, Server=Full)
2. You can **override** before building to any of the three options
3. LTO is enforced through the **5-phase hard enforcer** protection pipeline to survive all kernel configuration regeneration

**Example scenario**:
```
Default Profile: Gaming (Thin LTO)
My preference:   Full LTO
Action:          Select "LTO: Full" before clicking Build
Result:          Kernel compiled with Full LTO despite Gaming default
```

### GPU Driver Auto-Exclusion

When using **Modprobed-DB**, GOATd automatically excludes GPU drivers that don't match your detected hardware:

- **NVIDIA GPU detected**: Excludes AMD (`amdgpu`, `radeon`) and Intel (`i915`, `xe`) drivers
- **AMD GPU detected**: Excludes NVIDIA (`nouveau`, `nvidia`) and Intel (`i915`, `xe`) drivers
- **Intel iGPU detected**: Excludes NVIDIA and AMD drivers
- **No GPU**: Excludes all 6 GPU drivers (headless systems)

**Result**: 3–5% additional kernel size reduction, further optimized for your hardware.

---

## Troubleshooting

### Build Fails with "LTO Linking Timeout"

**Cause**: LTO (especially Full) requires significant memory during linking.

**Solution**:
1. Reduce background processes to free RAM
2. Switch to **Thin LTO** or **None** for faster compilation
3. Increase system swap if RAM is permanently limited

### Modprobed-DB Shows "WARNING: localmodconfig failed"

**Cause**: `~/.config/modprobed.db` not found or invalid permissions.

**Solution**:
1. Create the file using:
   ```bash
   modprobed-db store  # Creates the initial database
   ```
2. Ensure it's readable by your user:
   ```bash
   chmod 600 ~/.config/modprobed.db
   ```
3. Retry the build

### Kernel Won't Boot After Installation

**Cause**: Critical drivers (filesystems, storage) excluded by modprobed-db filtering.

**Solution**:
1. Re-enable **Desktop Experience Whitelist** (ensures Ext4, USB, etc.)
2. Or disable **Modprobed-DB** entirely to get the full kernel
3. Reinstall with updated settings

### High Jitter in Benchmark Results (Bronze/Silver Score)

**Cause**: Firmware interference (SMI), thermal throttling, or background processes.

**Solutions**:
- Check CPU thermal status (may be throttling under stress)
- Disable CPU frequency scaling while benchmarking
- Close background applications
- Verify no thermal paste degradation

### SCX Scheduler Fails to Load

**Cause**: SCX support not compiled into kernel, or `scx_loader.service` not available.

**Solution**:
1. Ensure your kernel was built with `CONFIG_SCHED_EXT=y`
2. Install `scx_loader` system package:
   ```bash
   sudo pacman -S scx-scheds
   ```
3. Enable service:
   ```bash
   sudo systemctl enable --now scx_loader.service
   ```

---

## Advanced Topics

### CONFIG_CMDLINE Bake-In

Critical parameters are hardcoded into the kernel binary via `CONFIG_CMDLINE`, ensuring they're enforced at runtime independent of bootloader settings:

- **Always injected**: `nowatchdog`, `preempt=full`
- **Gaming profile**: `mitigations=off` (performance trade-off for Spectre/Meltdown mitigations)
- **MGLRU-enabled profiles**: `lru_gen.enabled=7` (activates all MGLRU subsystems)

### The 5-Phase Hard Enforcer Protection

GOATd uses **five protection gates** during kernel compilation to ensure configuration survives aggressive Kconfig regeneration:

1. **Phase 5 (Orchestrator)**: Regex-based removal + atomic Clang/LTO injection at pre-build time
2. **Phase G1 (Prebuild LTO)**: Final LTO enforcement immediately before `make bzImage`
3. **Phase G2 (Post-Modprobed)**: Recovery from Kconfig dependency re-expansion after module filtering
4. **Phase G2.5 (Post-Setting)**: Recovery from Arch's destructive `cp ../config .config` overwrite
5. **Phase E1 (Post-Oldconfig)**: Re-applies LTO if Kconfig reverts to `CONFIG_LTO_NONE=y`

**Result**: Your kernel configuration survives all configuration steps and compiles with exact settings you selected.

---

## Getting Help

- **GitHub Issues**: Report bugs or request features via GitHub Issues on the project repository
- **DEVLOG.md**: Complete development history and technical architecture decisions
- **BLUEPRINT_V2.md**: Authoritative technical specification for advanced users
- **Project Scope**: See [`PROJECTSCOPE.md`](../PROJECTSCOPE.md) for architectural details
