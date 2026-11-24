//! RAVENBOT core domain types
//!
//! This crate contains the fundamental types that represent the RAVENBOT domain:
//! bots, threads, messages, skills, routines, runs, and more.

pub mod agent_activity;
pub mod approval;
pub mod audit;
pub mod bot;
pub mod browser_dock;
pub mod budget;
pub mod bundle;
pub mod capability;
pub mod channel;
pub mod chatroom;
pub mod file_change;
pub mod goal;
pub mod memory;
pub mod message;
pub mod model;
pub mod net;
pub mod office_memory;
pub mod paths;
pub mod question;
pub mod routine;
pub mod run;
pub mod skill;
pub mod team;
pub mod thread;
pub mod tool_pricing;
pub mod version;

// Re-exports for convenience
pub use agent_activity::*;
pub use approval::*;
pub use audit::*;
pub use bot::*;
pub use browser_dock::*;
pub use budget::*;
pub use bundle::*;
pub use capability::*;
pub use channel::*;
pub use chatroom::*;
pub use file_change::*;
pub use goal::*;
pub use memory::*;
pub use message::*;
pub use model::*;
pub use office_memory::*;
pub use paths::{
    agent_workspace, app_data_dir, cache_dir, config_dir, data_dir, default_db_path, ensure_dir,
    expand_home, keys_dir, legacy_db_path, logs_dir, mark_office_dir, office_workspace,
    projects_dir, raven_root, slugify, snapshots_dir, worktrees_dir, APP_IDENTIFIER,
};
pub use question::*;
pub use routine::*;
pub use run::*;
pub use skill::*;
pub use team::*;
pub use thread::*;
pub use version::*;
