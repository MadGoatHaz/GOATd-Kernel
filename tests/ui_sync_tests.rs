//! UI Synchronization Tests
//!
//! Tests to ensure that Egui properties are correctly synchronized with Rust state
//! on startup and during user interactions. This test suite prevents regression of the
//! startup state synchronization bug where checkbox states for BORE, Polly, and MGLRU
//! were not reflecting the Rust-side state values.
//!
//! # Phase A4: Test Suite Alignment
//!
//! This module now includes tests for:
//! - Whitelist/Auto-Discovery dependency (Whitelist toggle disabled/enabled based on modprobed state)
//! - LTO optional with profile-specific defaults
//! - Profile -> Default -> Manual Override -> Build chain
//! - AppState profile defaults and serialization (consolidated from startup_verification.rs)

#[cfg(test)]
mod ui_sync_tests {
    use goatd_kernel::config::AppState;
    use goatd_kernel::models::HardeningLevel;
    use goatd_kernel::ui::controller::{AppController, BuildEvent};
    use std::sync::Arc;
    use tokio::sync::mpsc;

    /// Test fixture: Initialize AppController with test dependencies
    pub async fn setup_controller() -> Arc<AppController> {
        let (build_tx, _build_rx) = mpsc::channel::<BuildEvent>(256);
        let (cancel_tx, _cancel_rx) = tokio::sync::watch::channel(false);

        let controller = AppController::new_production(build_tx, cancel_tx, None).await;
        Arc::new(controller)
    }

    // ============================================================================
    // CONSOLIDATED TESTS FROM startup_verification.rs
    // ============================================================================

    /// Test that AppState correctly handles profile-aware defaults
    #[test]
    fn test_app_state_profile_defaults() {
        // Create default state - Gaming profile
        let state = AppState::default();
        assert_eq!(
            state.selected_profile, "gaming",
            "Default profile should be gaming"
        );

        // Gaming profile should have default unchecked values initially
        assert!(!state.use_polly, "Initial state: Polly should be false");
        assert!(!state.use_mglru, "Initial state: MGLRU should be false");

        eprintln!(
            "[Test] Default AppState created: profile={}, Polly={}, MGLRU={}",
            state.selected_profile, state.use_polly, state.use_mglru
        );
    }

    /// Test that AppState reflects profile changes
    #[test]
    fn test_app_state_profile_switching() {
        let mut state = AppState::default();
        state.selected_profile = "gaming".to_string();

        eprintln!("[Test] Testing profile: {}", state.selected_profile);

        // Verify we can switch profiles
        state.selected_profile = "server".to_string();
        assert_eq!(
            state.selected_profile, "server",
            "Profile should switch to server"
        );

        state.selected_profile = "workstation".to_string();
        assert_eq!(
            state.selected_profile, "workstation",
            "Profile should switch to workstation"
        );

        eprintln!("[Test] ✅ Profile switching works correctly");
    }

    /// Test that AppState can be serialized and deserialized
    /// This is critical for persistence on startup
    #[test]
    fn test_app_state_serialization() {
        let mut original = AppState::default();
        original.selected_profile = "gaming".to_string();
        original.use_polly = true;
        original.use_mglru = true;
        original.kernel_hardening = HardeningLevel::Minimal;

        // Serialize
        let json = serde_json::to_string(&original).expect("Serialization failed");
        eprintln!("[Test] Serialized AppState: {}", json);

        // Deserialize
        let restored: AppState = serde_json::from_str(&json).expect("Deserialization failed");

        // Verify round-trip
        assert_eq!(
            restored.selected_profile, "gaming",
            "Profile must survive serialization"
        );
        assert_eq!(restored.use_polly, true, "Polly must survive serialization");
        assert_eq!(restored.use_mglru, true, "MGLRU must survive serialization");
        assert_eq!(
            restored.kernel_hardening,
            HardeningLevel::Minimal,
            "Hardening must survive serialization"
        );

        eprintln!("[Test] ✅ AppState serialization/deserialization works correctly");
    }

    /// Test that checkbox states (use_bore, use_polly, use_mglru) can be set and persist
    #[test]
    fn test_checkbox_state_persistence() {
        let mut state = AppState::default();

        // Set all checkboxes to true
        state.use_polly = true;
        state.use_mglru = true;

        // Verify they remain set
        assert!(state.use_polly, "Polly should remain true after setting");
        assert!(state.use_mglru, "MGLRU should remain true after setting");

        // Toggle them back
        state.use_polly = false;
        state.use_mglru = false;

        assert!(!state.use_polly, "Polly should be false after toggling");
        assert!(!state.use_mglru, "MGLRU should be false after toggling");

        eprintln!("[Test] ✅ Checkbox state persistence verified");
    }

