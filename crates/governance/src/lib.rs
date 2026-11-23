//! RAVENBOT governance system
//!
//! This crate provides budget management, audit logging,
//! and prompt version control.

pub mod audit;
pub mod budget;
pub mod version_control;

pub use audit::{
    AuditExport, AuditExportFilters, AuditExportPayload, AuditExportSignature, AuditLogRow,
    AuditLogger, AuditPage, AuditQuery, AUDIT_EXPORT_FORMAT, AUDIT_EXPORT_VERSION,
    MAX_AUDIT_EXPORT_ENTRIES,
};
pub use budget::{BudgetCheck, BudgetManager};
pub use version_control::PromptVersionControl;
