<img src="docs/Img/GOATd-Kernel.jpg" width="100%" alt="GOATd Kernel Banner">

<p align="center">
  <img src="https://img.shields.io/badge/Version-v0.2.1-blue?style=flat-square" alt="Version">
  <img src="https://img.shields.io/badge/Language-Rust-orange?style=flat-square" alt="Language">
  <img src="https://img.shields.io/badge/Platform-Arch_Linux-1793d1?style=flat-square" alt="Platform">
  <img src="https://img.shields.io/badge/License-GPL--3.0-green?style=flat-square" alt="License">
</p>

# GOATd Kernel

---

> **The Precision Orchestrator for Linux Kernel Sovereignty.**
> Built for power users who demand total control over their kernel lifecycle, transforming complex optimization into a streamlined, high-performance workflow grounded in mechanism, not marketing.

---

## The Reality of Kernel Optimization

Compiling a custom kernel isn't about chasing a 10% higher FPS or Geekbench score—those are userspace-heavy benchmarks that rarely touch the kernel. **GOATd Kernel is about minimizing system jitter, slashing the attack surface, and ensuring your hardware follows your specific intent, not a generic compatibility profile.**

---

## Determinism & Responsiveness: Why Custom Kernels Matter

**Mechanism Over Marketing**

Stock kernels prioritize universal compatibility at the cost of system determinism. GOATd Kernel replaces reactive algorithms with proactive mechanisms:

### MGLRU: Intelligent Memory Eviction
- **Traditional Page-Frame Reclaim Algorithm**: Evicts pages reactively, causing UI stutters and system hitching when RAM usage spikes.
- **MGLRU (Multi-Gen LRU)**: Makes smarter decisions about memory eviction by tracking multiple generations of page accesses, preventing "stuttering" during heavy I/O or multitasking.

### Native Optimizations (-march=native)
- Stock kernels compile for generic x86_64 to maximize compatibility.
- **Your kernel compiles specifically for your silicon**, enabling AVX2, AVX-512, BMI2, and other instruction sets that generic builds ignore.
- **Result**: Kernel processes data more efficiently for cryptographic operations, filesystem calculations, and syscall handling.

**Success Metric**: Lower 1% Lows—fewer stutters during heavy I/O or multitasking, not higher peak FPS.

---

## Security as a Foundation: The Real Cost

**Hardening isn't free—but it's honest about the tradeoff.**

### Control Flow Integrity (CFI) via Link-Time Optimization

**The Threat**: Return-Oriented Programming (ROP) attacks hijack control flow by chaining existing code fragments to execute unintended sequences.

**The Defense**: Full LTO allows the compiler to build a complete control-flow graph across all kernel source files:
- LTO inlines functions and prunes dead code, preventing gadget chains from chaining.
- CFI verifies every indirect function call against the compiler's control-flow graph, blocking invalid jumps.

### Attack Surface Reduction via Driver Exclusion

**The Scale Problem**: A stock kernel includes thousands of drivers for hardware you don't own.

**The Principle**: "If a driver isn't compiled into your kernel, it cannot be exploited."

**GOATd's Modprobed & Desktop Whitelist**:
- Scans your actual hardware—excludes drivers you don't need.
- Whitelist ensures essential desktop functions (USB storage, common filesystems) remain functional.
- **Result**: Drastically smaller kernel size, faster boot times, and measurably reduced exploitable surface.

### The Performance Tax: Honest Numbers

Hardened kernels enable every protection (KPTI, Retpolines, CFI):
- **Measured performance impact: 5-15% on syscall-heavy workloads** (measured on context-switch latency, syscall duration).
- This is the real cost of modern mitigations, not marketing spin.
- For gaming/interactive use on modern CPUs, the impact is much lower due to branch prediction optimization.

**Success Metric**: Security baseline achieved—running a kernel with a significantly smaller attack surface and optimized (not just "bolted-on") hardware mitigations.

---

## Intelligent Scheduling: The SCX Framework

**GOATd allows you to swap the "brain" of your system at runtime using eBPF-based schedulers.**

### Scheduler Selection

