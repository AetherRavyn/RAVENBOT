//! Near-duplicate detection for stored memories.
//!
//! The scan is deliberately conservative: it only compares memories that have
//! embeddings of the same dimension, groups transitive matches, and keeps the
//! most important, most recalled, oldest entry in each group.

use crate::embedding::cosine_similarity;
use chrono::{DateTime, Utc};
use ravenbot_core::{MemoryFact, OfficeMemory};
use serde::Serialize;
use std::collections::HashMap;
use uuid::Uuid;

pub const DEFAULT_DUPLICATE_THRESHOLD: f32 = 0.92;
pub const MIN_DUPLICATE_THRESHOLD: f32 = 0.5;
pub const MAX_DUPLICATE_THRESHOLD: f32 = 0.999;

pub trait DuplicateItem {
    fn duplicate_id(&self) -> Uuid;
    fn duplicate_content(&self) -> &str;
    fn duplicate_importance(&self) -> f32;
    fn duplicate_access_count(&self) -> u32;
    fn duplicate_created_at(&self) -> DateTime<Utc>;
    fn duplicate_embedding(&self) -> Option<&[f32]>;
}

impl DuplicateItem for MemoryFact {
    fn duplicate_id(&self) -> Uuid {
        self.id
    }
    fn duplicate_content(&self) -> &str {
        &self.content
    }
    fn duplicate_importance(&self) -> f32 {
        self.importance
    }
    fn duplicate_access_count(&self) -> u32 {
        self.access_count
    }
    fn duplicate_created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    fn duplicate_embedding(&self) -> Option<&[f32]> {
        self.embedding.as_deref()
    }
}

