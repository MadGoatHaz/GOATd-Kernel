[madgoat@madgoat GOATd Kernel]$ ./goatdkernel.sh
✓ Cargo available

 ██████╗  ██████╗  █████╗ ████████╗     ██╗
███╔═══╝ ██╔═══██╗██╔══██╗╚══██╔══╝ ██████║
██║  ███╗██║   ██║███████║   ██║   ██╔══██║
██║   ██║██║   ██║██╔══██║   ██║   ██║  ██║
╚██████╔╝╚██████╔╝██║  ██║   ██║   ╚█████╔╝

Starting GOATd Kernel...
Building with Cargo...
✓ Cargo available
Setting up project directories...
✓ All directories ready

Running: cargo build --release
    Finished `release` profile [optimized] target(s) in 0.37s
warning: the following packages contain code that will be rejected by a future version of Rust: ashpd v0.8.1
note: to see what the problems were, use the option `--future-incompat-report`, or run `cargo report future-incompatibilities --id 1`
✓ Build completed successfully
Binary location: /home/madgoat/Documents/GOATd Kernel/target/release/goatd_kernel
[LAUNCH DEBUG] Executing: /home/madgoat/Documents/GOATd Kernel/target/release/goatd_kernel
[LAUNCH DEBUG] Binary size: 30994720 bytes
[LAUNCH DEBUG] Binary timestamp: 2026-01-29 16:32:25.618708132 -0700
[LAUNCH DEBUG] File type: ELF 64-bit LSB pie executable, x86-64, version 1 (SYSV), dynamically linked, interpreter /lib64/ld-linux-x86-64.so.2, for GNU/Linux 4.4.0, BuildID[sha1]=14030eabe642e5101a33abeecbc0845484fe3354, not stripped
[LAUNCH DEBUG] Working dir: /home/madgoat/Documents/GOATd Kernel
[LAUNCH DEBUG] Logs location: /home/madgoat/Documents/GOATd Kernel/logs

