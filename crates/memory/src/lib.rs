//! RAVENBOT memory system
//!
//! This crate provides vector-based memory storage with retrieval,
//! decay, and self-review capabilities.

pub mod dedupe;
pub mod embedding;
pub mod eval;
pub mod learning;
pub mod office_memory;
pub mod remote_embedding;
pub mod retrieval;
pub mod self_review;
pub mod store;

pub use dedupe::{
    find_duplicate_groups, normalize_duplicate_threshold, DuplicateGroup, DuplicateMember,
};
pub use embedding::{EmbeddingHandle, EmbeddingProvider};
pub use learning::LearningEngine;
pub use office_memory::OfficeMemoryStore;
pub use remote_embedding::{RemoteEmbedding, RemoteEmbeddingBackend, RemoteEmbeddingConfig};
pub use retrieval::MemoryRetriever;
pub use self_review::SelfReviewer;
pub use store::MemoryStore;

/// Serializes embedding writes with a full reindex.
///
/// Memory writes take a read guard. A backend change takes the write guard for
/// both stores and for the provider swap, so a write either finishes before the
/// reindex (and is included) or waits until the new provider is active.
pub(crate) static MEMORY_INDEX_LOCK: tokio::sync::RwLock<()> = tokio::sync::RwLock::const_new(());

/// Re-embed agent and office memory and run `on_reindexed` while still holding
/// the index write lock. The callback is where the caller swaps the live
/// provider, so no write can slip between the vectors and the provider.
pub async fn reindex_all<F>(
    agent: &MemoryStore,
    office: &OfficeMemoryStore,
    provider: &dyn EmbeddingProvider,
    on_reindexed: F,
) -> Result<(u64, u64), String>
where
    F: FnOnce() + Send,
{
    let _guard = MEMORY_INDEX_LOCK.write().await;
    let agent_count = agent.reindex_locked(provider).await?;
    let office_count = office.reindex_locked(provider).await?;
    on_reindexed();
    Ok((agent_count, office_count))
}
