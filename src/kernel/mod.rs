//! Kernel Source Management Module
//!
//! Handles interactions with the Linux kernel source tree, including:
//! - Git operations (cloning, fetching) and source integrity verification
//! - Package management (listing, scanning, deletion)
//! - System audits (kernel info, performance metrics)

// Phase 1: Package management submodule
pub mod manager;

// Phase 1: System audit submodule
pub mod audit;

// Phase 2: Git management submodule
pub mod git;

// Phase 2: Parser and Patcher submodules
pub mod parser;
pub mod patcher;

// Phase 2: LTO and Validator submodules
pub mod lto;
pub mod validator;

// Phase 3: Source URL management submodule
pub mod sources;

// Phase 3: PKGBUILD version polling submodule
pub mod pkgbuild;
