//! RAVENBOT memory system
//!
//! This crate provides vector-based memory storage with retrieval,
//! decay, and self-review capabilities.

pub mod embedding;
pub mod store;
pub mod retrieval;
pub mod self_review;
pub mod office_memory;
pub mod learning;

pub use embedding::EmbeddingProvider;
pub use store::MemoryStore;
pub use retrieval::MemoryRetriever;
pub use self_review::SelfReviewer;
pub use office_memory::OfficeMemoryStore;
pub use learning::LearningEngine;