impl DuplicateItem for OfficeMemory {
    fn duplicate_id(&self) -> Uuid {
        self.id
    }
    fn duplicate_content(&self) -> &str {
        &self.content
    }
    fn duplicate_importance(&self) -> f32 {
        self.importance
    }
    fn duplicate_access_count(&self) -> u32 {
        self.access_count
    }
    fn duplicate_created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    fn duplicate_embedding(&self) -> Option<&[f32]> {
        self.embedding.as_deref()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DuplicateMember {
    pub id: String,
    pub content: String,
    pub importance: f32,
    pub access_count: u32,
    pub created_at: String,
    /// Cosine similarity to the entry that will be kept.
    pub similarity: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct DuplicateGroup {
    pub keep: DuplicateMember,
    pub duplicates: Vec<DuplicateMember>,
}

/// Group memories whose pairwise similarity is at least `threshold`.
pub fn normalize_duplicate_threshold(threshold: f32) -> f32 {
    if threshold.is_finite() {
        threshold.clamp(MIN_DUPLICATE_THRESHOLD, MAX_DUPLICATE_THRESHOLD)
    } else {
        DEFAULT_DUPLICATE_THRESHOLD
    }
}

pub fn find_duplicate_groups<T: DuplicateItem>(items: &[T], threshold: f32) -> Vec<DuplicateGroup> {
    let threshold = normalize_duplicate_threshold(threshold);

    let valid: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            item.duplicate_embedding()
                .map(|embedding| !embedding.is_empty())
                .unwrap_or(false)
        })
        .map(|(index, _)| index)
        .collect();

    let mut parent: Vec<usize> = (0..valid.len()).collect();
    for left in 0..valid.len() {
        for right in (left + 1)..valid.len() {
            let a = items[valid[left]].duplicate_embedding().unwrap_or(&[]);
            let b = items[valid[right]].duplicate_embedding().unwrap_or(&[]);
            if a.len() != b.len() {
                continue;
            }
            if cosine_similarity(a, b) >= threshold {
                union(&mut parent, left, right);
            }
        }
    }

    let mut buckets: HashMap<usize, Vec<usize>> = HashMap::new();
    for (index, original) in valid.iter().enumerate() {
        let root = find(&mut parent, index);
        buckets.entry(root).or_default().push(*original);
    }

    let mut groups: Vec<DuplicateGroup> = buckets
        .into_values()
        .filter(|group| group.len() > 1)
        .map(|mut group| {
            group.sort_by(|a, b| keep_order(&items[*a], &items[*b]));
            let keep_index = group[0];
            let keep_embedding = items[keep_index].duplicate_embedding().unwrap_or(&[]);
            let mut duplicates: Vec<DuplicateMember> = group[1..]
                .iter()
                .map(|index| {
                    let item = &items[*index];
                    let similarity = item
                        .duplicate_embedding()
                        .map(|embedding| cosine_similarity(keep_embedding, embedding))
                        .unwrap_or(0.0);
                    member(item, similarity)
                })
                .collect();
            duplicates.sort_by(|a, b| {
                b.similarity
                    .partial_cmp(&a.similarity)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            DuplicateGroup {
                keep: member(&items[keep_index], 1.0),
                duplicates,
            }
        })
        .collect();

    groups.sort_by(|a, b| {
        b.duplicates.len().cmp(&a.duplicates.len()).then_with(|| {
            b.keep
                .importance
                .partial_cmp(&a.keep.importance)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    groups
}

fn member<T: DuplicateItem>(item: &T, similarity: f32) -> DuplicateMember {
    DuplicateMember {
        id: item.duplicate_id().to_string(),
        content: item.duplicate_content().to_string(),
        importance: item.duplicate_importance(),
        access_count: item.duplicate_access_count(),
        created_at: item.duplicate_created_at().to_rfc3339(),
        similarity,
    }
}

fn keep_order<T: DuplicateItem>(a: &T, b: &T) -> std::cmp::Ordering {
    b.duplicate_importance()
        .partial_cmp(&a.duplicate_importance())
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| b.duplicate_access_count().cmp(&a.duplicate_access_count()))
        .then_with(|| a.duplicate_created_at().cmp(&b.duplicate_created_at()))
}

fn find(parent: &mut [usize], index: usize) -> usize {
    let mut root = index;
    while parent[root] != root {
        root = parent[root];
    }
    let mut current = index;
    while parent[current] != current {
        let next = parent[current];
        parent[current] = root;
        current = next;
    }
    root
}

fn union(parent: &mut [usize], a: usize, b: usize) {
    let root_a = find(parent, a);
    let root_b = find(parent, b);
    if root_a != root_b {
        parent[root_b] = root_a;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ravenbot_core::MemoryFact;

    fn fact(id: u8, content: &str, importance: f32, embedding: Option<Vec<f32>>) -> MemoryFact {
        let now = Utc::now();
        MemoryFact {
            id: Uuid::from_u128(id as u128),
            bot_id: Uuid::from_u128(999),
            content: content.to_string(),
            embedding,
            importance,
            access_count: id as u32,
            last_accessed: now,
            created_at: now,
            source: "unknown".to_string(),
            source_thread_id: None,
            source_run_id: None,
            updated_at: now,
        }
    }

    #[test]
    fn identical_embeddings_group_and_keep_the_most_important_entry() {
        let items = vec![
            fact(1, "Sign releases", 0.4, Some(vec![1.0, 0.0, 0.0])),
            fact(2, "Sign every release", 0.9, Some(vec![1.0, 0.0, 0.0])),
        ];
        let groups = find_duplicate_groups(&items, DEFAULT_DUPLICATE_THRESHOLD);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].keep.id, items[1].id.to_string());
        assert_eq!(groups[0].duplicates.len(), 1);
        assert_eq!(groups[0].duplicates[0].id, items[0].id.to_string());
        assert!((groups[0].duplicates[0].similarity - 1.0).abs() < 1e-6);
    }

    #[test]
    fn distinct_dimensions_and_orthogonal_vectors_are_not_duplicates() {
        let items = vec![
            fact(1, "a", 0.5, Some(vec![1.0, 0.0])),
            fact(2, "b", 0.5, Some(vec![0.0, 1.0])),
            fact(3, "c", 0.5, Some(vec![1.0, 0.0, 0.0])),
            fact(4, "d", 0.5, None),
        ];
        assert!(find_duplicate_groups(&items, DEFAULT_DUPLICATE_THRESHOLD).is_empty());
    }

    #[test]
    fn transitive_matches_form_one_group() {
        let items = vec![
            fact(1, "one", 0.5, Some(vec![1.0, 0.0, 0.0])),
            fact(2, "two", 0.5, Some(vec![1.0, 0.0, 0.0])),
            fact(3, "three", 0.5, Some(vec![1.0, 0.0, 0.0])),
        ];
        let groups = find_duplicate_groups(&items, DEFAULT_DUPLICATE_THRESHOLD);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].duplicates.len(), 2);
    }

    #[test]
    fn threshold_is_clamped_and_keep_order_breaks_ties_by_access_then_age() {
        let now = Utc::now();
        let mut first = fact(1, "first", 0.7, Some(vec![1.0, 0.0]));
        first.access_count = 2;
        first.created_at = now - chrono::Duration::days(2);
        let mut second = fact(2, "second", 0.7, Some(vec![1.0, 0.0]));
        second.access_count = 9;
        second.created_at = now;
        let groups = find_duplicate_groups(&[first, second], 5.0);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].keep.access_count, 9);
        assert!(find_duplicate_groups::<MemoryFact>(&[], 0.0).is_empty());
    }
}