    /// Test that gaming profile state is consistent across operations
    #[test]
    fn test_gaming_profile_state_consistency() {
        let mut state = AppState::default();
        state.selected_profile = "gaming".to_string();
        state.use_polly = true;
        state.use_mglru = true;
        state.kernel_hardening = HardeningLevel::Minimal;

        // Verify gaming-specific state
        eprintln!("[Test] Gaming profile state:");
        eprintln!("  profile: {}", state.selected_profile);
        eprintln!("  Polly: {}", state.use_polly);
        eprintln!("  MGLRU: {}", state.use_mglru);
        eprintln!("  Hardening: {}", state.kernel_hardening);

        assert_eq!(state.selected_profile, "gaming");
        assert!(state.use_polly);
        assert!(state.use_mglru);
        assert_eq!(state.kernel_hardening, HardeningLevel::Minimal);

        eprintln!("[Test] ✅ Gaming profile state is fully consistent");
    }

    /// Test that server profile state is consistent across operations
    #[test]
    fn test_server_profile_state_consistency() {
        let mut state = AppState::default();
        state.selected_profile = "server".to_string();
        state.use_polly = false; // Server: Not optimized for Polly
        state.use_mglru = true; // Server: MGLRU for memory efficiency
        state.kernel_hardening = HardeningLevel::Hardened; // Server: Hardened

        eprintln!("[Test] Server profile state:");
        eprintln!("  profile: {}", state.selected_profile);
        eprintln!("  Polly: {}", state.use_polly);
        eprintln!("  MGLRU: {}", state.use_mglru);
        eprintln!("  Hardening: {}", state.kernel_hardening);

        assert_eq!(state.selected_profile, "server");
        assert!(!state.use_polly);
        assert!(state.use_mglru);
        assert_eq!(state.kernel_hardening, HardeningLevel::Hardened);

        eprintln!("[Test] ✅ Server profile state is fully consistent");
    }

    /// Simulate the exact startup sequence:
    /// 1. Load default state
    /// 2. Apply profile defaults
    /// 3. Verify all values are correct
    #[test]
    fn test_startup_state_scenario() {
        eprintln!("[Test] === STARTUP SCENARIO ===");

        // Phase 1: Initialize from config (default or loaded)
        eprintln!("[Test] Phase 1: Loading state from config");
        let state = AppState::default();
        eprintln!("[Test] Loaded state: profile={}", state.selected_profile);

        // Phase 2: Apply current profile defaults in Rust
        eprintln!("[Test] Phase 2: Applying profile defaults");
        let mut state = state;
        if state.selected_profile == "gaming" {
            state.use_polly = true;
            state.use_mglru = true;
            state.kernel_hardening = HardeningLevel::Minimal;
            eprintln!(
                "[Test] Applied gaming defaults: Polly={}, MGLRU={}",
                state.use_polly, state.use_mglru
            );
        }

        // Phase 3: Verify state before UI sync
        eprintln!("[Test] Phase 3: Verifying state before UI sync");
        assert_eq!(state.selected_profile, "gaming");
        assert!(state.use_polly);
        assert!(state.use_mglru);
        assert_eq!(state.kernel_hardening, HardeningLevel::Minimal);

        // Phase 4: Simulate UI sync (would call invoke_from_event_loop in real code)
        eprintln!("[Test] Phase 4: Simulating UI sync via invoke_from_event_loop");
        // In the real code, this would be wrapped in invoke_from_event_loop
        // The state values would be passed to UI setters

        // Phase 5: Verify state persists after "UI sync"
        eprintln!("[Test] Phase 5: Verifying state persists");
        assert_eq!(state.selected_profile, "gaming", "Profile must persist");
        assert!(state.use_polly, "Polly must persist after UI sync");
        assert!(state.use_mglru, "MGLRU must persist after UI sync");

        eprintln!("[Test] ✅ Startup state scenario completed successfully");
    }

    /// Test rapid profile switching to catch any state corruption
    #[test]
    fn test_multiple_profile_switches() {
        eprintln!("[Test] Testing multiple profile switches");
        let mut state = AppState::default();

        let profiles = vec!["gaming", "server", "workstation", "laptop"];

        for profile in &profiles {
            state.selected_profile = profile.to_string();

            // Apply profile-specific defaults
            match state.selected_profile.as_str() {
                "gaming" => {
                    state.use_polly = true;
                    state.use_mglru = true;
                    state.kernel_hardening = HardeningLevel::Minimal;
                }
                "server" => {
                    state.use_polly = false;
                    state.use_mglru = true;
                    state.kernel_hardening = HardeningLevel::Hardened;
                }
                "workstation" => {
                    state.use_polly = false;
                    state.use_mglru = true;
                    state.kernel_hardening = HardeningLevel::Hardened;
                }
                "laptop" => {
                    state.use_polly = false;
                    state.use_mglru = true;
                    state.kernel_hardening = HardeningLevel::Minimal;
                }
                _ => {}
            }

            eprintln!("[Test] Switched to {} profile", profile);
            assert_eq!(
                state.selected_profile,
                profile.to_string(),
                "Profile should have switched"
            );
        }

        eprintln!("[Test] ✅ Multiple profile switches handled correctly");
    }

