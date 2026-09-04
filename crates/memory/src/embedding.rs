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
}

/// Simple local embedding using hash-based approach
/// In production, use a proper model like all-MiniLM-L6-v2 via candle
pub struct LocalEmbedding {
    dimension: usize,
}

impl LocalEmbedding {
    pub fn new(dimension: usize) -> Self {
        Self { dimension }
    }

    /// Create a simple hash-based embedding (for demo purposes)
    fn simple_hash_embedding(&self, text: &str) -> Vec<f32> {
        let mut embedding = vec![0.0; self.dimension];
        let bytes = text.as_bytes();
        
        for (i, &byte) in bytes.iter().enumerate() {
            let idx = (i * 7 + byte as usize) % self.dimension;
            embedding[idx] += 1.0;
        }

        // Normalize
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut embedding {
                *x /= norm;
            }
        }

        embedding
    }
}

#[async_trait]
impl EmbeddingProvider for LocalEmbedding {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        Ok(self.simple_hash_embedding(text))
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        Ok(texts.iter().map(|t| self.simple_hash_embedding(t)).collect())
    }

    fn dimension(&self) -> usize {
        self.dimension
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
