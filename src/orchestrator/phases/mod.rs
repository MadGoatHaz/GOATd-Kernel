//! Orchestrator phases: modularized build pipeline stages.
//!
//! This module organizes the orchestrator's responsibilities into distinct phases:
//! - **Phase 1: Preparation** (`prep`) - Hardware validation and environment setup
//! - Future phases: Validation, patching, execution, etc.
//!
//! Each phase is independently testable and can be composed into higher-level workflows.

pub mod prep;

pub use prep::prepare_build_environment;
