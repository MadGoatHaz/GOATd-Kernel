# GOATd Kernel: Complete User Guide

**Start here for setup, performance optimization, and troubleshooting.**

---

## Table of Contents

1. [Setup & Installation](#setup--installation)
2. [Environment Purity & Workspace Management](#environment-purity--workspace-management) (NEW)
3. [Understanding modprobed-db](#understanding-modprobed-db)
4. [Understanding Performance Metrics](#understanding-performance-metrics)
5. [The GOAT Recipe: Optimal Configuration](#the-goat-recipe-optimal-configuration)
6. [Dashboard Flow & First-Run Guide](#dashboard-flow--first-run-guide)
7. [Troubleshooting](#troubleshooting)
8. [Advanced Topics](#advanced-topics)

---

## Setup & Installation

### System Requirements

GOATd Kernel requires:
- **Linux**: Arch Linux or Arch-based distribution (Manjaro, EndeavourOS, etc.)
- **System update**: Current package database
- **Rust toolchain**: For compiling the GOATd application
- **LLVM/Clang 16+**: For kernel compilation (automatically detected/installed)
- **Base Development Tools**: `base-devel` group (pacman, git, gcc, make, etc.)

### Pre-Installation Checklist

Before running GOATd Kernel for the first time, perform these critical steps:

#### 1. ⚠️ MANDATORY: Update Your System

This step is **non-negotiable** and must be completed before anything else:

```bash
sudo pacman -Syu
```

**Why this cannot be skipped**: Outdated packages cause compilation failures, kernel build errors, and LTO optimization issues. This is the #1 cause of build failures. If your kernel build fails, the first troubleshooting step is always: **run `sudo pacman -Syu` again**.

#### 2. Install Rust (if needed)

Check if Rust is installed:

```bash
rustup --version
```

If not found, install it:

```bash
sudo pacman -S rustup
rustup default stable
```

Verify installation:

```bash
cargo --version
```

### First-Time Startup

Once prerequisites are met, clone or navigate to the GOATd Kernel directory and run:

```bash
./goatdkernel.sh
```

This script will:
1. Verify system prerequisites (Rust, LLVM, base-devel)
2. Install missing packages via `pacman` (requires sudo/password)
3. Configure GPG keys for kernel signature verification
4. Build the GOATd Kernel application
5. Launch the GUI dashboard

**The entire process takes 5-15 minutes depending on your internet connection and system speed.**

### First-Start: Dashboard Orientation

After the GUI launches, you'll see the **Dashboard Flow**. Follow these steps in order:

1. **Health Check Tab** — Review system status and installed components
2. **Fix Environment Tab** — Resolve any warnings or missing dependencies
3. **Manual AUR Steps Tab** — Install `modprobed-db` from AUR (see below)
4. **Baseline Audit Tab** — Scan your hardware and establish a performance baseline
5. **Kernel Selection & Build** — Choose a profile and build your first kernel
6. **Benchmark Your Kernel** — Run the GOATd Full Benchmark to measure results

See [Dashboard Flow & First-Run Guide](#dashboard-flow--first-run-guide) for detailed walkthrough of each tab.

---

## Environment Purity & Workspace Management

### Understanding Environment Purity Controls

GOATd Kernel enforces **hermetically sealed build environments** to prevent contamination from system-wide compiler flags or variables that could interfere with kernel compilation:

#### What is Environment Purity?

The build system creates a "clean room" compilation context by:
- **Explicit Variable Management**: All CFLAGS, LDFLAGS, and compiler flags defined centrally (not inherited from system defaults)
- **PATH Purification**: GCC directories removed; only Clang/LLVM tools available in build PATH
- **Global Clang Enforcement**: All build phases mandatory use Clang via `_FORCE_CLANG=1` environment variable
- **Bytecode Cache Prevention**: `PYTHONDONTWRITEBYTECODE=1` prevents Python tools from contaminating build artifacts with cached `.pyc` files

#### Impact on Your Builds

This environment purity provides:
- **Reproducible Builds**: Same configuration produces identical binaries across systems
- **Deterministic Compilation**: No surprise system-wide CFLAGS affecting your kernel optimization
- **Clean Artifacts**: No path leaks or build metadata embedded in final kernel images
- **Safe Cross-System Deployments**: Kernels built on one system work reliably on others

### Workspace Root & Build Pipeline

GOATd Kernel manages builds in isolated workspaces to maintain clean separation between:
- **Source Checkout**: Temporary kernel source from upstream repositories
- **Build Artifacts**: Compiled kernel and modules
- **Caching**: ccache and build state for incremental rebuilds
- **Logging**: Complete build transcripts for debugging and verification

#### Workspace Detection

The application automatically detects your workspace root via:
1. **Configuration File Location**: Looks for `.goatdrc` or similar marker in project root
2. **Fallback Detection**: Uses current working directory or environment variable `GOATD_HOME`
3. **Canonical Path Resolution**: Converts all paths to absolute filesystem paths to prevent symlink issues

If you move the GOATd Kernel directory, workspace detection automatically adjusts. For custom workspace locations, set the `GOATD_HOME` environment variable before launching.

#### Build Pipeline Guarantee

All critical operations run through the **Unified Surgical Engine** ([`KernelPatcher`](src/kernel/patcher.rs)) which ensures:
- **Atomic File Operations**: `.config` and PKGBUILD modifications use atomic swaps with full backups
- **Verification Before Commit**: All patches validated via dry-run before actual file modification
- **Rollback Capability**: Original files preserved; easy revert if build fails verification
- **Audit Trail**: All modifications logged with timestamps and context for future reference

---

## Understanding modprobed-db

### What is modprobed-db?

`modprobed-db` is an Arch Linux utility that maintains a database of kernel modules loaded on your system. It enables **hardware-aware kernel module filtering**, reducing the massive Linux kernel module set from **6500+ to fewer than 200 modules — a 97% reduction**.

### The Critical Problem It Solves

A vanilla Linux kernel contains **6500+ loadable kernel modules (LKMs)**, but your hardware only uses a small fraction of them. **Including all 6500+ modules creates a compounding performance penalty**:

- **Massive compilation time**: 60-120+ minutes without filtering
- **LTO explodes the build time**: Link-Time Optimization must process all 6500+ modules, creating exponential compilation cost
- **Bloated kernel size**: Wastes storage and transfer bandwidth
- **Excessive boot overhead**: More modules = slower initialization
- **Wasted system resources**: Compilation stalls on irrelevant hardware drivers

### The modprobed-db Solution: 97% Reduction with Compounding Benefits

By filtering your kernel to include **only the modules your hardware actually loads** (~<200 modules), you unlock **compounding performance gains**:

| Benefit | Impact | Why It Compounds |
|---------|--------|------------------|
| **Build Time Reduction** | 30-50% faster (20-40 min vs. 60-120 min) | Fewer modules = less work for compiler |
| **LTO Optimization Speed** | 40-60% faster LTO phase | LTO processes only ~200 modules instead of 6500 (32x fewer LTO targets) — this is the **largest single speedup** |
| **Boot Time** | 10-20% faster kernel boot | Smaller, focused module set initializes quicker |
| **Kernel Size** | 50-70% smaller (20MB vs. 60MB+) | Storage savings, faster distribution |
| **Compilation Resource Usage** | 60-70% less RAM/CPU stress | System remains responsive during builds |
| **Compounding Effect Together** | **All benefits amplify each other** | Smaller module set → Faster LTO → Faster compile → Faster boot → Faster iteration cycles |

### Why Manual Installation?

GOATd Kernel **requires manual AUR installation of modprobed-db** for important reasons:

1. **User Control**: You decide which modules to include via the whitelist strategy
2. **Transparency**: You see exactly which modules are being filtered
3. **Safety Net**: The "Desktop Experience" whitelist ensures critical filesystems, USB, and EFI support remain available even if auto-discovery misses something
4. **Flexibility**: You can customize the filtering strategy after installation

### Installation Steps

#### Step 1: Clone the modprobed-db Repository

```bash
git clone https://aur.archlinux.org/modprobed-db.git
cd modprobed-db
```

#### Step 2: Build and Install

```bash
makepkg -si
```

- `-s` installs dependencies via pacman
- `-i` installs the package after building

#### Step 3: Initialize the Module Database

Once installed, populate the database with currently loaded modules:

```bash
modprobed-db store
```

This captures a snapshot of all modules your system currently has loaded. Run this periodically (especially after installing new hardware or drivers) to keep the database current.

### How GOATd Kernel Uses modprobed-db

After you install modprobed-db, GOATd Kernel will:

1. **Detect modprobed-db installation** during Health Check
2. **Enable filtering options** in the kernel build configuration
3. **Apply the whitelist strategy** (if selected) to ensure critical drivers remain available
4. **Build a highly optimized, hardware-specific kernel** with only necessary modules

### Recommended Usage: The Whitelist Strategy

When building your kernel, select the **whitelist strategy**. This applies the "Desktop Experience" safety-net whitelist, which includes:

- **Critical Filesystems**: Ext4, FAT, VFAT, ISO9660, CIFS
- **EFI Support**: NLS (National Language Support) for ASCII, CP437
- **Storage Drivers**: NVMe, AHCI/SATA controllers
- **Input Devices**: USB HID (keyboards, mice, controllers)
- **Virtual Devices**: Loopback device, UEFI support

This ensures your kernel boots reliably on any desktop system while still benefiting from module filtering.

---

## Understanding Performance Metrics

### The 7-Core Performance Metrics

GOATd Kernel measures kernel performance across 7 weighted metrics, each targeting a different aspect of system behavior:

#### 1. **Latency (27% Weight)**

**What It Measures**: The time between a stimulus (input, system call, interrupt) and the kernel's response, measured at the P99 percentile.

**Why It Matters**:
- **Gaming**: Lower latency = more responsive controls, higher competitive advantage
- **Desktop**: Snappier UI responsiveness, better user experience
- **Real-time Applications**: Audio production, video editing, scientific computing
- **Everyday Use**: Noticeable difference in system "feel"

**How to Optimize**:
- Enable low-latency scheduler (BORE, bpfland SCX)
- Use Gaming or Workstation profile
- Set 1000Hz timer frequency
- Enable preemption mode

#### 2. **Consistency (18% Weight)**

**What It Measures**: Stability of latency across samples, using Coefficient of Variation (CV %). Lower CV% = more consistent performance.

**Formula**: `Score = 1.0 - (CV - 0.05) / 0.25` with a 5% noise floor and 2%-30% measurement range

**Why It Matters**:
- **Prevents Micro-Stutters**: Inconsistent latency causes frame drops and stuttering
- **Predictable Performance**: Applications can rely on consistent response times
- **Professional Workloads**: Video rendering, livestreaming, real-time synthesis
- **Gaming**: Stable FPS is more important than peak FPS

**How to Optimize**:
- Reduce thermal throttling (MGLRU, proper cooling)
- Minimize scheduler jitter
- Reduce system load variance
- Enable preemption modes

#### 3. **Jitter (15% Weight)**

**What It Measures**: Variance in latency between measurement runs, calibrated to P99.9 percentile with a 10µs optimal floor (the finest micro-stutter detection).

**Why It Matters**:
- **Gaming**: Detects frame-time variance at competitive levels
- **Audio**: Identifies timing drift in audio processing
- **Professional**: Reveals scheduling consistency under load
- **Perception**: The smallest jitter humans can perceive

**How to Optimize**:
- Enable MGLRU (Multi-Generation LRU) for consistent memory access
- Disable CPU frequency scaling during tests
- Run on isolated performance cores
- Minimize SMI (System Management Interrupt) events
- Enable SCX scheduler on auto mode

#### 4. **Throughput (10% Weight)**

**What It Measures**: Total number of operations (instructions, I/O completions, system calls) the kernel processes per unit time.

**Why It Matters**:
- **Server Workloads**: Maximizing operations per second is the primary goal
- **Bulk Data Processing**: Database operations, file serving, network throughput
- **Compilation**: Build speed, parallel job efficiency
- **Video Encoding**: Frames per second throughput

**How to Optimize**:
- Use Server or Laptop profile
- Enable Full LTO (longer compilation time, maximum optimization)
- Enable Polly loop vectorization
- Use EEVDF scheduler
- Increase timer frequency (300Hz+ for responsiveness)

#### 5. **Efficiency (10% Weight)**

**What It Measures**: Performance per watt of energy consumed. Higher efficiency = same performance with less power draw.

**Why It Matters**:
- **Laptop Battery Life**: Direct impact on unplugged usage time
- **Power Costs**: Significant for servers and datacenters
- **Thermal**: Lower power = lower heat = less cooling needed
- **Sustainability**: Reduce environmental impact

**How to Optimize**:
- Use Laptop or Server profile
- Enable voluntary preemption mode
- Enable power-efficient SCX scheduler option
- Use Thin LTO (faster optimization = less CPU cycles)
- Lower timer frequency (300Hz is efficient)

#### 6. **Thermal (10% Weight)**

**What It Measures**: Peak core temperature under maximum load and thermal headroom available before throttling begins.

**Why It Matters**:
- **Hardware Longevity**: Excessive heat degrades components
- **Throttling Prevention**: Hot CPUs slow down, killing performance
- **System Stability**: Extreme heat can trigger shutdowns
- **Cooling Requirements**: Lower temps = simpler cooling, quieter fans

**How to Optimize**:
- Enable MGLRU (reduces memory pressure, improves access patterns)
- Use Laptop profile with voluntary preemption
- Ensure adequate case airflow
- Reduce background processes during kernel work
- Monitor temps with `watch sensors`

#### 7. **SMI Resilience (10% Weight)**

**What It Measures**: How well the kernel handles System Management Interrupts (SMI) — uninterruptible hardware events that steal CPU cycles from your workload.

**Why It Matters**:
- **Latency-Critical Workloads**: SMI events cause unavoidable latency spikes
- **Trading Systems**: Even 100µs latency is costly
- **Experimental Science**: Requires deterministic timing
- **High-Frequency Workloads**: Any non-determinism is visible

**How to Optimize**:
- Minimize SMI-causing events (fewer power state changes, less thermal throttling)
- Use SCX scheduler with good SMI handling
- Disable CPU frequency scaling
- Reduce interrupt processing load
- Keep system thermals stable (SMI increases under thermal stress)

### GOAT Score: Composite Metric

The **GOAT Score** (0-1000) is a weighted average of all 7 metrics:

```
GOAT Score = (Latency×0.27) + (Consistency×0.18) + (Jitter×0.15) + 
             (Throughput×0.10) + (Efficiency×0.10) + (Thermal×0.10) + 
             (SMI_Resilience×0.10)
```

Each metric is normalized to 0-1000 before weighting.

### Interpreting Your GOAT Score

| Score Range | Classification | What It Means |
|-------------|-----------------|--------------|
| **0-250** | Minimal Optimization | Baseline or unoptimized kernel; significant room for improvement |
| **250-500** | Standard Kernel | Well-maintained vanilla kernel; reasonable performance |
| **500-750** | Well-Optimized Kernel | Effective customization; noticeable improvements in daily use |
| **750-1000** | Highly Tuned Kernel | Expert configuration; competitive-grade performance |

### Best Practices for Metric Interpretation

1. **Compare to Baseline**: Your baseline score (vanilla kernel) provides context
2. **Track Improvements**: Compare your custom kernel to your previous attempt
3. **Understand Trade-Offs**: Optimizing for latency may reduce throughput; choose your priorities
4. **Watch Thermal**: If thermal increases while optimizing, you're pushing too hard; consider better cooling
5. **Test Multiple Kernels**: Build 2-3 kernels with different profile/LTO combinations and benchmark each

---

## The GOAT Recipe: Optimal Configuration

### Purpose

The **GOAT Recipe** is the **single best configuration** for maximum performance across gaming, desktop, and general workloads. It combines:
- **Minimal hardening** (or standard for balanced security)
- **Full LTO** (maximum optimization)
- **Polly-enabled** loop vectorization
- **Native optimizations** (`-march=native` for your exact CPU)
- **MGLRU enabled** (consistency and jitter reduction)
- **modprobed-db filtering** (97% module reduction = 40-60% faster LTO)
- **bpfland SCX scheduler** on auto mode (best latency/throughput balance)

**Expected Results**: GOAT Score 750-900, P99 latency <100µs, CV% consistency <12%, competitive-grade performance.

### Prerequisites

1. ✅ System updated (`sudo pacman -Syu`) — **mandatory, do not skip**
2. ✅ Rust installed and verified
3. ✅ `modprobed-db` installed from AUR (via Manual AUR Steps in Dashboard) — **critical for the 40-60% LTO speedup**
4. ✅ Hardware modules captured (`modprobed-db store` run at least once)

### The Recipe: Configuration Checklist

Follow these steps **in exact order** in the GOATd Kernel dashboard. Do not deviate.

#### Step 1: Select Gaming Profile (MANDATORY)

In the **Kernel Selection** tab, select **Gaming Profile** — do not choose Workstation, Server, or Laptop.

**Exactly why**: Gaming Profile defaults to 1000Hz timer (responsiveness) + Thin LTO (reasonable compile time). You will override LTO to Full in Step 2.

#### Step 2: Override LTO Strategy to Full LTO (MANDATORY)

After selecting Gaming Profile, **change the LTO dropdown** from "Thin LTO" to **"Full LTO"**.

```
LTO Strategy: [Thin LTO] ← change this to ↓
LTO Strategy: [Full LTO] ← select this
```

**Why**: Full LTO compiles the entire kernel at link-time for maximum optimization. With modprobed-db reducing modules from 6500 to <200, even Full LTO is fast (30-40 min).

#### Step 3: Enable Polly Loop Vectorization (MANDATORY)

Find the **Polly** checkbox in the optimization section and **toggle it ON**.

```
☐ Polly  ← uncheck = NO
☑ Polly  ← check = YES (this is what you want)
```

**Why**: Polly enables `-mllvm -polly -mllvm -polly-vectorizer=stripmine` for advanced loop vectorization, adding 5-10% throughput directly.

#### Step 4: Enable Native CPU Targeting (MANDATORY)

Find the **Native CPU Targeting** checkbox (often labeled `-march=native` or similar) and **toggle it ON**.

```
☐ Native CPU Targeting  ← uncheck = NO
☑ Native CPU Targeting  ← check = YES (this is what you want)
```

**Why**: Compiles kernel instructions **specifically for your exact CPU model**. This unlocks 5-15% performance improvement by using your CPU's specific instruction sets (SSE, AVX, AVX2, AVX-512, etc.).

#### Step 5: Verify MGLRU is Enabled (MANDATORY)

In the memory management section, find **MGLRU (Multi-Generation LRU)** and **ensure it is enabled**.

```
MGLRU: [Enabled]  ← this is correct
MGLRU: [Disabled] ← do NOT use this
```

**Why**: MGLRU is the single largest contributor to consistency and jitter reduction. It dramatically improves memory access patterns under load, preventing stutters and frame drops.

#### Step 6: Configure Module Filtering with Whitelist (MANDATORY)

In the **Module Filtering** section:
- Verify `modprobed-db` shows as **installed**
- Select the **Whitelist Strategy** option explicitly

```
Module Filtering Strategy:
○ Auto-discovery only
◉ Auto-discovery + Whitelist (Desktop Experience)  ← select this
○ Disabled (use all 6500+ modules)
```

**Why**: The Whitelist Strategy applies modprobed-db filtering (6500→<200 modules) while preserving critical filesystems (Ext4, FAT, VFAT), USB HID (keyboards/mice), NVMe, AHCI/SATA, and EFI support. This guarantees both maximum performance **and** bootability.

#### Step 7: Set Sched_ext Scheduler to bpfland Auto (MANDATORY)

Find the **Sched_ext Scheduler** option and configure it:

```
Scheduler: [bpfland]
Mode: [auto]
```

**Why**: bpfland is the best-balanced SCX scheduler, providing:
- Excellent P99 latency (microsecond-class response times)
- Good throughput scaling
- Automatic power-aware mode switching
- Optimal for gaming and desktop workloads

#### Step 8: Verify Hardening Level and Review All Settings

Select your **Hardening Level**:
- **Gaming (Minimal)**: Maximum performance, lighter security baseline
- **Standard**: Balanced security + performance (recommended for most)
- **Hardened**: Maximum security (slower, for high-threat environments)

**Final verification checklist before clicking Build:**

```
Profile:               Gaming Profile ✓
LTO Strategy:          Full LTO ✓
Polly:                 Enabled ✓
Native CPU Targeting:  Enabled ✓
MGLRU:                 Enabled ✓
Module Filtering:      Whitelist Strategy ✓
Scheduler:             bpfland SCX (auto) ✓
Hardening:             Gaming or Standard ✓
```

If all items are checked, proceed to Step 9.

#### Step 9: Build the Kernel (Click Build)

Once all settings match the checklist above, click the **Build** button and wait for completion.

**Build times with GOAT Recipe**:
- **With modprobed-db + Full LTO**: 30-45 minutes (typical desktop system)
- **With modprobed-db + Thin LTO**: 20-30 minutes (if you chose Thin instead)
- **Without modprobed-db**: 60-120+ minutes (this is why modprobed-db is critical)

**During the build**: The UI shows real-time compilation logs. You should see:
- Kernel source extraction
- Configuration application
- Compilation phase with progress indicators
- Linking phase (this is where Full LTO shows its power)
- Module compilation

The build will complete with a notification. Do not interrupt or close the application during this time.

#### Step 10: Install and Benchmark the New Kernel

**After build completes successfully:**

1. Click **Install Kernel** (registers it with bootloader)
2. Reboot your system: `reboot`
3. Boot into the new kernel (verify with `uname -r`)
4. Open GOATd Kernel and navigate to the **Performance** tab
5. Click **Run GOATd Full Benchmark** (60-second evaluation)
6. **Review your GOAT Score and metric breakdown**

**Expected GOAT Score with The GOAT Recipe**: 750-900 (excellent to expert-level performance)

### Expected Results

After applying the GOAT Recipe, expect:

| Metric | Improvement | Typical Range |
|--------|-------------|----------------|
| **Latency** | 15-30% reduction | P99 < 100µs |
| **Consistency** | 20-40% improvement | CV% < 12% |
| **Jitter** | 10-25% reduction | P99.9 variance < 50µs |
| **Throughput** | 5-15% improvement | Profile-dependent |
| **Thermal** | Slight improvement | Heat-dependent system |
| **SMI Resilience** | 10-20% improvement | Bpfland-dependent |
| **Overall GOAT Score** | 750-900 range | Excellent performance |

### Performance Tiers

After building with the GOAT Recipe:

- **750-800**: Excellent gaming/desktop performance
- **800-850**: Outstanding optimization; competitive-grade
- **850-900**: Expert-level tuning; significant advantages
- **900+**: Rare; requires perfect conditions and hardware

If you score below 750, check:
1. Is modprobed-db installed and capturing modules correctly?
2. Is bpfland SCX scheduler actually running? (`scxctl status`)
3. Are there background processes consuming CPU during benchmarks?
4. Is your system thermals stable (no throttling)?

---

## Dashboard Flow & First-Run Guide

### Overview

GOATd Kernel's dashboard is structured as a **5-step guided workflow**. Each tab represents a stage in preparing and building your custom kernel.

### Step 1: Health Check

**Location**: First tab in the dashboard

**Purpose**: Validate system prerequisites and report any missing components.

**What It Checks**:
- ✅ Rust installation and version
- ✅ LLVM/Clang version (minimum 16+)
- ✅ Base-devel package group
- ✅ Git installation
- ✅ modprobed-db installation status
- ✅ System kernel information
- ✅ Boot partition mount status

**What You'll See**:
- Green checkmarks (✓) for all components in place
- Red exclamation marks (❌) for missing or outdated components
- Suggested fix commands for any issues

**Action Items**:
- If all checks pass ✓, proceed to **Fix Environment**
- If any checks fail, note the suggested commands

**Example Output**:
```
✓ Rust 1.75.0 installed
✓ LLVM 17.0.0 installed
✓ base-devel installed
❌ modprobed-db not installed (can be installed from AUR)
```

### Step 2: Fix Environment

**Location**: Second tab in the dashboard

**Purpose**: Resolve missing dependencies and configure required tools.

**What It Fixes**:
- Missing Rust (installs if needed)
- Outdated LLVM (suggests upgrade)
- Missing base-devel packages
- GPG key setup for kernel signature verification
- Clang/LLVM toolchain configuration

**Two Options**:

#### Option A: Automatic Fix (Recommended)
Click **Auto-Fix** to automatically install missing packages (requires sudo password).

#### Option B: Manual Commands
Copy individual commands and run them manually for more control:
```bash
sudo pacman -S rust               # Install Rust if missing
sudo pacman -S llvm clang lld polly  # Install LLVM 16+
sudo pacman -S base-devel         # Install development tools
```

**Action Items**:
- Complete all suggested fixes
- If you see green checkmarks ✓ next to each item, proceed to **Manual AUR Steps**
- Test by running `clang --version` (should show 16 or higher)

### Step 3: Manual AUR Steps

**Location**: Third tab in the dashboard

**Purpose**: Guide installation of `modprobed-db` from the Arch User Repository (AUR).

**Why This Is Manual**: This step requires careful attention to ensure you understand the module filtering impact.

**Copy-Paste Commands**:

The dashboard provides ready-to-copy commands:

```bash
# Clone modprobed-db from AUR
git clone https://aur.archlinux.org/modprobed-db.git
cd modprobed-db

# Build and install
makepkg -si
```

**Step-by-Step**:

1. Copy the first command block (git clone + cd)
2. Paste in your terminal and press Enter
3. Copy the build command (makepkg -si)
4. Paste and press Enter
5. Answer any prompts (usually just press 'y' for yes)

**After Installation**:

Once `modprobed-db` is installed, initialize the module database:

```bash
modprobed-db store
```

This captures your currently-loaded hardware modules.

**Verification**:

Check installation:
```bash
modprobed-db --version
```

Should output a version number (e.g., `modprobed-db 2.43`).

**Action Items**:
- ✓ Complete `makepkg -si` successfully
- ✓ Run `modprobed-db store` to initialize database
- ✓ Verify with `modprobed-db --version`
- Proceed to **Baseline Audit** once complete

**Troubleshooting**: See [Troubleshooting](#troubleshooting) section below if AUR installation fails.

### Step 4: Baseline Audit

**Location**: Fourth tab in the dashboard

**Purpose**: Scan your hardware and establish a performance baseline for comparison.

**What It Scans**:
- CPU model, core count, and instruction sets (SSE, AVX, AVX2, AVX-512)
- GPU hardware (NVIDIA, AMD, Intel integrated)
- RAM capacity and type
- Storage devices (NVMe, SATA, etc.)
- Kernel scheduler (BORE vs. EEVDF detection)
- Sched_ext (SCX) scheduler availability and status
- Active LTO settings
- Thermal capabilities

**What It Produces**:
- Hardware summary report
- Current kernel specifications
- Available optimization options
- Baseline performance snapshot (vanilla kernel or previous custom kernel)

**Review the Audit**:
- Take note of your **CPU model** — this determines best optimization flags
- Check **Available Schedulers** — confirms which SCX schedulers your kernel supports
- Review **Baseline Performance** — this is your comparison point

**Action Items**:
- ✓ Wait for audit to complete (usually 30-60 seconds)
- ✓ Screenshot or note your hardware specs
- ✓ Note your baseline GOAT Score (if a baseline benchmark has been run)
- Proceed to **Kernel Selection & Build**

### Step 5: Kernel Selection & Building

**Location**: Fifth tab in the dashboard (labeled "Profiles" or similar)

**Purpose**: Choose your kernel profile and configure optimization settings.

**Available Profiles**:

1. **Gaming Profile** (RECOMMENDED for most users)
   - Focus: Low latency, responsiveness
   - Default LTO: Thin LTO
   - Timer Frequency: 1000Hz
   - Best for: Gaming, desktop use, competitive edge

2. **Workstation Profile**
   - Focus: Security + Performance balance
   - Default LTO: Thin LTO
   - Timer Frequency: 1000Hz
   - Best for: Development, professional work, security-conscious users

3. **Server Profile**
   - Focus: Maximum throughput
   - Default LTO: Full LTO
   - Timer Frequency: 100Hz
   - Best for: Server workloads, database operations

4. **Laptop Profile**
   - Focus: Battery efficiency
   - Default LTO: Thin LTO
   - Timer Frequency: 300Hz
   - Best for: Laptops, mobile workstations

**Configuration Steps**:

1. Select your profile (Gaming recommended for first build)
2. Configure optimizations:
   - **LTO Strategy**: Choose None, Thin, or Full
   - **Polly**: Enable/disable loop vectorization
   - **Native Targeting**: Enable `-march=native` for your exact CPU
   - **MGLRU**: Enable/disable multi-generation LRU
   - **Hardening Level**: Minimal, Standard, or Hardened
   - **Module Filtering**: Select modprobed-db strategy
3. Click **Build**

**Build Progress**:

- UI shows real-time build logs
- Progress bar indicates completion estimate
- Status updates every few seconds
- Typical build time: 20-40 minutes (with modprobed-db), 60-120 minutes (without)

**After Build Completes**:

- Installation prompt appears
- Click **Install** to register the kernel with bootloader
- Reboot to the new kernel
- Proceed to benchmarking

### Step 6: Benchmarking & Results

**Location**: Performance tab

**Purpose**: Measure your kernel's performance with the GOATd Full Benchmark.

**Running the Benchmark**:

1. Boot into your new kernel (verify via `uname -r`)
2. Open GOATd Kernel dashboard
3. Navigate to **Performance** tab
4. Click **Run GOATd Full Benchmark** (60 seconds)

**Benchmark Phases** (6 stages, 10 seconds each):

1. **Baseline Phase** — Idle performance measurement
2. **Computational Heat** — CPU stress, latency measurement
3. **Memory Saturation** — RAM-heavy workload, consistency measurement
4. **Scheduler Flood** — Heavy task switching, jitter measurement
5. **Gaming Simulator** — Interactive gaming scenario
6. **The Gauntlet** — Combined stress, comprehensive scoring

**Expected Output**:
- Individual metric scores (Latency, Consistency, Jitter, etc.)
- Weightings and final GOAT Score
- Comparison to baseline (if available)
- Performance tier classification

**Interpreting Results**:

- **750+**: Excellent — Keep this kernel, consider further tweaking
- **500-750**: Good — Solid improvement; consider different profile/LTO for further gains
- **250-500**: Standard — Room for improvement; review GUIDE.md for optimization strategies
- **Below 250**: Room to improve — Check troubleshooting section

**Next Steps**:

- If satisfied (GOAT Score 750+), set as default kernel
- If not satisfied, try a different profile/LTO combination and rebuild
- Compare multiple kernels side-by-side using the result comparison UI

---

## Troubleshooting

### Build Failures

#### Issue: "Clang/LLVM not found" or "Clang version too old"

**Error Message**:
```
error: found clang version 15.0, expected 16.0 or higher
```

**Solution**:

1. Check current version: `clang --version`
2. Install LLVM 16+:
   ```bash
   sudo pacman -S llvm clang lld polly
   ```
3. Verify update: `clang --version` (should show 16+)
4. Retry kernel build

**Prevention**: Keep LLVM updated with `sudo pacman -Syu` before building.

---

#### Issue: "modprobed-db not found"

**Error Message**:
```
warning: modprobed-db not installed, skipping module filtering
```

**Solution**:

1. Install from AUR (via Manual AUR Steps tab in dashboard):
   ```bash
   git clone https://aur.archlinux.org/modprobed-db.git
   cd modprobed-db
   makepkg -si
   ```

2. Initialize database:
   ```bash
   modprobed-db store
   ```

3. Retry kernel build

**Note**: Without modprobed-db, builds are 2-3x slower due to compiling all 6500+ kernel modules.

---

#### Issue: "Build takes excessively long (2+ hours)"

**Solution**:

1. Install `modprobed-db` (see above) — reduces modules from 6500+ to <200
2. Change LTO to **Thin LTO** instead of Full LTO (faster optimization)
3. Ensure no background heavy processes:
   ```bash
   top    # Monitor CPU/RAM usage
   ```
4. Close unnecessary applications (browsers, IDEs, media players)

**Expected Build Times**:
- With modprobed-db + Thin LTO: 20-30 minutes
- With modprobed-db + Full LTO: 30-45 minutes
- Without modprobed-db: 60-120+ minutes

---

#### Issue: "Out of disk space" during build

**Error Message**:
```
error: No space left on device
```

**Solution**:

1. Check available space:
   ```bash
   df -h
   ```

2. Clean build cache:
   ```bash
   rm -rf ~/.<project>/target  # Remove old build artifacts
   ```

3. Clear pacman cache (safe, can be rebuilt):
   ```bash
   sudo pacman -Sc
   ```

4. Ensure at least **10GB** available before rebuilding

---

### Performance Issues

#### Issue: GOAT Score is lower than expected (< 500)

**Causes**:
- Background processes consuming CPU
- Thermal throttling
- Suboptimal profile/LTO selection
- Hardware limitation

**Solution**:

1. **Close background processes**:
   ```bash
   killall -9 firefox chromium steam  # Close heavy apps
   ```

2. **Ensure thermal stability**:
   ```bash
   watch sensors  # Monitor CPU temperatures
   ```
   If temperature > 80°C, improve case airflow or cooling.

3. **Review Settings**: Re-run dashboard Baseline Audit
   - Verify MGLRU is enabled
   - Confirm bpfland SCX scheduler is active
   - Check Polly is enabled

4. **Wait for system stabilization**: Reboot and wait 5 minutes before benchmarking again

5. **Try The GOAT Recipe**: Follow the exact steps in [The GOAT Recipe](#the-goat-recipe-optimal-configuration) section

---

#### Issue: Thermal throttling detected during benchmark

**Symptoms**:
- Thermal metric shows red/warning
- GOAT Score drops mid-benchmark
- CPU temperatures exceed 90°C

**Solution**:

1. **Improve cooling**:
   - Ensure case has adequate airflow
   - Clean dust filters and heatsink
   - Verify fans are spinning (use `watch sensors`)

2. **Reduce background load**:
   ```bash
   sudo systemctl stop cups  # Stop printing service
   sudo systemctl stop tlp   # Stop power management if too aggressive
   ```

3. **Disable CPU frequency scaling** (temporary, for testing):
   ```bash
   echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
   ```

4. **Enable MGLRU**: Reduces memory pressure and thermal load
   - Verified in dashboard Baseline Audit
   - Typically reduces thermal by 5-10°C under load

5. **Use Laptop Profile with Voluntary Preemption**: Reduces thermal generation

6. **Verify with thermal sensor**:
   ```bash
   stress-ng --cpu 4 &
   watch sensors
   ```
   Safe temp range: <80°C sustained, <90°C peak.

---

#### Issue: Jitter spikes during gameplay or benchmarking

**Symptoms**:
- Frame drops or stuttering
- Jitter metric shows orange/yellow
- Inconsistent framerates

**Solution**:

1. **Verify bpfland SCX scheduler is running**:
   ```bash
   scxctl status
   ```
   
   If not running, activate:
   ```bash
   sudo systemctl start scx_loader
   ```

2. **Disable CPU frequency scaling**:
   ```bash
   echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
   ```

3. **Enable MGLRU** (reduces memory-related jitter):
   - Verify in dashboard: should show "MGLRU: enabled"

4. **Check for SMI events**:
   ```bash
   cat /sys/firmware/acpi/interrupts/sci | head -1  # SMI counter
   ```
   
   If growing rapidly, SMI is causing issues:
   - Disable CPU power states: `sudo systemctl stop intel-pstate` (Intel) or equivalent for AMD
   - Increase system idle time between tests

5. **Isolate benchmark cores** (advanced):
   - Use `taskset -c 0-3 <benchmark>` to pin to specific cores
   - Ensures consistent scheduling

---

### System Issues

#### Issue: Bootloader doesn't recognize new kernel

**Symptoms**:
- Boot menu missing the new kernel
- Fallback to old kernel on reboot
- ESP (EFI System Partition) not found

**Solution**:

1. **Verify ESP is mounted**:
   ```bash
   mount | grep /efi
   ```
   
   If no output, ESP is unmounted:
   ```bash
   sudo mount /efi
   ```

2. **Reinstall bootloader entries**:
   ```bash
   sudo bootctl install
   ```

3. **Refresh bootloader**:
   ```bash
   sudo bootctl update
   ```

4. **Verify entry created**:
   ```bash
   sudo bootctl list
   ```
   
   Should show your new kernel entry.

5. **If using GRUB**:
   ```bash
   sudo grub-mkconfig -o /boot/grub/grub.cfg
   ```

6. **Reboot and verify**:
   ```bash
   reboot
   uname -r  # Should show new kernel version
   ```

---

#### Issue: Kernel modules failing to load after install

**Error Message**:
```
ERROR: could not insert 'nvidia': Unknown symbol in module
```

**Causes**:
- Module incompatibility with kernel ABI
- Missing dependencies in whitelist strategy
- Firmware not found

**Solution**:

1. **Verify modprobed-db whitelist included the driver**:
   ```bash
   grep nvidia ~/.config/modprobed-db/modprobed.db
   ```

2. **Run Baseline Audit** in dashboard:
   - Reports module compatibility
   - Suggests which modules failed

3. **Rebuild kernel with Standard hardening** (more compatible):
   - Go to Step 5 (Kernel Selection)
   - Choose Standard Hardening instead of Minimal
   - Rebuild and test

4. **Manually add missing modules** (advanced):
   ```bash
   echo "nvidia" >> ~/.config/modprobed-db/modprobed.db
   modprobed-db rebuild
   ```

5. **If GPU-specific**: Reinstall GPU drivers:
   ```bash
   # NVIDIA
   sudo pacman -S nvidia nvidia-utils nvidia-lts
   
   # AMD
   sudo pacman -S amdgpu-pro-installer
   
   # Intel
   sudo pacman -S intel-media-driver
   ```

---

#### Issue: Frequent kernel panics or crashes after reboot

**Symptoms**:
- Random reboots or freezes
- Kernel panic messages in logs
- System unstable after 10-30 minutes

**Causes**:
- Incompatible optimizations for your hardware
- Bad `-march` targeting (Native optimization too aggressive)
- Memory corruption from LTO bugs

**Solution**:

1. **Boot previous kernel**:
   - Reboot and select old kernel from bootloader menu
   - Confirms hardware is stable with vanilla kernel

2. **Reduce optimization**:
   - Disable Native CPU targeting (`-march=native`)
   - Use Thin LTO instead of Full LTO
   - Disable Polly optimization

3. **Test stability**:
   ```bash
   stress-ng --cpu 8 --timeout 600  # Run 10-minute stress test
   ```
   
   If stable for 10+ minutes, optimization was too aggressive.

4. **Review kernel logs**:
   ```bash
   journalctl -b -1  # Previous boot logs
   dmesg             # Current boot logs
   ```
   
   Look for hardware errors or specific driver issues.

5. **Rebuild with Workstation Profile**:
   - More conservative optimizations
   - Higher compatibility
   - Still good performance

6. **Last Resort: Disable module filtering**:
   - Don't use modprobed-db whitelist
   - Use full 6500+ modules (slower build, maximum compatibility)

---

### Getting Help

If you encounter issues not covered above:

1. **Check Dashboard Health Check** — Reports system status and suggestions
2. **Review Baseline Audit** — Hardware compatibility information
3. **Run tests**:
   ```bash
   stress-ng --cpu 4 --timeout 60  # CPU stress test
   memtester 1G                    # RAM test (if installed)
   ```
4. **Check logs**:
   ```bash
   journalctl -n 50  # Last 50 journal entries
   dmesg | tail -50  # Last 50 kernel messages
   ```
5. **Report on GitHub Issues** — Include dashboard screenshots and log excerpts

---

## Advanced Topics

### Sched_ext SCX Scheduler Options

GOATd Kernel supports multiple BPF-based schedulers via the Sched_ext (SCX) interface. Available options:

#### **bpfland** (RECOMMENDED)

**Focus**: Balanced latency and throughput
**Best For**: Gaming, general desktop, workstations
**Features**:
- Automatic power-aware scheduling
- Low latency (P99 < 100µs typical)
- Good throughput scaling
- Thermal-aware load balancing

**Configuration**:
```
Scheduler: bpfland
Mode: auto (switches between power modes)
```

---

#### **lavd** (Low-Latency Virtual Deadline)

**Focus**: Ultra-low latency
**Best For**: Competitive gaming, audio production, real-time synthesis
**Features**:
- Virtual deadline scheduling algorithm
- Extremely low latency (P99 < 50µs possible)
- Lower throughput (trade-off)
- Best with CPU frequency scaling disabled

**Configuration**:
```
Scheduler: lavd
Mode: fixed (performance mode)
```

**Caution**: Reduced throughput; not suitable for server workloads.

---

#### **scx_simple** (Baseline)

**Focus**: Minimal, reference implementation
**Best For**: Testing, comparison baseline, minimal overhead
**Features**:
- Simplest SCX scheduler
- Low overhead (minimal CPU usage)
- Standard latency/throughput
- Good for debugging

**Configuration**:
```
Scheduler: scx_simple
Mode: auto
```

---

### GPU Kernel Module Selection

GOATd Kernel supports hardware-aware GPU driver filtering:

#### **NVIDIA**

**Options**:
- **Proprietary Driver** (`nvidia`): Best performance, proprietary code
- **Open-Source Driver** (`nouveau`): Performance impact, open standards

**Module Filtering**:
- If using proprietary NVIDIA, ensure `nvidia` in modprobed-db
- Typical size: 200-400 modules with GPU support

---

#### **AMD**

**Driver**: `amdgpu` (always open-source on Linux)

**Module Filtering**:
- Include `amdgpu` in modprobed-db
- AMD drivers integrate well with module filtering
- Good performance with <200 module reduction

---

#### **Intel Integrated Graphics**

**Driver**: `i915` (Intel Direct Rendering)

**Module Filtering**:
- Include `i915` in modprobed-db
- Very efficient, minimal module overhead
- Excellent results with aggressive filtering

---

### Custom Profile Creation

For advanced users wanting full control:

1. **Clone existing profile** in GOATd Kernel dashboard
2. **Modify parameters**:
   - Timer frequency (100Hz, 300Hz, 1000Hz)
   - Preemption mode (none, voluntary, full)
   - Scheduler selection (BORE, EEVDF, SCX)
   - LTO strategy
3. **Save custom profile** with descriptive name
4. **Build from custom profile**
5. **Benchmark and iterate** until optimal

**Common Custom Profiles**:

- **Streaming Profile**: Server LTO + Gaming latency tuning
- **Audio Profile**: Gaming latency + Laptop efficiency
- **Server Gaming**: Server throughput + SCX low-latency scheduler

---

### Performance Tuning Reference

#### Tuning Levers by Metric

| Metric | Best Lever | Setting |
|--------|-----------|---------|
| **Latency** | Scheduler | bpfland or lavd SCX |
| **Latency** | Timer Frequency | 1000Hz |
| **Consistency** | MGLRU | Enabled |
| **Consistency** | Preemption | Full preemption |
| **Jitter** | SMI Mitigation | bpfland + freq scaling disabled |
| **Throughput** | LTO | Full LTO |
| **Throughput** | Polly | Enabled |
| **Efficiency** | Scheduler | Power-efficient SCX mode |
| **Efficiency** | Timer Freq | 300Hz |
| **Thermal** | MGLRU | Enabled |
| **Thermal** | Preemption | Voluntary |

---

### Benchmarking Best Practices

1. **Stabilize the system** before benchmarking:
   ```bash
   reboot
   sleep 300  # Wait 5 minutes after boot
   ```

2. **Close unnecessary applications** (browsers, IDEs, games)

3. **Enable performance mode**:
   ```bash
   echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
   ```

4. **Run benchmark 2-3 times** and average results:
   - First run may show outliers (system warm-up)
   - Second/third runs are more stable

5. **Compare same-day results**:
   - Temperature and background load affect scores
   - Compare kernels built same day, tested same environment

6. **Document your conditions**:
   - Room temperature
   - Open applications
   - CPU governor setting
   - Time since boot

---

### Contact & Support

For issues or questions:

1. **Check this GUIDE.md** — Most questions answered here
2. **Review README.md** — Feature overview and links
3. **Read [BLUEPRINT_V2.md](BLUEPRINT_V2.md)** — Technical specifications
4. **Consult [DEVELOPER_GUIDE.md](docs/DEVELOPER_GUIDE.md)** — Advanced topics
5. **GitHub Issues** — Report bugs or request features

---

**Last Updated**: January 2025  
**GOATd Kernel Version**: Phase 42+ Stable