| Scheduler | Logic | Primary Use |
| :--- | :--- | :--- |
| **EEVDF** | Fairness & Lag | General Purpose: Standard Linux baseline for stable, predictable fairness. |
| **scx_bpfland** | Interactivity | Everyday Hero: Prioritizes your foreground app, ensuring game/stream/Discord stays responsive. |
| **scx_lavd** | Latency Criticality | Pro-Audio & Gaming: Eliminates buffer underruns by identifying critical "wait chains" between tasks. |
| **scx_rusty** | Cache Locality | Server & Throughput: Partitions by LLC domains on high-core systems for maximum parallelism. |

### Profiling Flags: Runtime Tuning

- **🤖 Auto (Default)**: Dynamic heuristic. Disables aggressive features during idleness, scales up during high-interactivity demands. Best balanced experience.
- **🚀 Gaming**: Locks scheduler to performance-first state. Maximizes **1% Lows** by prioritizing P-cores, increasing context-switch frequency to reduce input lag, preventing background task interference.
- **⚡ LowLatency**: Real-time responsiveness. Reduces time-to-CPU for high-priority threads, ideal for DAW and real-time communication where hitching is unacceptable.
- **🔋 PowerSave**: CPU deep sleep priority. Aggressively parks idle cores, intentionally delays non-critical tasks to enable hardware low-power states.
- **🖥️ Server**: Sustained high-throughput. Pins tasks to LLC domains, reduces inter-core task migration, maximizes cache locality for database engines and web servers.

**Success Metric**: Deterministic responsiveness. Your system responds the same way every time, regardless of background load.

---

## Performance Realities: Myth-Busting

**Expectation**: Focus on minimized 1% lows and higher minimum FPS while maintaining a MUCH higher security level.
**Reality**: A system that *feels* as fast or faster than an "unprotected" kernel.

**Why**: You're running hardened, optimized code. The security tax is real but measured. Background jitter is eliminated. Memory management is smarter. Scheduling is tailored to your workload.

### The Realistic Outcome

When you build a GOATd Kernel, you get:

1. **Lower 1% Lows**: Fewer stutters during heavy I/O or multitasking.
2. **Determinism**: Your system responds consistently, regardless of background load.
3. **Security Baseline**: Modern hardware mitigations are optimized into the kernel, not just forced on top.
4. **Attack Surface Reduction**: Only drivers you actually use are compiled in.

---

### Core Features

* 🛠️ **Orchestrator Build Pipeline**: State-aware engine for automated dependency resolution and microarchitecture detection.
* 📊 **Spectrum UI Telemetry**: Real-time monitoring of context switches, jitter, and syscall latency via native `egui`.
* ⚙️ **LLVM-First Toolchain**: Advanced LTO for maximum binary efficiency.
* 🛡️ **Integrity Audit**: Automated verification of source signatures and build environment health.

---

### Variant Matrix

| Variant | Focus | Description |
| :--- | :--- | :--- |
| **Mainline** | Edge | Latest upstream RC code with full pipeline support. |
| **Stable** | Balanced | Upstream standard with GOATd optimization passes. |
| **Hardened** | Security | Maximum attack surface reduction and hardening. |
| **LTS** | Reliability | Stability-first approach for mission-critical systems. |

---

### Quick Start


**For Arch Linux Users - AUR Installation (Fastest)**

The latest release is now available on the AUR: ```goatdkernel```. Use the following to quickly download and install the pre-built binary without having to compile:
```bash
yay -S goatdkernel
```
or
```bash
paru -S goatdkernel
```

---

**1. Prerequisites**
Ensure you are on Arch Linux with base development tools:
```bash
sudo pacman -Syu --needed base-devel rustup git
rustup default stable
```

**2. Installation**
Clone and initialize the environment:
```bash
git clone https://github.com/madgoat/goatd-kernel.git
cd goatd-kernel
./goatdkernel.sh --setup
```

**3. Launch**
Run the orchestrator:
```bash
cargo run --release
```

---

### Architecture & Documentation

Explore the internal logic and design principles:
* [MPL Architecture](docs/MPL_ARCHITECTURE.md)
* [Kernel Profiles](docs/KERNEL_PROFILES.md)
* [Performance Metrics Spec](docs/PERFORMANCE_METRICS_SPEC.md)
* [User Guide](docs/USER_GUIDE.md)
* [Feature Breakdown](docs/Feature%20Breakdown.md)

---

*GOATd Kernel: Built for those who know the difference.*
