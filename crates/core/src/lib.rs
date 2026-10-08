//! RAVENBOT core domain types
//!
//! This crate contains the fundamental types that represent the RAVENBOT domain:
//! bots, threads, messages, skills, routines, runs, and more.

pub mod bot;
pub mod capability;
pub mod thread;
pub mod message;
pub mod skill;
pub mod routine;
pub mod run;
pub mod memory;
pub mod model;
pub mod budget;
pub mod audit;
pub mod version;
pub mod bundle;
pub mod approval;
pub mod question;
pub mod team;
pub mod channel;
pub mod chatroom;
pub mod paths;
pub mod office_memory;
pub mod file_change;

// Re-exports for convenience
pub use bot::*;
pub use capability::*;
pub use thread::*;
pub use message::*;
pub use skill::*;
pub use routine::*;
pub use run::*;
pub use memory::*;
pub use model::*;
pub use budget::*;
pub use audit::*;
pub use version::*;
pub use bundle::*;
pub use approval::*;
pub use question::*;
pub use team::*;
pub use channel::*;
pub use chatroom::*;
pub use paths::{
    agent_workspace, app_data_dir, cache_dir, config_dir, data_dir, default_db_path, ensure_dir,
    expand_home, keys_dir, legacy_db_path, logs_dir, mark_office_dir, office_workspace, projects_dir,
    raven_root, slugify, APP_IDENTIFIER,
};
pub use office_memory::*;
pub use file_change::*;
