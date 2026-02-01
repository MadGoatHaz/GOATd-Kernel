# GOATd Kernel Builder: User Guide (v0.2.1)

Welcome to GOATd Kernel Builder—a comprehensive toolkit I've developed for building, customizing, and managing Linux kernels tailored to your hardware and workload. This guide walks you through installation, basic usage, and advanced features.

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
- **Rust 2021 Edition**
- **Clang 16+** (for kernel compilation)
- **sudo privileged access** (for kernel build and installation)

### Installation: Running GOATd Kernel Builder

**Automatic Setup (Arch Linux - Recommended)**:

Simply run:
```bash
./goatdkernel.sh
```

My launcher script will automatically:
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
   ```

3. Build and launch the application:
   ```bash
   cd /path/to/GOATd\ Kernel
   ./goatdkernel.sh
   # Or manually: cargo build --release && ./target/release/goatd_kernel
   ```

4. The **egui** interface will launch in a native window. You'll be prompted for sudo credentials for administrative operations.

5. Grant **Polkit authorization** for privileged operations (one-time setup):
   - Policy file: `assets/com.goatd.kernel.policy`
   - Actions: `com.goatd.kernel.build`, `com.goatd.kernel.provision`, `com.goatd.kernel.install`

---

### Modprobed-DB Auto-Initialization

**What it does**: On **Arch Linux systems**, my launcher script automatically initializes `modprobed-db` if it's installed.

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

**Why LLVM matters**: I enforce **Clang-based compilation exclusively** for all kernels.

**Why this is important**:
- **Polly Loop Optimization**: Advanced vectorization via `-mllvm -polly` flags for better CPU cache utilization
- **LTO (Link-Time Optimization)**: Full whole-program optimization requires LLVM toolchain
- **Modern ISA Support**: Proper `-march` targeting for AVX-512, other modern CPU features
- **Consistent Optimization**: No ambiguity from GCC vs. Clang differences

**Automatic Detection & Installation**:
- On Arch Linux, `./goatdkernel.sh` automatically installs: `llvm`, `clang`, `lld`, `polly`
- If you are not on Arch, you **must** install these packages manually before building

**Verification**:
```bash
clang --version  # Should be 16.0.0 or later
```

---

## Quick Start

1. **Open the Build Tab**: Select your target kernel (e.g., `Mainline RC`, `Linux`, `LTS`, `Hardened`).

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

5. **Installation**: After build completion, navigate to the **Kernel Manager** tab to install and manage your kernels.(DO NOT UNINSTALL THE WRONG KERNEL)

---

## Optimization Profiles

### 🎮 Gaming
**Purpose**: Extreme responsiveness and frame-pacing consistency.
- **LTO**: Thin (balanced speed/performance)
- **Scheduler**: EEVDF (kernel baseline) + optional SCX userspace BPF scheduler (e.g., `scx_bpfland`)
- **Timer Frequency**: 1000 Hz (maximum granularity)
- **Preemption**: Full (Real-Time behavior)
- **Mitigations**: Forced OFF (maximum performance)

**Use Case**: Competitive gaming and latency-sensitive media production.

### 🛠️ Workstation
**Purpose**: Balanced performance with developer-centric security.
- **LTO**: Thin
- **Hardening**: Moderate (Control Flow Integrity enabled)
- **Preemption**: Voluntary
- **MGLRU**: Enabled with performance tiering

**Use Case**: Software development, compilation-heavy workloads, and general multitasking.

### 🗄️ Server
**Purpose**: Maximum I/O throughput and multi-tenant isolation.
- **LTO**: Full (maximum binary optimization)
- **Scheduler**: EEVDF baseline + `scx_rustland` for throughput balancing
- **Timer Frequency**: 250 Hz (reduced context switch overhead)
- **Estimated Build Time**: 30–50 minutes (Full LTO is slower)

**Use Case**: Web servers and application servers.

### 💻 Laptop
**Purpose**: Power-efficient kernel that extends battery life while maintaining responsiveness.
- **LTO**: Thin
- **Scheduler**: EEVDF baseline + optional SCX power-efficient strategy (e.g., `scx_rustland`)
- **Timer Frequency**: 300 Hz (reduced timer interrupts, lower power draw)
- **Preemption**: Voluntary (balance between responsiveness and power)

**Use Case**: Ultrabooks and low-power systems where battery life is critical.

---

## The Gauntlet Benchmark

**What is it?** A standardized 60-second performance evaluation sequence I've built that stresses different kernel subsystems in 10-second intervals, providing a **GOAT Score** (0–1000) that classifies your kernel's quality.

### The 6 Phases of The Gauntlet:
1. **Phase 1 (Latency)**: Micro-benchmark for task-switching response
2. **Phase 2 (Throughput)**: Raw I/O and syscall capacity
3. **Phase 3 (Jitter)**: Measurement of timing variance under moderate load
4. **Phase 4 (Efficiency)**: Context-switch overhead measurement
5. **Phase 5 (Thermal)**: CPU temperature stability under sustained load
6. **Phase 6 (The Gauntlet)**: Simultaneous CPU, Memory, and I/O saturation

### Interpreting Your GOAT Score:
- **900–1000 (GOAT Status)**: Perfect optimization for your hardware.
- **750–899 (Diamond)**: Exceptional performance; ready for any workload.
- **500–749 (Gold)**: Solid performance, standard for most builds.
- **Below 500 (Silver/Bronze)**: Potential configuration issues or thermal throttling detected.

---

## Performance Diagnostics

The **Performance Dashboard** provides live telemetry during your kernel's operation:

### The 7-Metric Spectrum
I've implemented seven horizontal strips visualizing system behavior across different dimensions:
1. **Latency**: P99.9 response time (optimal: 10µs)
2. **Consistency**: CV% across all samples (optimal: 2%)
3. **Jitter**: Standard deviation of latency (optimal: 1µs)
4. **Throughput**: Operations per second (optimal: 1.0M+ ops/sec)
5. **Efficiency**: Context-switch latency (optimal: 1µs)
6. **Thermal**: CPU temperature under stress (optimal: 40–60°C)
7. **SMI Resilience**: System Management Interrupt detection (optimal: 0 SMIs)

---

## Kernel Management

The **Kernel Manager** tab is where you handle the lifecycle of your builds:

1. **Installation**: Deploy your newly built kernels to `/boot` with automated initramfs generation and bootloader updates (supports GRUB, systemd-boot, and rEFInd).
2. **View Installed Kernels**: See all available kernel variants with version numbers and installation status.
3. **Boot Status Indicator**: Identifies which kernel is currently running (🔒 for booted kernel, protected from deletion).
4. **Deep Audit**: Inspect detailed kernel configuration, compiler version, LTO settings, and scheduler status.

---

## Sched_ext Scheduler Management

GOATd supports modern **userspace BPF-based schedulers** (sched_ext/SCX) that run OUTSIDE the kernel, allowing runtime switching without recompilation.

### How it works:
- **Baseline Kernel Scheduler**: All kernels built use the fair **EEVDF** scheduler (Linux 6.7+ default).
- **SCX Enhancement**: Optional userspace BPF programs provide specialized strategies:
  - `scx_bpfland`: Responsive scheduling for desktop/gaming (Gaming, Workstation).
  - `scx_rustland`: Power-efficient scheduling (Laptop, Server).
- **Dynamic Switching**: You can swap schedulers in real-time via the Performance tab without a reboot.

---

## Advanced Build Options

### Desktop Experience Whitelist

**What it does**: Specifically designed to work with Modprobed-DB. It ensures that critical drivers required for a functional desktop experience are NEVER excluded, even if they aren't currently loaded in your system state.

**I recommend ensuring these stay included**:
- **Filesystems** (root FS): `ext4`, `btrfs`, `vfat`, `exfat`, `nls_cp437`, `nls_iso8859_1`
- **USB Support** (external drives): `usb_core`, `usb_storage`, `xhci_hcd`, `ehci_hcd`, `ohci_hcd`
- **HID** (keyboards, mice): `hid`, `hid-generic`, `evdev`, `usbhid`

### LTO (Link-Time Optimization) Levels

**What it does**: Applies whole-program optimization to the kernel, trading compile time for runtime performance.

**Options**:
- **None**: Baseline (fastest compile, baseline performance)
- **Thin**: Balanced—recommended for Gaming/Workstation/Laptop (10–15 min overhead)
- **Full**: Aggressive optimization for maximum performance (30–50 min overhead), used by Server profile

### GPU Driver Auto-Exclusion

When using **Modprobed-DB**, GOATd automatically excludes GPU drivers that don't match your detected hardware to further reduce kernel size.

---

## Troubleshooting

### Build Fails with "LTO Linking Timeout"
**Cause**: LTO (especially Full) requires significant memory during linking.
**Solution**: Scale back to **Thin LTO** or **None** for faster compilation.

### Modprobed-DB Shows "WARNING: localmodconfig failed"
**Cause**: `~/.config/modprobed.db` not found or invalid permissions.
**Solution**: Run `modprobed-db store` to create the initial database.

### Kernel Won't Boot After Installation
**Cause**: Critical drivers (filesystems, storage) excluded by modprobed-db filtering.
**Solution**: Re-enable **Desktop Experience Whitelist** (ensures Ext4, USB, etc.) and rebuild.

### SCX Scheduler Fails to Load
**Cause**: SCX support not compiled into kernel, or `scx-scheds` package not installed.
**Solution**: Ensure your kernel was built with `CONFIG_SCHED_EXT=y` and you have `scx-scheds` installed on your system.

---

## Getting Help

- **GitHub Issues**: Report bugs or request features via the project repository.
- **DEVLOG.md**: My development history and technical architecture decisions.
- **Project Scope**: See [`PROJECTSCOPE.md`](../PROJECTSCOPE.md) for architectural details.
