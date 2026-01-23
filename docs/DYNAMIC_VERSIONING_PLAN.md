# Dynamic Versioning Strategy: Transitioning to "Latest" Kernel Versions

## 1. Core Concept
The goal is to move from pinned kernel versions (e.g., "6.12.1") to a dynamic "Always Latest" model. This ensures users always build the most recent stable or variant-specific kernel without manual version selection.

## 2. Model Changes ([`src/models.rs`](src/models.rs))

### `KernelConfig` Enhancement
We will use `"latest"` as a special sentinel value for the `version` field in [`KernelConfig`](src/models.rs:237).

```rust
impl KernelConfig {
    /// Returns true if the config is set to track the latest version
    pub fn is_dynamic_version(&self) -> bool {
        self.version == "latest"
    }
}
```

- **Default Value**: The `Default` implementation for [`KernelConfig`](src/models.rs:268) will be updated to use `"latest"` instead of `"6.6.0"`.
- **UI State**: When `"latest"` is selected, the UI will display a "Checking for latest..." status until resolved.

## 3. Orchestration Workflow ([`src/orchestrator/executor.rs`](src/orchestrator/executor.rs))

Version resolution should happen as early as possible in the build pipeline, specifically during the **Preparation** phase.

### Polling Implementation
The `prepare_build_environment` or a dedicated `resolve_dynamic_version` function in [`executor.rs`](src/orchestrator/executor.rs) will handle the translation of `"latest"` to a concrete version string.

```rust
pub async fn resolve_dynamic_version(config: &mut KernelConfig) -> Result<String, BuildError> {
    if !config.is_dynamic_version() {
        return Ok(config.version.clone());
    }

    log::info!("[ORCHESTRATOR] Resolving latest version for variant: {}", config.kernel_variant);
    
    match get_latest_version_by_variant(&config.kernel_variant).await {
        Ok(version) => {
            log::info!("[ORCHESTRATOR] Resolved 'latest' to: {}", version);
            config.version = version.clone();
            Ok(version)
        }
        Err(e) => {
            // Fallback strategy (see Section 5)
            Err(BuildError::PreparationFailed(format!("Failed to resolve latest version: {}", e)))
        }
    }
}
```

## 4. UI Feedback & Interaction ([`src/ui/kernels.rs`](src/ui/kernels.rs) & [`src/ui/controller.rs`](src/ui/controller.rs))

### Build Events
A new [`BuildEvent`](src/ui/controller.rs:67) variant or an update to `StatusUpdate` will communicate the resolution progress.

```rust
pub enum BuildEvent {
    // ... existing variants
    VersionResolved(String), // Concrete version resolved from 'latest'
}
```

### UI Indicators
- **Dashboard/Build Tab**: If `version == "latest"`, show a loading spinner or "⟳ Resolving version..." text.
- **Console Log**: Output `[ORCHESTRATOR] Resolving 'latest' variant 'linux-zen' -> 6.13.1-zen1-1`.

## 5. Offline Scenarios & Failure Handling

Dynamic polling depends on network availability. We must ensure the app remains functional when offline.

### Fallback Hierarchy
1. **Successful Poll**: Use the fetched version from PKGBUILD/Git.
2. **Cached Version**: If polling fails, check if we have a previously resolved version for this variant in [`SettingsManager`](src/config/loader.rs) or a dedicated cache file.
3. **Local PKGBUILD Parse**: Search the local workspace for an existing `PKGBUILD` and extract the version using regex (already implemented in [`src/kernel/pkgbuild.rs`](src/kernel/pkgbuild.rs)).
4. **Hardcoded Baseline**: If all else fails, use a safe baseline version defined in [`models.rs`](src/models.rs).

### Error Reporting
If no version can be resolved (offline + no cache + no local source), the build should fail gracefully with a clear error:
> "Unable to resolve 'latest' version. Please check your internet connection or specify a concrete version."

## 6. Implementation Checklist
- [ ] Add `is_dynamic_version` method to [`KernelConfig`](src/models.rs).
- [ ] Implement `resolve_dynamic_version` in [`executor.rs`](src/orchestrator/executor.rs).
- [ ] Update `AsyncOrchestrator::run` to call version resolution at the start of Phase 1.
- [ ] Add `VersionResolved` to `BuildEvent` in [`controller.rs`](src/ui/controller.rs).
- [ ] Update [`src/ui/build.rs`](src/ui/build.rs) to handle the resolution state.
- [ ] Implement local cache for resolved versions to support offline fallback.
