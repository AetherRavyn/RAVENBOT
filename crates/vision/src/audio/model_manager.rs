//! Model download + cache for voice models.
//!
//! Models live in `~/.ravenbot/models/<repo>/...` and are reused across
//! restarts. We download via the Hugging Face Hub HTTP API (no hf-hub
//! dependency needed) so the flow is transparent and debuggable.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("Download failed: {0}")]
    DownloadFailed(String),
    #[error("Model not found: {0}")]
    NotFound(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Root dir for cached models.
///
/// `RAVENBOT_MODEL_HOME` overrides it; otherwise the weights land in
/// `~/RAVENBOT/cache/models` alongside everything else RAVENBOT owns, rather
/// than in a second hidden `~/.ravenbot` tree that a user backing up or
/// uninstalling would miss.
pub fn models_root() -> PathBuf {
    if let Ok(home) = std::env::var("RAVENBOT_MODEL_HOME") {
        let home = home.trim();
        if !home.is_empty() {
            let home = PathBuf::from(home);
            let _ = std::fs::create_dir_all(&home);
            return home;
        }
    }
    ravenbot_core::ensure_dir(&ravenbot_core::cache_dir().join("models"))
}

/// Local cache path for a repo.
pub fn model_dir(repo: &str) -> PathBuf {
    let mut p = models_root();
    p.push(repo.replace('/', "_"));
    p
}

/// Download a model repo from Hugging Face Hub into the local cache.
/// Uses the HF Tree API to list files, then downloads each one.
pub async fn download_model(repo: &str) -> Result<PathBuf, ModelError> {
    let target = model_dir(repo);
    if is_cached(repo) {
        tracing::info!(path = %target.display(), "Model already cached");
        return Ok(target);
    }

    tracing::info!(repo = repo, "Downloading voice model from Hugging Face Hub");

    // HF Hub tree API: list all files in the repo.
    let url = format!("https://huggingface.co/api/models/{}/tree/main", repo);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| ModelError::DownloadFailed(e.to_string()))?;

    let resp = client.get(&url).send().await
        .map_err(|e| ModelError::DownloadFailed(format!("Failed to reach Hugging Face: {}", e)))?;

    if !resp.status().is_success() {
        return Err(ModelError::DownloadFailed(format!(
            "Repo '{}' not accessible (HTTP {}). Check the name and your network.",
            repo, resp.status()
        )));
    }

    let tree: serde_json::Value = resp.json().await
        .map_err(|e| ModelError::DownloadFailed(format!("Bad response: {}", e)))?;

    let files = tree.as_array()
        .ok_or_else(|| ModelError::DownloadFailed("Unexpected tree format".to_string()))?;

    let mut downloaded = 0;
    for file in files {
        let path = file.get("path").and_then(|p| p.as_str()).unwrap_or("");
        let size = file.get("size").and_then(|s| s.as_u64()).unwrap_or(0);
        if path.is_empty() || size == 0 {
            continue; // skip subdirs
        }

        let file_url = format!("https://huggingface.co/{}/resolve/main/{}", repo, path);
        let file_path = target.join(path);

        if let Some(parent) = file_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        // Skip if already present and non-empty.
        if file_path.exists() && std::fs::metadata(&file_path).map(|m| m.len() == size).unwrap_or(false) {
            continue;
        }

        let data = client.get(&file_url).send().await
            .map_err(|e| ModelError::DownloadFailed(format!("Failed to download {}: {}", path, e)))?
            .bytes().await
            .map_err(|e| ModelError::DownloadFailed(format!("Failed to read {}: {}", path, e)))?;

        std::fs::write(&file_path, &data)?;
        downloaded += 1;
    }

    if downloaded == 0 && !is_cached(repo) {
        return Err(ModelError::DownloadFailed(
            "No files downloaded. The repo may be empty or gated (set HF_TOKEN).".to_string(),
        ));
    }

    tracing::info!(path = %target.display(), files = downloaded, "Model downloaded");
    Ok(target)
}

/// Check if a model is already cached locally.
pub fn is_cached(repo: &str) -> bool {
    let d = model_dir(repo);
    d.exists() && d.read_dir().map(|mut i| i.next().is_some()).unwrap_or(false)
}

/// Find the model weights file in a downloaded model dir.
pub fn find_weights(dir: &std::path::Path) -> Option<PathBuf> {
    for name in &["model.safetensors", "pytorch_model.bin", "model.bin"] {
        let p = dir.join(name);
        if p.exists() { return Some(p); }
    }
    // Recurse one level (some repos nest under a subdir).
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                if let Some(found) = find_weights(&entry.path()) {
                    return Some(found);
                }
            }
        }
    }
    None
}
