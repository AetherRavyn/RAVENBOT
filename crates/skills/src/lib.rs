//! RAVENBOT skills system
//!
//! This crate implements the skill trait and provides built-in skills
//! for web search, file operations, shell execution, and more.

// The `make_slides` schema is deliberately descriptive; the nested JSON macro
// needs more expansion budget than the default.
#![recursion_limit = "256"]

pub mod awesome;
pub mod builtin;
pub mod diff;
pub mod exec;
pub mod local;
pub mod registry;
pub mod safety;
pub mod traits;

pub use registry::SkillRegistry;
pub use safety::dangerous_command_reason;
pub use traits::{Skill, SkillContext, SkillError, SkillKind, SkillResult, SkillRisk};

/// Clip a UTF-8 string to at most `max_bytes` without splitting a character.
///
/// Tool output is model-visible text and can contain any script. Indexing a
/// Rust string by bytes panics when the offset lands inside a code point; the
/// cap exists to protect context, not to crash the run.
pub fn clip_utf8(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

#[cfg(test)]
mod clip_tests {
    use super::clip_utf8;

    #[test]
    fn clips_on_a_char_boundary() {
        let text = format!("{}é{}", "a".repeat(1023), "b".repeat(50));
        let clipped = clip_utf8(&text, 1024);
        assert_eq!(clipped.len(), 1023);
        assert!(!clipped.contains('é'));
    }

    #[test]
    fn leaves_short_text_alone() {
        assert_eq!(clip_utf8("hello", 1024), "hello");
    }
}