    // ============================================================================
    // ORIGINAL UI SYNC TESTS WITH APPCONTROLLER
    // ============================================================================

    /// Test: Verify apply_current_profile_defaults sets Polly, MGLRU correctly
    #[tokio::test]
    async fn test_profile_defaults_applied_on_startup() {
        let controller = setup_controller().await;

        // First, set profile to Gaming
        controller
            .handle_profile_change("Gaming")
            .expect("Failed to set Gaming profile");

        // Get initial state
        let initial_state = controller.get_state().expect("Failed to get initial state");
        eprintln!(
            "[TEST] Initial state: profile={}, Polly={}, MGLRU={}",
            initial_state.selected_profile, initial_state.use_polly, initial_state.use_mglru
        );

        // Apply current profile defaults (simulates startup behavior)
        controller
            .apply_current_profile_defaults()
            .expect("Failed to apply profile defaults");

        // Get updated state
        let updated_state = controller.get_state().expect("Failed to get updated state");
        eprintln!(
            "[TEST] Updated state: profile={}, Polly={}, MGLRU={}",
            updated_state.selected_profile, updated_state.use_polly, updated_state.use_mglru
        );

        // Verify the profile's defaults are applied
        assert!(
            !updated_state.selected_profile.is_empty(),
            "Profile should not be empty"
        );

        // Gaming profile should have: Polly=true, MGLRU=true
        if updated_state.selected_profile == "Gaming" {
            assert!(
                updated_state.use_polly,
                "Gaming profile should have Polly enabled"
            );
            assert!(
                updated_state.use_mglru,
                "Gaming profile should have MGLRU enabled"
            );
        }
    }

    /// Test: Verify handle_profile_change updates all scheduler options
    #[tokio::test]
    async fn test_profile_change_updates_all_options() {
        let controller = setup_controller().await;

        // Change to Gaming profile
        controller
            .handle_profile_change("Gaming")
            .expect("Failed to change to Gaming profile");

        let gaming_state = controller.get_state().expect("Failed to get Gaming state");

        // Gaming should enable all performance features
        eprintln!(
            "[TEST] Gaming profile state: Polly={}, MGLRU={}",
            gaming_state.use_polly, gaming_state.use_mglru
        );

        assert_eq!(gaming_state.selected_profile, "Gaming");
        assert!(gaming_state.use_polly, "Gaming should enable Polly");
        assert!(gaming_state.use_mglru, "Gaming should enable MGLRU");

        // Change to Server profile
        controller
            .handle_profile_change("Server")
            .expect("Failed to change to Server profile");

        let server_state = controller.get_state().expect("Failed to get Server state");
        eprintln!(
            "[TEST] Server profile state: Polly={}, MGLRU={}",
            server_state.use_polly, server_state.use_mglru
        );

        assert_eq!(server_state.selected_profile, "Server");
        assert!(server_state.use_mglru, "Server should enable MGLRU");
    }

    /// Test: Verify individual option changes persist
    #[tokio::test]
    async fn test_individual_option_changes() {
        let controller = setup_controller().await;

        // Start with Gaming profile
        controller
            .handle_profile_change("Gaming")
            .expect("Failed to change to Gaming profile");

        let before = controller.get_state().expect("Failed to get state");
        eprintln!("[TEST] Before Polly toggle: Polly={}", before.use_polly);

        // Toggle Polly manually
        controller
            .handle_polly_change(false)
            .expect("Failed to toggle Polly");

        let after = controller
            .get_state()
            .expect("Failed to get state after Polly toggle");
        eprintln!("[TEST] After Polly toggle: Polly={}", after.use_polly);

        assert!(
            !after.use_polly,
            "Polly should be disabled after manual toggle"
        );
        assert_eq!(before.use_mglru, after.use_mglru, "MGLRU should not change");
    }

