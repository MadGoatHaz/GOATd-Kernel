//! Checkpoint data structures for build state serialization.
//!
//! **IMPORTANT**: This module contains ONLY DATA STRUCTURES.
//! 
//! CheckpointManager and checkpoint persistence logic is now in Python
//! (src/core/build_orchestrator.py). This module remains for potential
//! serialization support if needed.
//!
//! Python handles:
//! - Saving checkpoints to JSON
//! - Loading checkpoints from storage
//! - Managing checkpoint directory
//! - State recovery on resume

// Note: This module is kept minimal since Python now handles checkpointing.
// BuildState serialization is handled via serde in models.rs

#[cfg(test)]
mod tests {
    // Checkpoint tests are now in Python (tests/test_layer_integration.py)
}
