//! Remote embedding providers for semantic memory.
//!
//! Ollama is the local-first path: the embedding model runs on the user's
//! machine and memory text never leaves it. The OpenAI-compatible path is
//! explicit opt-in for users who already run a compatible endpoint. The app
//! never calls a remote endpoint unless the user configured one.

use crate::embedding::{EmbeddingError, EmbeddingProvider};
use async_trait::async_trait;
use std::time::Duration;

const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteEmbeddingBackend {
    Ollama,
    OpenAiCompatible,
}

#[derive(Debug, Clone)]
pub struct RemoteEmbeddingConfig {
    pub backend: RemoteEmbeddingBackend,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
}

pub struct RemoteEmbedding {
    config: RemoteEmbeddingConfig,
    client: reqwest::Client,
    dimension: std::sync::atomic::AtomicUsize,
}

impl RemoteEmbedding {
    pub fn new(config: RemoteEmbeddingConfig) -> Result<Self, EmbeddingError> {
        validate_base_url(&config.base_url)?;
        if config.model.trim().is_empty() {
            return Err(EmbeddingError::Provider(
                "An embedding model name is required.".to_string(),
            ));
        }
        if config.backend == RemoteEmbeddingBackend::OpenAiCompatible
            && config
                .api_key
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_none()
        {
            return Err(EmbeddingError::Provider(
                "This embedding endpoint requires an API key.".to_string(),
            ));
        }
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| EmbeddingError::Provider(error.to_string()))?;
        Ok(Self {
            config,
            client,
            dimension: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    async fn request(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let vectors = match self.config.backend {
            RemoteEmbeddingBackend::Ollama => self.request_ollama(texts).await?,
            RemoteEmbeddingBackend::OpenAiCompatible => self.request_openai(texts).await?,
        };
        if let Some(first) = vectors.first() {
            self.dimension
                .store(first.len(), std::sync::atomic::Ordering::Relaxed);
        }
        Ok(vectors)
    }

    async fn request_ollama(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let url = format!("{}/api/embed", self.config.base_url.trim_end_matches('/'));
        let response = self
            .client
            .post(url)
            .json(&serde_json::json!({
                "model": self.config.model,
                "input": texts,
            }))
            .send()
            .await
            .map_err(|error| EmbeddingError::Provider(error.to_string()))?;
        let body = read_body(response).await?;
        let parsed: OllamaResponse = serde_json::from_str(&body).map_err(|error| {
            EmbeddingError::Provider(format!(
                "The Ollama embedding response was not valid: {error}"
            ))
        })?;
        validate_vectors(parsed.embeddings, texts.len())
    }

    async fn request_openai(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let url = format!("{}/embeddings", self.config.base_url.trim_end_matches('/'));
        let key = self
            .config
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| EmbeddingError::Provider("Missing embedding API key.".to_string()))?;
        let response = self
            .client
            .post(url)
            .bearer_auth(key)
            .json(&serde_json::json!({
                "model": self.config.model,
                "input": texts,
            }))
            .send()
            .await
            .map_err(|error| EmbeddingError::Provider(error.to_string()))?;
        let body = read_body(response).await?;
        let mut parsed: OpenAiResponse = serde_json::from_str(&body).map_err(|error| {
            EmbeddingError::Provider(format!(
                "The embedding endpoint response was not valid: {error}"
            ))
        })?;
        parsed.data.sort_by_key(|item| item.index.unwrap_or(0));
        validate_vectors(
            parsed.data.into_iter().map(|item| item.embedding).collect(),
            texts.len(),
        )
    }
}

#[async_trait]
impl EmbeddingProvider for RemoteEmbedding {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        let mut vectors = self.request(&[text]).await?;
        vectors.pop().ok_or_else(|| {
            EmbeddingError::Provider("The provider returned no embedding.".to_string())
        })
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        self.request(texts).await
    }

    fn dimension(&self) -> usize {
        self.dimension.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn is_semantic(&self) -> bool {
        true
    }
}

#[derive(serde::Deserialize)]
struct OllamaResponse {
    embeddings: Vec<Vec<f32>>,
}

#[derive(serde::Deserialize)]
struct OpenAiResponse {
    data: Vec<OpenAiEmbedding>,
}

#[derive(serde::Deserialize)]
struct OpenAiEmbedding {
    embedding: Vec<f32>,
    #[serde(default)]
    index: Option<usize>,
}

async fn read_body(response: reqwest::Response) -> Result<String, EmbeddingError> {
    let status = response.status();
    let bytes = response
        .bytes()
        .await
        .map_err(|error| EmbeddingError::Provider(error.to_string()))?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(EmbeddingError::Provider(
            "The embedding response is larger than the 16 MB limit.".to_string(),
        ));
    }
    let body = String::from_utf8_lossy(&bytes).to_string();
    if !status.is_success() {
        let detail: String = body.chars().take(300).collect();
        return Err(EmbeddingError::Provider(format!(
            "The embedding endpoint returned HTTP {}: {detail}",
            status.as_u16()
        )));
    }
    Ok(body)
}

fn validate_vectors(
    vectors: Vec<Vec<f32>>,
    expected: usize,
) -> Result<Vec<Vec<f32>>, EmbeddingError> {
    if vectors.len() != expected {
        return Err(EmbeddingError::Provider(format!(
            "The embedding endpoint returned {} vectors for {expected} inputs.",
            vectors.len()
        )));
    }
    let dimension = vectors.first().map(Vec::len).unwrap_or(0);
    if dimension == 0 {
        return Err(EmbeddingError::Provider(
            "The embedding endpoint returned an empty vector.".to_string(),
        ));
    }
    if vectors
        .iter()
        .any(|vector| vector.len() != dimension || vector.iter().any(|value| !value.is_finite()))
    {
        return Err(EmbeddingError::Provider(
            "The embedding endpoint returned inconsistent or non-finite vectors.".to_string(),
        ));
    }
    Ok(vectors)
}