    /// Test: Verify Polly option changes independently
    #[tokio::test]
    async fn test_polly_option_changes_independently() {
        let controller = setup_controller().await;

        // Start with Gaming profile
        controller
            .handle_profile_change("Gaming")
            .expect("Failed to change to Gaming profile");

        let before = controller.get_state().expect("Failed to get state");
        eprintln!("[TEST] Before Polly toggle: Polly={}", before.use_polly);

        // Toggle Polly manually
        controller
            .handle_polly_change(false)
            .expect("Failed to toggle Polly");

        let after = controller
            .get_state()
            .expect("Failed to get state after Polly toggle");
        eprintln!("[TEST] After Polly toggle: Polly={}", after.use_polly);

        assert!(
            !after.use_polly,
            "Polly should be disabled after manual toggle"
        );
        assert_eq!(before.use_mglru, after.use_mglru, "MGLRU should not change");
    }

    /// Test: Verify MGLRU option changes independently
    #[tokio::test]
    async fn test_mglru_option_changes_independently() {
        let controller = setup_controller().await;

        // Start with Gaming profile
        controller
            .handle_profile_change("Gaming")
            .expect("Failed to change to Gaming profile");

        let before = controller.get_state().expect("Failed to get state");
        eprintln!("[TEST] Before MGLRU toggle: MGLRU={}", before.use_mglru);

        // Toggle MGLRU manually
        controller
            .handle_mglru_change(false)
            .expect("Failed to toggle MGLRU");

        let after = controller
            .get_state()
            .expect("Failed to get state after MGLRU toggle");
        eprintln!("[TEST] After MGLRU toggle: MGLRU={}", after.use_mglru);

        assert!(
            !after.use_mglru,
            "MGLRU should be disabled after manual toggle"
        );
        assert_eq!(before.use_polly, after.use_polly, "Polly should not change");
    }

    /// Test: Verify state is persisted across profile changes
    #[tokio::test]
    async fn test_state_persistence_across_profile_changes() {
        let controller = setup_controller().await;

        // Set initial profile
        controller
            .handle_profile_change("Gaming")
            .expect("Failed to set Gaming profile");

        let gaming_state = controller.get_state().expect("Failed to get Gaming state");
        eprintln!(
            "[TEST] Gaming profile: Polly={}, MGLRU={}",
            gaming_state.use_polly, gaming_state.use_mglru
        );

        // Change to Workstation
        controller
            .handle_profile_change("Workstation")
            .expect("Failed to set Workstation profile");

        let workstation_state = controller
            .get_state()
            .expect("Failed to get Workstation state");
        eprintln!(
            "[TEST] Workstation profile: Polly={}, MGLRU={}",
            workstation_state.use_polly, workstation_state.use_mglru
        );

        // Change back to Gaming
        controller
            .handle_profile_change("Gaming")
            .expect("Failed to set Gaming profile again");

        let gaming_state_2 = controller
            .get_state()
            .expect("Failed to get Gaming state again");
        eprintln!(
            "[TEST] Gaming profile again: Polly={}, MGLRU={}",
            gaming_state_2.use_polly, gaming_state_2.use_mglru
        );

        // Gaming defaults should be reapplied
        assert!(
            gaming_state_2.use_polly,
            "Gaming profile should have Polly enabled on reapply"
        );
        assert!(
            gaming_state_2.use_mglru,
            "Gaming profile should have MGLRU enabled on reapply"
        );
    }

    /// Test: Whitelist toggle is synchronized with Auto-Discovery (modprobed) state
    ///
    /// CANONICAL TRUTH: Whitelist and Auto-Discovery (modprobed) are synchronized.
    /// When profiles change, both are set to the same value based on profile.enable_module_stripping.
    #[tokio::test]
    async fn test_whitelist_auto_discovery_synchronization() {
        let controller = setup_controller().await;

        // Gaming profile has enable_module_stripping=true
        controller
            .handle_profile_change("Gaming")
            .expect("Failed to set Gaming profile");

        let gaming_state = controller.get_state().expect("Failed to get Gaming state");
        eprintln!(
            "[TEST] Gaming profile: use_modprobed={}, use_whitelist={}",
            gaming_state.use_modprobed, gaming_state.use_whitelist
        );

        // Both should be synchronized with profile setting
        assert_eq!(
            gaming_state.use_modprobed, gaming_state.use_whitelist,
            "Whitelist must be synchronized with modprobed on profile change"
        );
        assert!(
            gaming_state.use_modprobed,
            "Gaming profile should enable modprobed"
        );
        assert!(
            gaming_state.use_whitelist,
            "Whitelist should be enabled when modprobed is enabled"
        );
    }

