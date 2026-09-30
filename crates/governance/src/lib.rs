//! RAVENBOT governance system
//!
//! This crate provides budget management, audit logging,
//! and prompt version control.

pub mod budget;
pub mod audit;
pub mod version_control;

pub use budget::BudgetManager;
pub use audit::AuditLogger;
pub use version_control::PromptVersionControl;