/// Local endpoints may use plain HTTP; anything else must use HTTPS. This keeps
/// an accidental `http://` remote endpoint from sending memory text in clear.
pub fn validate_base_url(raw: &str) -> Result<(), EmbeddingError> {
    let raw = raw.trim();
    if raw.is_empty() || raw.len() > 2048 {
        return Err(EmbeddingError::Provider(
            "The embedding endpoint URL is invalid.".to_string(),
        ));
    }
    let url = reqwest::Url::parse(raw).map_err(|_| {
        EmbeddingError::Provider("The embedding endpoint URL is invalid.".to_string())
    })?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(EmbeddingError::Provider(
            "The embedding endpoint URL must not contain credentials.".to_string(),
        ));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(EmbeddingError::Provider(
            "The embedding endpoint URL must not contain a query or fragment.".to_string(),
        ));
    }
    match url.scheme() {
        "https" => Ok(()),
        "http" => {
            let host = url.host_str().unwrap_or("").to_ascii_lowercase();
            if host == "localhost" || host == "127.0.0.1" || host == "::1" || host == "[::1]" {
                Ok(())
            } else {
                Err(EmbeddingError::Provider(
                    "Use HTTPS for a non-local embedding endpoint.".to_string(),
                ))
            }
        }
        _ => Err(EmbeddingError::Provider(
            "The embedding endpoint must use HTTP or HTTPS.".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn serve(
        status: &'static str,
        body: &'static str,
    ) -> (String, tokio::sync::oneshot::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let mut request = vec![0u8; 8192];
            let read = socket.read(&mut request).await.unwrap_or(0);
            let _ = tx.send(String::from_utf8_lossy(&request[..read]).to_string());
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(response.as_bytes()).await;
        });
        (format!("http://{addr}"), rx)
    }

    #[tokio::test]
    async fn ollama_embeddings_are_parsed_from_the_local_api() {
        let (base_url, request) =
            serve("200 OK", r#"{"embeddings":[[0.1,0.2,0.3],[0.4,0.5,0.6]]}"#).await;
        let provider = RemoteEmbedding::new(RemoteEmbeddingConfig {
            backend: RemoteEmbeddingBackend::Ollama,
            base_url,
            model: "nomic-embed-text".to_string(),
            api_key: None,
        })
        .unwrap();
        let vectors = provider.embed_batch(&["alpha", "beta"]).await.unwrap();
        assert_eq!(vectors.len(), 2);
        assert_eq!(vectors[0], vec![0.1, 0.2, 0.3]);
        assert_eq!(provider.dimension(), 3);
        let request = request.await.unwrap();
        assert!(request.starts_with("POST /api/embed"));
        assert!(request.contains("nomic-embed-text"));
    }

    #[tokio::test]
    async fn openai_compatible_embeddings_use_the_bearer_key() {
        let (base_url, request) =
            serve("200 OK", r#"{"data":[{"embedding":[0.5,0.5],"index":0}]}"#).await;
        let provider = RemoteEmbedding::new(RemoteEmbeddingConfig {
            backend: RemoteEmbeddingBackend::OpenAiCompatible,
            base_url,
            model: "text-embedding-3-small".to_string(),
            api_key: Some("test-key".to_string()),
        })
        .unwrap();
        let vector = provider.embed("hello").await.unwrap();
        assert_eq!(vector, vec![0.5, 0.5]);
        let request = request.await.unwrap();
        assert!(request.starts_with("POST /embeddings"));
        assert!(request
            .to_ascii_lowercase()
            .contains("authorization: bearer test-key"));
    }

    #[tokio::test]
    async fn malformed_or_failed_embedding_responses_are_rejected() {
        let (base_url, _) = serve("200 OK", r#"{"embeddings":[[0.1],[0.2,0.3]]}"#).await;
        let provider = RemoteEmbedding::new(RemoteEmbeddingConfig {
            backend: RemoteEmbeddingBackend::Ollama,
            base_url,
            model: "nomic-embed-text".to_string(),
            api_key: None,
        })
        .unwrap();
        assert!(provider.embed_batch(&["a", "b"]).await.is_err());

        let (base_url, _) = serve("500 Internal Server Error", r#"{"error":"bad"}"#).await;
        let provider = RemoteEmbedding::new(RemoteEmbeddingConfig {
            backend: RemoteEmbeddingBackend::Ollama,
            base_url,
            model: "nomic-embed-text".to_string(),
            api_key: None,
        })
        .unwrap();
        let error = provider.embed("a").await.unwrap_err().to_string();
        assert!(error.contains("HTTP 500"));
    }

    #[test]
    fn endpoint_urls_reject_plaintext_remote_hosts() {
        assert!(validate_base_url("http://127.0.0.1:11434").is_ok());
        assert!(validate_base_url("http://localhost:11434").is_ok());
        assert!(validate_base_url("http://[::1]:11434").is_ok());
        assert!(validate_base_url("https://api.example.com/v1").is_ok());
        assert!(validate_base_url("http://api.example.com/v1").is_err());
        assert!(validate_base_url("ftp://api.example.com").is_err());
        assert!(validate_base_url("https://user:pass@api.example.com").is_err());
        assert!(validate_base_url("https://api.example.com/v1?key=x").is_err());
    }
}
