//! Text embedding generation

use async_trait::async_trait;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EmbeddingError {
    #[error("Provider error: {0}")]
    Provider(String),
    #[error("Dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch { expected: usize, actual: usize },
}

/// Trait for generating text embeddings
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate embedding for a single text
    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError>;

    /// Generate embeddings for multiple texts (batch)
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError>;

    /// Get the dimension of embeddings
    fn dimension(&self) -> usize;

    /// Whether this backend can find related text that shares no words.
    ///
    /// The local lexical embedding cannot, so retrieval narrows its candidate
    /// set with FTS5 before computing cosine similarity. A semantic backend
    /// keeps a full vector pass so a paraphrase with no lexical overlap is still
    /// found. Backends opt in explicitly rather than the store guessing from a
    /// model name.
    fn is_semantic(&self) -> bool {
        false
    }
}

/// Local, dependency-free embedding over word, bigram and character-gram
/// features.
///
/// This is deliberately lexical, not a neural model: it runs fully offline,
/// needs no download and cannot send memory text anywhere. FTS5 supplies the
/// exact-term candidate set and this vector reranks it; a configured Ollama or
/// OpenAI backend still replaces it for true semantic recall. The old byte
/// positional hash was sensitive to UTF-8 byte order and could not relate
/// "invoice" to "invoices" at all, which made it noise rather than a fallback.
pub struct LocalEmbedding {
    dimension: usize,
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

impl LocalEmbedding {
    pub fn new(dimension: usize) -> Self {
        Self {
            dimension: dimension.max(1),
        }
    }

    fn feature_embedding(&self, text: &str) -> Vec<f32> {
        let mut vector = vec![0.0f32; self.dimension];
        let tokens = crate::text::tokenize(text);
        if tokens.is_empty() {
            return vector;
        }

        for (index, token) in tokens.iter().enumerate() {
            add_feature(&mut vector, hash_bytes(token.as_bytes()), 1.0);
            if index > 0 {
                add_feature(&mut vector, hash_pair(&tokens[index - 1], token), 0.6);
            }
            let characters: Vec<char> = token.chars().collect();
            if characters.len() >= 6 {
                for window in characters.windows(3) {
                    add_feature(&mut vector, hash_characters(window), 0.25);
                }
            }
        }

        // Sublinear term frequency: a word repeated five times should matter
        // more than once, but not five times as much.
        for value in &mut vector {
            if *value > 1.0 {
                *value = 1.0 + value.ln();
            }
        }

        let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut vector {
                *x /= norm;
            }
        }
        vector
    }
}

fn add_feature(vector: &mut [f32], hash: u64, weight: f32) {
    let index = (hash % vector.len() as u64) as usize;
    vector[index] += weight;
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

fn hash_pair(first: &str, second: &str) -> u64 {
    let mut hash = FNV_OFFSET;
    for &byte in first
        .as_bytes()
        .iter()
        .chain(std::iter::once(&b'_'))
        .chain(second.as_bytes())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

fn hash_characters(characters: &[char]) -> u64 {
    let mut hash = FNV_OFFSET;
    let mut buffer = [0u8; 4];
    for character in characters {
        for &byte in character.encode_utf8(&mut buffer).as_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    hash
}

#[async_trait]
impl EmbeddingProvider for LocalEmbedding {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        Ok(self.feature_embedding(text))
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        Ok(texts.iter().map(|t| self.feature_embedding(t)).collect())
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}

/// A shared, replaceable embedding provider.
///
/// Each memory store keeps a clone of this handle. Configuring a different
/// backend swaps the provider for every store at once, without rebuilding the
/// runtime or its open database handles.
#[derive(Clone)]
pub struct EmbeddingHandle {
    current: std::sync::Arc<std::sync::RwLock<std::sync::Arc<dyn EmbeddingProvider>>>,
}

impl EmbeddingHandle {
    pub fn new(provider: std::sync::Arc<dyn EmbeddingProvider>) -> Self {
        Self {
            current: std::sync::Arc::new(std::sync::RwLock::new(provider)),
        }
    }

    pub fn replace(&self, provider: std::sync::Arc<dyn EmbeddingProvider>) {
        let mut current = self
            .current
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *current = provider;
    }

    pub fn current(&self) -> std::sync::Arc<dyn EmbeddingProvider> {
        self.current
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

#[async_trait]
impl EmbeddingProvider for EmbeddingHandle {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        let provider = self.current();
        provider.embed(text).await
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let provider = self.current();
        provider.embed_batch(texts).await
    }

    fn dimension(&self) -> usize {
        self.current().dimension()
    }

    fn is_semantic(&self) -> bool {
        self.current().is_semantic()
    }
}

/// Compute cosine similarity between two vectors
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot_product / (norm_a * norm_b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_local_embedding() {
        let provider = LocalEmbedding::new(128);
        let embedding = provider.embed("Hello, world!").await.unwrap();
        assert_eq!(embedding.len(), 128);

        // Check normalization
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn local_embedding_ranks_related_text_above_unrelated_text() {
        let provider = LocalEmbedding::new(384);
        let query = provider
            .embed("When does the release train leave?")
            .await
            .unwrap();
        let related = provider
            .embed("The release train departs every Thursday at 14:00 UTC.")
            .await
            .unwrap();
        let unrelated = provider
            .embed("The office printer is on the second floor.")
            .await
            .unwrap();
        assert!(
            cosine_similarity(&query, &related) > cosine_similarity(&query, &unrelated),
            "the lexical embedding must rank the topical fact higher"
        );
    }

    #[tokio::test]
    async fn local_embedding_is_stable_and_order_insensitive_for_bag_features() {
        let provider = LocalEmbedding::new(128);
        let first = provider.embed("backup restore drill").await.unwrap();
        let second = provider.embed("backup restore drill").await.unwrap();
        assert_eq!(first, second, "embeddings must be deterministic");

        let reordered = provider.embed("restore backup drill").await.unwrap();
        assert!(
            cosine_similarity(&first, &reordered) > 0.6,
            "word order should not destroy similarity"
        );
    }

    #[tokio::test]
    async fn local_embedding_handles_empty_and_stop_word_only_text() {
        let provider = LocalEmbedding::new(64);
        assert_eq!(provider.embed("").await.unwrap(), vec![0.0; 64]);
        assert_eq!(provider.embed("the and of").await.unwrap(), vec![0.0; 64]);
    }

    #[tokio::test]
    async fn embedding_handle_replaces_the_provider_for_every_clone() {
        struct Fixed(Vec<f32>);
        #[async_trait]
        impl EmbeddingProvider for Fixed {
            async fn embed(&self, _text: &str) -> Result<Vec<f32>, EmbeddingError> {
                Ok(self.0.clone())
            }
            async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
                Ok(texts.iter().map(|_| self.0.clone()).collect())
            }
            fn dimension(&self) -> usize {
                self.0.len()
            }
        }

        let handle = EmbeddingHandle::new(std::sync::Arc::new(Fixed(vec![1.0, 0.0])));
        let clone = handle.clone();
        assert_eq!(handle.embed("x").await.unwrap(), vec![1.0, 0.0]);
        handle.replace(std::sync::Arc::new(Fixed(vec![0.0, 1.0, 0.0])));
        assert_eq!(clone.embed("x").await.unwrap(), vec![0.0, 1.0, 0.0]);
        assert_eq!(clone.dimension(), 3);
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);

        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &b)).abs() < 0.001);
    }
}