[System] initialize_logging() called (delegated to LogCollector in main.rs)
[Main] ✓ Unified logging system initialized via system::initialize_logging()
[Main] ✓ Diagnostic buffer initialized (capacity=4096, non-blocking)
[Log] [DISPATCHER] Registered LogCollector 1 in global dispatcher
[Main] ✓ LogCollector initialized (decoupled, non-blocking)
[Log] [INIT] Creating full log file immediately: /mnt/Optane/Documents/GOATd Kernel/logs/full/build_20260129_163400.164.log
[Main] ✓ LogCollector registered as global logger (all log::* macros piped to disk)
[Log] [INIT] Creating parsed log file immediately: /mnt/Optane/Documents/GOATd Kernel/logs/parsed/20260120_201551_parsed.log
[Main] Application initialized and running
[Main] Launching egui frontend...
[Main] [HW] Starting background hardware detection
[Main] [HW] ✓ Detected: CPU=AMD Ryzen 9 5950X 16-Core Processor, RAM=62GB, GPU=Nvidia
[Main] [HW] Done: Hardware info cached in controller
[Main] [HW] Hardware detection async task completed (repaint signals deferred to app.rs update loop)
[UI] [KERNELS] Loaded 2 installed kernels
[UI] [KERNELS] Loaded/refreshed 1 built artifacts from /mnt/Optane/goatd
[UI] [SCX] Scheduler list initialized with EEVDF (Stock) prepended: ["EEVDF (Stock)", "scx_bpfland", "scx_lavd", "scx_rusty"]
[UI] [KERNELS] Selected kernel: linux-goatd-gaming (6.18.7.arch1-1) (index 1)
[UI] [KERNELS] ✓ Versioned audit completed for: 6.18.7.arch1-1
[UI] [KERNELS] ✓ Kernel uninstall succeeded: linux-goatd-gaming (6.18.7.arch1-1)
[UI] [KERNELS] Loaded 1 installed kernels
[UI] [KERNELS] Loaded/refreshed 1 built artifacts from /mnt/Optane/goatd
[UI] [KERNELS] Selected artifact: linux-goatd-gaming (6.18.7.arch1-1) (index 0)
[KERNEL] [WORKSPACE] Resolved workspace to absolute path: /mnt/Optane/goatd
[UI] [KERNELS] Installation task spawned for artifact: linux-goatd-gaming (6.18.7.arch1-1)
[KERNEL] [PATH_CANON] Entry path canonicalized to absolute: /mnt/Optane/goatd/linux/linux-goatd-gaming-6.18.7.arch1-1-x86_64.pkg.tar.zst
[KERNEL] ✓ Resolved kernel version: 6.18.7-arch1-1-goatd-gaming
[KERNEL] [REGISTRY] Initializing KernelArtifactRegistry with heuristic discovery
[KERNEL] [REGISTRY] ✓ Registry initialized: [linux-goatd-gaming-6.18.7.arch1-1-x86_64.pkg.tar.zst, linux-goatd-gaming-headers-6.18.7.arch1-1-x86_64.pkg.tar.zst, linux-goatd-gaming-docs-6.18.7.arch1-1-x86_64.pkg.tar.zst]
[KERNEL] [REGISTRY] Kernel path: /mnt/Optane/goatd/linux/linux-goatd-gaming-6.18.7.arch1-1-x86_64.pkg.tar.zst
[KERNEL] [REGISTRY] Headers path: /mnt/Optane/goatd/linux/linux-goatd-gaming-headers-6.18.7.arch1-1-x86_64.pkg.tar.zst
[KERNEL] [REGISTRY] Docs path: /mnt/Optane/goatd/linux/linux-goatd-gaming-docs-6.18.7.arch1-1-x86_64.pkg.tar.zst
[KERNEL] [REGISTRY] Collected 3 artifact(s) for installation
[KERNEL] DKMS will be batched with kernel installation for unified privilege session
[KERNEL] [UNIFIED] Step 1: Setting up DKMS safety net in unified batch
[KERNEL] [UNIFIED] Step 2: Adding pre-install cleanup to unified batch
[KERNEL] [UNIFIED] Cleanup command: rm -rf /usr/lib/modules/6.18.7-arch1-1-goatd-gaming/build /usr/lib/modules/6.18.7-arch1-1-goatd-gaming/source
[KERNEL] [UNIFIED] Step 3: Adding pacman install to unified batch
[KERNEL] [UNIFIED] Bundled pacman command for 3 artifact(s)
[KERNEL] [UNIFIED]   Artifacts: [linux-goatd-gaming-6.18.7.arch1-1-x86_64.pkg.tar.zst, linux-goatd-gaming-headers-6.18.7.arch1-1-x86_64.pkg.tar.zst, linux-goatd-gaming-docs-6.18.7.arch1-1-x86_64.pkg.tar.zst]
[KERNEL] [UNIFIED]   Resolved paths:
[KERNEL] [UNIFIED]     [1] '/mnt/Optane/goatd/linux/linux-goatd-gaming-6.18.7.arch1-1-x86_64.pkg.tar.zst'
[KERNEL] [UNIFIED]     [2] '/mnt/Optane/goatd/linux/linux-goatd-gaming-headers-6.18.7.arch1-1-x86_64.pkg.tar.zst'
[KERNEL] [UNIFIED]     [3] '/mnt/Optane/goatd/linux/linux-goatd-gaming-docs-6.18.7.arch1-1-x86_64.pkg.tar.zst'
[KERNEL] [UNIFIED] ═════════════════════════════════════════════════════
[KERNEL] [UNIFIED] BATCH COMMAND WITH VERIFICATION GUARD:
[KERNEL] [UNIFIED] (command length: 611 bytes, showing structure)
[KERNEL] [UNIFIED] Structure: 3 ls checks && pacman install
[KERNEL] [UNIFIED] ═════════════════════════════════════════════════════
[KERNEL] [UNIFIED] Step 3: Adding HARDENED symlink creation with validated paths (registry-injected, soft failure)
[KERNEL] [UNIFIED] Injecting validated headers path: /mnt/Optane/goatd/linux/linux-goatd-gaming-headers-6.18.7.arch1-1-x86_64.pkg.tar.zst
[KERNEL] [UNIFIED] Step 4: Adding DKMS autoinstall to unified batch (soft failure)
[KERNEL] [UNIFIED] ═════════════════════════════════════════════════════
[KERNEL] [UNIFIED] Executing unified batch (7 steps in SINGLE privileged session)
[KERNEL] [UNIFIED] ═════════════════════════════════════════════════════
[KERNEL] [UNIFIED] INTERPOLATED COMMAND CHAIN:
[KERNEL] [UNIFIED]   Step 1: mkdir -p /etc/dkms/framework.conf.d
[KERNEL] [UNIFIED]   Step 2: printf '...' > /etc/dkms/framework.conf.d/goatd.conf
[KERNEL] [UNIFIED]   Step 3: chmod 644 /etc/dkms/framework.conf.d/goatd.conf
[KERNEL] [UNIFIED]   Step 4: rm -rf /usr/lib/modules/6.18.7-arch1-1-goatd-gaming/build /usr/lib/modules/6.18.7-arch1-1-goatd-gaming/source
[KERNEL] [UNIFIED]   Step 5: pacman -U --noconfirm --overwrite 'usr/lib/modules/*/build,usr/lib/modules/*/source' [linux-goatd-gaming-6.18.7.arch1-1-x86_64.pkg.tar.zst, linux-goatd-gaming-headers-6.18.7.arch1-1-x86_64.pkg.tar.zst, linux-goatd-gaming-docs-6.18.7.arch1-1-x86_64.pkg.tar.zst]
[KERNEL] [UNIFIED]   Step 6: DYNAMIC symlink creation with intelligent header discovery (soft failure)
[KERNEL] [UNIFIED]     - Search: /usr/src/linux-*-goatd* (GOATd-specific headers)
[KERNEL] [UNIFIED]     - Fallback: /usr/src/linux-* (any available headers)
[KERNEL] [UNIFIED]     - Create: ln -sf $hdr_dir /usr/lib/modules/6.18.7-arch1-1-goatd-gaming/build
[KERNEL] [UNIFIED]     - Create: ln -sf $hdr_dir /usr/lib/modules/6.18.7-arch1-1-goatd-gaming/source
[KERNEL] [UNIFIED]   Step 7: dkms autoinstall -k 6.18.7-arch1-1-goatd-gaming (soft failure)
[KERNEL] [UNIFIED] ═════════════════════════════════════════════════════
[KERNEL] [UNIFIED] SENTINEL DETECTION ENABLED:
[KERNEL] [UNIFIED] - If symlinks fail, sentinel 'DKMS_FAILED_SYMLINKS' will be echoed to stdout
[KERNEL] [UNIFIED] - If DKMS fails, sentinel 'DKMS_FAILED_MODULES' will be echoed to stdout
[KERNEL] [UNIFIED] - Detection will report 'Partial Success' if sentinels appear in output
[KERNEL] [UNIFIED] ═════════════════════════════════════════════════════
[KERNEL] [UNIFIED] Invoking system.batch_privileged_commands() for unified execution...
[KERNEL] [UNIFIED BATCH] Batch execution completed - scanning for soft failure sentinels...
[KERNEL] [UNIFIED BATCH] Captured stdout for sentinel scanning: 2146 bytes
[KERNEL] [UNIFIED BATCH] ⚠ SOFT FAILURE DETECTED: Non-critical step failed
[KERNEL] [UNIFIED BATCH]   - Sentinel found: DKMS_FAILED_SYMLINKS
[KERNEL] [UNIFIED BATCH] ═══════════════════════════════════════════════════
[KERNEL] [UNIFIED BATCH] ✓ UNIFIED BATCH COMPLETED
[KERNEL] [UNIFIED BATCH] ═══════════════════════════════════════════════════
[KERNEL] [UNIFIED BATCH] ✓ Step 1: DKMS safety net configuration: SUCCESS
[KERNEL] [UNIFIED BATCH] ✓ Step 2: Pacman kernel+headers+docs installation: SUCCESS
[KERNEL] [UNIFIED BATCH]   Artifacts: [linux-goatd-gaming-6.18.7.arch1-1-x86_64.pkg.tar.zst, linux-goatd-gaming-headers-6.18.7.arch1-1-x86_64.pkg.tar.zst, linux-goatd-gaming-docs-6.18.7.arch1-1-x86_64.pkg.tar.zst]
[KERNEL] [UNIFIED BATCH] Step 3: Fallback symlink creation: ⚠ FAILED (soft) (soft failure enabled)
[KERNEL] [UNIFIED BATCH] Step 4: DKMS autoinstall for kernel 6.18.7-arch1-1-goatd-gaming: ⚠ FAILED/INCOMPATIBLE (soft) (soft failure enabled)
[KERNEL] [UNIFIED BATCH] ═══════════════════════════════════════════════════
[KERNEL] [UNIFIED BATCH] ⚠⚠⚠ PARTIAL SUCCESS: Kernel installed, but DKMS/drivers incompatible
[KERNEL] [UNIFIED BATCH] ═══════════════════════════════════════════════════
[KERNEL] [UNIFIED BATCH] INSTALLATION STATUS: Partial Success
[KERNEL] [UNIFIED BATCH]   ✓ Kernel installed: 6.18.7-arch1-1-goatd-gaming
[KERNEL] [UNIFIED BATCH]   ✓ Headers and docs installed
[KERNEL] [UNIFIED BATCH]   ⚠ DKMS/GPU drivers: Incompatible or failed
[KERNEL] [UNIFIED BATCH] ═══════════════════════════════════════════════════
[KERNEL] [UNIFIED BATCH] GUIDANCE FOR PARTIAL SUCCESS:
[KERNEL] [UNIFIED BATCH]   1. Kernel is ready to boot: reboot now to test
[KERNEL] [UNIFIED BATCH]   2. GPU driver installation failed (DKMS incompatibility)
[KERNEL] [UNIFIED BATCH]      This commonly occurs on RC kernels or with unsupported GPUs
[KERNEL] [UNIFIED BATCH]   3. After reboot, you can manually install drivers:
[KERNEL] [UNIFIED BATCH]      $ sudo dkms autoinstall -k 6.18.7-arch1-1-goatd-gaming
[KERNEL] [UNIFIED BATCH]      OR install NVIDIA/AMD drivers from your package manager
[KERNEL] [UNIFIED BATCH]   4. If drivers remain unavailable:
[KERNEL] [UNIFIED BATCH]      - Check GPU compatibility with this kernel version
[KERNEL] [UNIFIED BATCH]      - RC kernels may require driver patches from upstream
[KERNEL] [UNIFIED BATCH] ═══════════════════════════════════════════════════
[KERNEL] [UNIFIED BATCH] ✓ Emitted InstallationComplete(true) for Partial Success path
[KERNEL] [VERIFY] Waiting 2 seconds for kernel-install hooks and DKMS to complete...
[KERNEL] [VERIFY] Starting post-install verification for: 6.18.7-arch1-1-goatd-gaming
[KERNEL] [VERIFY] ✓ Headers discovered via heuristic search: /usr/src/linux-goatd-goatd-mainline-gaming-gaming
[VERIFY] ===== STARTING COMPREHENSIVE KERNEL VERIFICATION =====
[VERIFY] Kernel version: 6.18.7-arch1-1-goatd-gaming
[VERIFY] Checking kernel module directory for: 6.18.7-arch1-1-goatd-gaming
[VERIFY] ✓ Module directory exists: /usr/lib/modules/6.18.7-arch1-1-goatd-gaming
[VERIFY] ✓ modules.dep found (valid kernel install)
[VERIFY] Module directory: ✓
[VERIFY] Checking build symlink for: 6.18.7-arch1-1-goatd-gaming
[VERIFY] ✓ Build symlink exists: /usr/lib/modules/6.18.7-arch1-1-goatd-gaming/build
[VERIFY] ✓ Build symlink points to: /usr/lib/modules/6.18.7-arch1-1-goatd-gaming/build
[VERIFY] ✓ Makefile found in build target
[VERIFY] Build symlink: ✓
[VERIFY] Checking source symlink for: 6.18.7-arch1-1-goatd-gaming
[VERIFY] ✗ Source symlink not found
[VERIFY] Source symlink: ✗
[VERIFY] Checking kernel headers installation for: 6.18.7-arch1-1-goatd-gaming
[DISCOVER_HEADERS] [UNIFIED-NAMING] Searching for headers for kernel version: 6.18.7-arch1-1-goatd-gaming
[DISCOVER_HEADERS] [UNIFIED-NAMING] Base version extracted: 6.18.7-arch1-1-goatd
[DISCOVER_HEADERS] [STRATEGY-1] Trying exact match: /usr/src/linux-6.18.7-arch1-1-goatd-gaming
[DISCOVER_HEADERS] [STRATEGY-2] Trying base version: /usr/src/linux-6.18.7-arch1-1-goatd
[DISCOVER_HEADERS] [STRATEGY-3] Scanning /usr/src for linux-* directories with STRICT .kernelrelease validation
[DISCOVER_HEADERS] [STRATEGY-4] [BRANDING-FALLBACK] Scanning for GOATd-branded directories (last resort)
[DISCOVER_HEADERS] ✗ No kernel headers found for version: 6.18.7-arch1-1-goatd-gaming
[VERIFY] ✗ Kernel headers not found for version: 6.18.7-arch1-1-goatd-gaming
[VERIFY] Headers installed: ✗
[VERIFY] DKMS readiness: ✗ NOT READY
[VERIFY] ===== VERIFICATION COMPLETE =====
[KERNEL] [VERIFY] ⚠ Kernel installation incomplete (unexpected condition)
[KERNEL] [VERIFY] Note: Dynamic symlinks were created with intelligent header discovery
[KERNEL] [VERIFY] Dynamic headers discovered: /usr/src/linux-goatd-goatd-mainline-gaming-gaming
[KERNEL] [UNIFIED] Installation successful - signaling UI completion event
[UI] [KERNELS] Loaded 2 installed kernels
[UI] [KERNELS] Loaded/refreshed 1 built artifacts from /mnt/Optane/goatd