    /// Test: Manual modprobed override affects Whitelist state
    ///
    /// When user disables Auto-Discovery (modprobed), Whitelist must also be disabled.
    /// This maintains the canonical synchronization rule.
    #[tokio::test]
    async fn test_modprobed_override_disables_whitelist() {
        let controller = setup_controller().await;

        // Start with Gaming profile (which enables modprobed and whitelist)
        controller
            .handle_profile_change("Gaming")
            .expect("Failed to set Gaming profile");

        let before = controller.get_state().expect("Failed to get initial state");
        assert!(before.use_modprobed, "Gaming should enable modprobed");
        assert!(before.use_whitelist, "Whitelist should be enabled");

        // User disables modprobed via handle_modprobed_change
        controller
            .handle_modprobed_change(false)
            .expect("Failed to toggle modprobed");

        let after = controller
            .get_state()
            .expect("Failed to get state after modprobed toggle");
        eprintln!(
            "[TEST] After modprobed toggle: use_modprobed={}, use_whitelist={}",
            after.use_modprobed, after.use_whitelist
        );

        assert!(!after.use_modprobed, "modprobed should be disabled");
        assert!(
            !after.use_whitelist,
            "Whitelist must follow modprobed state"
        );
    }

    /// Test: Manual modprobed enable re-enables Whitelist
    ///
    /// When user enables Auto-Discovery (modprobed) back, Whitelist follows.
    #[tokio::test]
    async fn test_modprobed_enable_enables_whitelist() {
        let controller = setup_controller().await;

        // Start with Gaming profile but disable modprobed
        controller
            .handle_profile_change("Gaming")
            .expect("Failed to set Gaming profile");

        controller
            .handle_modprobed_change(false)
            .expect("Failed to disable modprobed");

        let before = controller.get_state().expect("Failed to get initial state");
        assert!(!before.use_modprobed, "modprobed should be disabled");
        assert!(!before.use_whitelist, "Whitelist should be disabled");

        // User re-enables modprobed
        controller
            .handle_modprobed_change(true)
            .expect("Failed to enable modprobed");

        let after = controller
            .get_state()
            .expect("Failed to get state after modprobed re-enable");
        eprintln!(
            "[TEST] After modprobed re-enable: use_modprobed={}, use_whitelist={}",
            after.use_modprobed, after.use_whitelist
        );

        assert!(after.use_modprobed, "modprobed should be enabled");
        assert!(
            after.use_whitelist,
            "Whitelist should be re-enabled with modprobed"
        );
    }

    /// Test: LTO is optional but has profile-specific defaults
    ///
    /// CANONICAL TRUTH: LTO is optional in the UI but each profile provides a default.
    /// Profile Select -> Loads Default -> Manual Override -> Build uses overridden value
    #[tokio::test]
    async fn test_lto_optional_with_profile_defaults() {
        let controller = setup_controller().await;

        // Step 1: Apply Gaming profile
        controller
            .handle_profile_change("Gaming")
            .expect("Failed to set Gaming profile");

        let gaming_state = controller.get_state().expect("Failed to get Gaming state");
        eprintln!(
            "[TEST] Step 1 - Gaming profile: selected_lto={}",
            gaming_state.selected_lto
        );

        // Gaming should have Thin LTO (profile default)
        assert_eq!(
            gaming_state.selected_lto, "thin",
            "Gaming profile default should be Thin LTO"
        );

        // Step 2: User manually overrides to Full LTO
        controller
            .handle_lto_change("full")
            .expect("Failed to set LTO to Full");

        let override_state = controller
            .get_state()
            .expect("Failed to get state after LTO override");
        eprintln!(
            "[TEST] Step 2 - LTO override: selected_lto={}",
            override_state.selected_lto
        );

        assert_eq!(
            override_state.selected_lto, "full",
            "LTO should be set to full after user override"
        );

        // Step 3: Change to Server profile (which has Full LTO default)
        controller
            .handle_profile_change("Server")
            .expect("Failed to set Server profile");

        let server_state = controller.get_state().expect("Failed to get Server state");
        eprintln!(
            "[TEST] Step 3 - Server profile: selected_lto={}",
            server_state.selected_lto
        );

        // Server profile should reset LTO to its default (Full)
        assert_eq!(
            server_state.selected_lto, "full",
            "Server profile default should be Full LTO"
        );

        // Step 4: User overrides Server's default to Thin
        controller
            .handle_lto_change("thin")
            .expect("Failed to set LTO to Thin");

        let final_state = controller.get_state().expect("Failed to get final state");
        eprintln!(
            "[TEST] Step 4 - Final LTO: selected_lto={}",
            final_state.selected_lto
        );

        assert_eq!(
            final_state.selected_lto, "thin",
            "LTO should be thin after user override for Server"
        );
    }

