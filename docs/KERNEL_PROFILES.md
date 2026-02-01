# Kernel Profiles

The GOATd Kernel features a sophisticated 4-profile system that tailors the kernel's behavior and performance characteristics to specific use cases. These profiles are defined in [`src/config/profiles.rs`](src/config/profiles.rs) and applied during the configuration finalization phase.

## The 4-Profile System

| Profile | Focus | Key Characteristics | Recommended For |
| :--- | :--- | :--- | :--- |
| **Gaming** | Low Latency | 1000Hz, Full Preemption, Polly Optimizations, Thin LTO | Competitive gaming, real-time apps |
| **Server** | Throughput | 100Hz, Server Preemption, Full LTO, MGLRU | Hosting, databases, compile servers |
| **Workstation** | Stability & Security | 1000Hz, Full Preemption, Hardened Security, Thin LTO | Daily driving, development |
| **Laptop (Power)** | Efficiency | 300Hz, Voluntary Preemption, Power-aware scheduling | Max battery life, mobile work |

*Note: A "Generic" profile also exists as a balanced fallback.*

## Profile Influence on Configuration

Profiles in the GOATd Kernel go beyond simple presets; they influence the entire kernel configuration pipeline:

### 1. Kernel Configuration (Kconfig)
Profiles dictate critical Kconfig values such as:
- **`CONFIG_HZ`**: Timer frequency (100Hz to 1000Hz).
- **`CONFIG_PREEMPT`**: Preemption model (Full, Voluntary, or Server).
- **Security Hardening**: Toggling advanced security features based on the `HardeningLevel`.
- **Optimization Flags**: Enabling `Clang`, `Polly`, and `Thin/Full LTO`.

### 2. Rebranding and Versioning
Profiles are integrated into the kernel's identity:
- **`LOCALVERSION`**: The profile name is appended to the kernel version (e.g., `-goatd-gaming`).
- **MPL Integration**: The active profile is recorded in the Metadata Persistence Layer ([`docs/MPL_ARCHITECTURE.md`](docs/MPL_ARCHITECTURE.md)), ensuring the build system and UI are aware of the kernel's personality.

### 3. Build Toolchain
Profiles determine which compilers and optimization passes are used. For example, the **Gaming** profile leverages Polly (LLVM's polyhedral loop optimizer) for maximum performance, while the **Server** profile prioritizes **Full LTO** for binary-wide optimization.

## Implementation Details

The profile system is implemented as a data-driven hierarchy:
1.  **Definitions:** Located in [`src/config/profiles.rs`](src/config/profiles.rs) as a static map of `ProfileDefinition`.
2.  **Application:** Centralized in [`src/config/finalizer.rs`](src/config/finalizer.rs), where profile defaults are merged with hardware-specific optimizations and user overrides.
3.  **Propagation:** The selected profile name is propagated through the `OrchestrationState` to the patcher and finally into the build metadata.
