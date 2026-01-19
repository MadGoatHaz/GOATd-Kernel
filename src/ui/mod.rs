//! UI Module - egui integration and AppController
//!
//! Handles the interface between the Rust backend logic and the egui UI frontend.
//! All Slint code has been removed and the structure has been promoted directly
//! into the ui module for a cleaner architecture.

pub mod controller;
pub mod app;
pub mod dashboard;
pub mod build;
pub mod performance;
pub mod kernels;
pub mod settings;
pub mod widgets;
pub mod threading;

use std::path::PathBuf;
use crate::kernel::manager::KernelPackage;
use futures::future::BoxFuture;

pub use controller::{AppController, BuildEvent};
pub use app::{AppUI, UIState};

/// Trait for system-level operations (OS commands, logging, etc.)
pub trait SystemWrapper: Send + Sync {
    fn install_package(&self, path: PathBuf) -> Result<(), String>;
    fn uninstall_package(&self, pkg_name: &str) -> Result<(), String>;
    fn get_booted_kernel(&self) -> String;
    fn install_nvidia_drivers_dkms(&self, kernel_version: &str) -> Result<(), String>;
    
    /// Execute multiple privileged commands in a single pkexec session
    ///
    /// Reduces authentication prompts by batching all privileged operations.
    fn batch_privileged_commands(&self, commands: Vec<&str>) -> Result<(), String>;
    
    /// Execute multiple commands as the current user (without privileges)
    ///
    /// Joins commands with ` && ` and executes them sequentially as the current user.
    /// Intended for user-level operations that should NOT run with elevated privileges,
    /// such as GPG key imports, git operations, or other user-specific tasks.
    fn batch_user_commands(&self, commands: Vec<&str>) -> Result<(), String>;
}

/// Trait for kernel management operations
pub trait KernelManagerTrait: Send + Sync {
    fn list_installed(&self) -> Vec<KernelPackage>;
    fn scan_workspace(&self, path: &str) -> Vec<KernelPackage>;
    fn delete_built_artifact(&self, pkg: &KernelPackage) -> Result<(), String>;
}

/// Trait for system auditing and metrics collection
pub trait AuditTrait: Send + Sync {
    fn get_summary(&self) -> Result<crate::kernel::audit::AuditSummary, String>;
    fn run_deep_audit_async(&self) -> BoxFuture<'static, Result<crate::kernel::audit::KernelAuditData, String>>;
    /// Run deep audit for a specific kernel version (optional parameter)
    /// If version is None, audits the running kernel
    fn run_deep_audit_async_for_version(&self, version: Option<String>) -> BoxFuture<'static, Result<crate::kernel::audit::KernelAuditData, String>>;
    fn get_performance_metrics(&self) -> Result<crate::kernel::audit::PerformanceMetrics, String>;
    fn run_jitter_audit_async(&self) -> BoxFuture<'static, Result<crate::kernel::audit::JitterAuditResult, String>>;
}