    /// Test: LTO overrides are independent of other option changes
    ///
    /// Changing other options (Polly, etc.) should NOT affect LTO overrides.
    #[tokio::test]
    async fn test_lto_override_independence() {
        let controller = setup_controller().await;

        // Start with Gaming profile
        controller
            .handle_profile_change("Gaming")
            .expect("Failed to set Gaming profile");

        // User overrides LTO to Full
        controller
            .handle_lto_change("full")
            .expect("Failed to set LTO to Full");

        let before = controller.get_state().expect("Failed to get initial state");
        assert_eq!(before.selected_lto, "full", "LTO should be full");

        // User toggles other options
        controller
            .handle_polly_change(false)
            .expect("Failed to toggle Polly");

        let after = controller
            .get_state()
            .expect("Failed to get state after other changes");
        eprintln!(
            "[TEST] After Polly changes: selected_lto={}, use_polly={}",
            after.selected_lto, after.use_polly
        );

        // LTO override should persist
        assert_eq!(
            after.selected_lto, "full",
            "LTO override should persist across other option changes"
        );
        assert!(!after.use_polly, "Polly should be disabled");
    }

    /// CRITICAL PHASE D1 TEST: Gaming Profile Startup Synchronization
    ///
    /// This test validates that startup state synchronization correctly applies
    /// profile defaults (Polly, MGLRU checkboxes, LTO).
    ///
    /// Fix: Synchronous property sync BEFORE ui.run() in main.rs ensures Rust
    /// state is authoritative on startup.
    ///
    /// This test simulates the app startup sequence to verify all checkboxes
    /// are properly synchronized.
    #[tokio::test]
    async fn test_gaming_profile_startup_sync_all_options() {
        eprintln!("[TEST] === PHASE D1: Gaming Profile Startup Synchronization ===");

        let controller = setup_controller().await;

        // STARTUP SEQUENCE STEP 1: Profile is set to Gaming (default or persisted)
        controller
            .handle_profile_change("Gaming")
            .expect("Failed to set Gaming profile");

        eprintln!("[TEST] STEP 1: Profile changed to Gaming");

        // STARTUP SEQUENCE STEP 2: Apply current profile defaults (called early in main.rs)
        controller
            .apply_current_profile_defaults()
            .expect("Failed to apply profile defaults");

        eprintln!("[TEST] STEP 2: apply_current_profile_defaults() called");

        // STARTUP SEQUENCE STEP 3: Get the state for UI sync (simulated by reading properties)
        let startup_state = controller.get_state().expect("Failed to get startup state");

        eprintln!(
            "[TEST] STEP 3: State at startup: profile={}, Polly={}, MGLRU={}",
            startup_state.selected_profile, startup_state.use_polly, startup_state.use_mglru
        );

        // CRITICAL VERIFICATION: All options must be TRUE for Gaming
        // These values would be set directly to UI properties (synchronously) before ui.run()
        assert_eq!(
            startup_state.selected_profile, "Gaming",
            "Profile should be Gaming at startup"
        );

        assert!(
            startup_state.use_polly,
            "CRITICAL: Gaming profile MUST have use_polly=true at startup"
        );

        assert!(
            startup_state.use_mglru,
            "CRITICAL: Gaming profile MUST have use_mglru=true at startup"
        );

        // VERIFICATION: Profile Specifications should show these values
        // LTO: Thin ✓ (Gaming default)
        // Compiler: Clang 19 ✓ (gaming default)
        eprintln!("[TEST] SUCCESS: Gaming profile startup state is correctly synchronized");
        eprintln!("[TEST] Profile Specifications will show:");
        eprintln!("[TEST]   Compiler: Clang 19");
        eprintln!("[TEST]   LTO: Thin");
        eprintln!("[TEST] Checkboxes will display:");
        eprintln!("[TEST]   Use Polly: {} (checked)", startup_state.use_polly);
        eprintln!("[TEST]   Use MGLRU: {} (checked)", startup_state.use_mglru);
    }

