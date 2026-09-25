//! RAVENBOT core domain types
//!
//! This crate contains the fundamental types that represent the RAVENBOT domain:
//! bots, threads, messages, skills, routines, runs, and more.

pub mod bot;
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

// Re-exports for convenience
pub use bot::*;
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
pub use paths::{APP_IDENTIFIER, app_data_dir, data_dir, default_db_path};
pub use office_memory::*;
