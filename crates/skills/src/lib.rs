//! RAVENBOT skills system
//!
//! This crate implements the skill trait and provides built-in skills
//! for web search, file operations, shell execution, and more.

pub mod traits;
pub mod builtin;
pub mod registry;
pub mod awesome;

pub use traits::{Skill, SkillContext, SkillError, SkillResult};
pub use registry::SkillRegistry;