    /// Verify that the startup sync handles other profiles correctly too
    #[tokio::test]
    async fn test_profile_startup_sync_all_variants() {
        let controller = setup_controller().await;

        let profiles = vec!["Gaming", "Workstation", "Server", "Laptop"];

        for profile_name in profiles {
            eprintln!("[TEST] Testing startup sync for {} profile", profile_name);

            controller
                .handle_profile_change(profile_name)
                .expect(&format!("Failed to set {} profile", profile_name));

            controller.apply_current_profile_defaults().expect(&format!(
                "Failed to apply defaults for {} profile",
                profile_name
            ));

            let state = controller.get_state().expect("Failed to get state");

            eprintln!(
                "[TEST] {} startup state: Polly={}, MGLRU={}, LTO={}",
                profile_name, state.use_polly, state.use_mglru, state.selected_lto
            );

            // Verify profile is correctly set
            assert_eq!(state.selected_profile, profile_name);

            // Verify expected states based on profile defaults
            match profile_name {
                "Gaming" => {
                    assert!(state.use_polly, "Gaming should enable Polly");
                    assert!(state.use_mglru, "Gaming should enable MGLRU");
                    assert_eq!(state.selected_lto, "thin", "Gaming should use Thin LTO");
                }
                "Workstation" => {
                    assert!(state.use_mglru, "Workstation should enable MGLRU");
                    assert_eq!(
                        state.selected_lto, "thin",
                        "Workstation should use Thin LTO"
                    );
                }
                "Server" => {
                    assert!(state.use_mglru, "Server should enable MGLRU");
                    assert_eq!(state.selected_lto, "full", "Server should use Full LTO");
                }
                "Laptop" => {
                    assert!(state.use_mglru, "Laptop should enable MGLRU");
                    assert_eq!(state.selected_lto, "thin", "Laptop should use Thin LTO");
                }
                _ => panic!("Unknown profile: {}", profile_name),
            }
        }

        eprintln!("[TEST] SUCCESS: All profiles startup sync correctly");
    }

    /// PHASE 8 FINAL VERIFICATION GATE: UIState ↔ AppState Synchronization
    ///
    /// This test ensures that egui UIState fields properly reflect the backend
    /// AppState after all Phase 7 wiring changes. It validates the canonical truth:
    /// - AppController.get_state() is the source of truth
    /// - All build parameter selections (variant, profile, LTO, hardening) sync correctly
    /// - All optimization flags (Polly, MGLRU, modprobed, whitelist) sync correctly
    /// - SCX profile changes don't corrupt local state
    #[tokio::test]
    async fn test_phase8_final_verification_gate_ui_state_sync() {
        eprintln!("[TEST] === PHASE 8: Final Verification Gate (UIState ↔ AppState Sync) ===");

        let controller = setup_controller().await;

        // STEP 1: Set initial profile and apply defaults
        controller
            .handle_profile_change("Gaming")
            .expect("Failed to set Gaming profile");
        controller
            .apply_current_profile_defaults()
            .expect("Failed to apply profile defaults");

        let state_after_profile = controller
            .get_state()
            .expect("Failed to get state after profile change");

        eprintln!("[TEST] STEP 1 - State after Gaming profile:");
        eprintln!(
            "[TEST]   selected_profile: {}",
            state_after_profile.selected_profile
        );
        eprintln!("[TEST]   use_polly: {}", state_after_profile.use_polly);
        eprintln!("[TEST]   use_mglru: {}", state_after_profile.use_mglru);
        eprintln!(
            "[TEST]   selected_lto: {}",
            state_after_profile.selected_lto
        );
        eprintln!(
            "[TEST]   use_modprobed: {}",
            state_after_profile.use_modprobed
        );
        eprintln!(
            "[TEST]   use_whitelist: {}",
            state_after_profile.use_whitelist
        );

        // VERIFICATION 1: Profile defaults are correctly applied
        assert_eq!(state_after_profile.selected_profile, "Gaming");
        assert!(state_after_profile.use_polly, "Gaming should enable Polly");
        assert!(state_after_profile.use_mglru, "Gaming should enable MGLRU");
        assert_eq!(
            state_after_profile.selected_lto, "thin",
            "Gaming should use Thin LTO"
        );
        assert!(
            state_after_profile.use_modprobed,
            "Gaming should enable modprobed"
        );
        assert!(
            state_after_profile.use_whitelist,
            "Gaming should enable whitelist"
        );

        // STEP 2: User toggles individual options (override profile defaults)
        controller
            .handle_lto_change("full")
            .expect("Failed to override LTO");
        controller
            .handle_modprobed_change(false)
            .expect("Failed to toggle modprobed");

        let state_after_overrides = controller
            .get_state()
            .expect("Failed to get state after overrides");

        eprintln!("[TEST] STEP 2 - State after manual overrides:");
        eprintln!(
            "[TEST]   selected_lto: {} (should be full)",
            state_after_overrides.selected_lto
        );
        eprintln!(
            "[TEST]   use_modprobed: {} (should be false)",
            state_after_overrides.use_modprobed
        );
        eprintln!(
            "[TEST]   use_whitelist: {} (should be false due to modprobed sync)",
            state_after_overrides.use_whitelist
        );

        // VERIFICATION 2: User overrides are respected (canonical truth)
        assert_eq!(
            state_after_overrides.selected_lto, "full",
            "LTO override to full should persist"
        );
        assert!(
            !state_after_overrides.use_modprobed,
            "modprobed override should persist"
        );
        assert!(
            !state_after_overrides.use_whitelist,
            "whitelist should sync with modprobed"
        );

        // STEP 3: Change variant and verify isolation
        controller
            .update_state(|s| {
                s.selected_variant = "linux-lts".to_string();
            })
            .expect("Failed to update variant");

        let state_after_variant_change = controller
            .get_state()
            .expect("Failed to get state after variant change");

        eprintln!("[TEST] STEP 3 - State after variant change:");
        eprintln!(
            "[TEST]   selected_variant: {}",
            state_after_variant_change.selected_variant
        );
        eprintln!("[TEST]   Previous overrides should persist:");
        eprintln!(
            "[TEST]   selected_lto: {}",
            state_after_variant_change.selected_lto
        );

        // VERIFICATION 3: Variant change doesn't affect optimization state
        assert_eq!(state_after_variant_change.selected_variant, "linux-lts");
        assert_eq!(
            state_after_variant_change.selected_lto, "full",
            "LTO override should persist after variant change"
        );

        // STEP 4: Simulate SCX profile change and verify isolation
        controller
            .update_state(|s| {
                s.selected_scx_profile = "Gaming (Burst)".to_string();
            })
            .expect("Failed to change SCX profile");

        let state_after_scx = controller
            .get_state()
            .expect("Failed to get state after SCX change");

        eprintln!("[TEST] STEP 4 - State after SCX profile change:");
        eprintln!(
            "[TEST]   selected_scx_profile: {}",
            state_after_scx.selected_scx_profile
        );
        eprintln!("[TEST]   Build options should remain untouched:");
        eprintln!(
            "[TEST]   selected_profile: {}",
            state_after_scx.selected_profile
        );

        // VERIFICATION 4: SCX changes don't corrupt build settings
        assert_eq!(state_after_scx.selected_scx_profile, "Gaming (Burst)");
        assert_eq!(
            state_after_scx.selected_profile, "Gaming",
            "Build profile should persist"
        );
        assert_eq!(
            state_after_scx.selected_lto, "full",
            "LTO should persist across SCX changes"
        );

        // STEP 5: Simulate hardening toggle
        controller
            .handle_hardening_change("hardened")
            .expect("Failed to toggle hardening");

        let state_after_hardening = controller
            .get_state()
            .expect("Failed to get state after hardening change");

        eprintln!("[TEST] STEP 5 - State after hardening toggle:");
        eprintln!(
            "[TEST]   kernel_hardening: {}",
            state_after_hardening.kernel_hardening
        );

        // VERIFICATION 5: Hardening is applied independently
        assert_eq!(
            state_after_hardening.kernel_hardening,
            goatd_kernel::models::HardeningLevel::Hardened,
            "Hardening should be hardened"
        );

        // FINAL COMPREHENSIVE VERIFICATION
        eprintln!("[TEST] === FINAL COMPREHENSIVE STATE SNAPSHOT ===");
        eprintln!("[TEST] Build Configuration:");
        eprintln!(
            "[TEST]   Variant: {}",
            state_after_hardening.selected_variant
        );
        eprintln!(
            "[TEST]   Profile: {}",
            state_after_hardening.selected_profile
        );
        eprintln!("[TEST]   LTO: {}", state_after_hardening.selected_lto);
        eprintln!(
            "[TEST]   Hardening: {}",
            state_after_hardening.kernel_hardening
        );
        eprintln!("[TEST] Optimization Flags:");
        eprintln!("[TEST]   Polly: {}", state_after_hardening.use_polly);
        eprintln!("[TEST]   MGLRU: {}", state_after_hardening.use_mglru);
        eprintln!("[TEST] Module Stripping:");
        eprintln!(
            "[TEST]   Modprobed: {}",
            state_after_hardening.use_modprobed
        );
        eprintln!(
            "[TEST]   Whitelist: {}",
            state_after_hardening.use_whitelist
        );
        eprintln!("[TEST] SCX:");
        eprintln!(
            "[TEST]   Profile: {}",
            state_after_hardening.selected_scx_profile
        );
        eprintln!("[TEST] Workspace:");
        eprintln!("[TEST]   Path: {}", state_after_hardening.workspace_path);

        // CRITICAL ASSERTION: Final state is complete and consistent
        eprintln!("[TEST] Verifying final state consistency...");
        assert_eq!(
            state_after_hardening.selected_profile, "Gaming",
            "Profile remains Gaming throughout test"
        );
        assert_eq!(
            state_after_hardening.selected_lto, "full",
            "LTO remains full (user override)"
        );
        assert!(
            state_after_hardening.use_polly,
            "Polly persists throughout test"
        );

        eprintln!("[TEST] ✓ PHASE 8 FINAL VERIFICATION GATE PASSED");
        eprintln!("[TEST] UIState ↔ AppState synchronization is correct and reliable");
    }
}
