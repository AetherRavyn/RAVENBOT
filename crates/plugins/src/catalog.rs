//! A real, installable OpenAPI catalog backed by the APIs.guru directory.
//!
//! The directory is an 8–9 MB JSON index covering thousands of public APIs.
//! Downloading and parsing that on every keystroke would be wasteful, so this
//! module compacts it into a small on-disk index and keeps that index in
//! memory for the life of the process. Search, category filters, pagination,
//! and install resolution all run against the cached index.
//!
//! The curated starter picks stay separate because they point at canonical,
//! maintained specs (Stripe's own repository, GitHub's own description, …)
//! rather than at a copy in the directory. When the directory contains the
//! same provider, that duplicate is hidden so the user sees one card.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

pub const APIS_GURU_LIST_URL: &str = "https://api.apis.guru/v2/list.json";
const MAX_LIST_BYTES: usize = 32 * 1024 * 1024;
const CACHE_TTL_SECONDS: i64 = 24 * 60 * 60;
const CACHE_SCHEMA: u32 = 1;
pub const DEFAULT_PAGE_SIZE: usize = 48;
pub const MAX_PAGE_SIZE: usize = 200;

struct CuratedDefinition {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    spec_url: &'static str,
    category: &'static str,
    categories: &'static [&'static str],
    provider: &'static str,
    version: &'static str,
    openapi_version: &'static str,
    requires_auth: bool,
}

/// Canonical starter picks. These specs are fetched live on install; nothing
/// is pre-seeded into the database.
const CURATED: &[CuratedDefinition] = &[
    CuratedDefinition {
        id: "stripe",
        name: "Stripe",
        description: "Payments, invoices, customers, subscriptions, and billing.",
        spec_url: "https://raw.githubusercontent.com/stripe/openapi/master/openapi/spec3.json",
        category: "finance",
        categories: &["finance", "payment"],
        provider: "stripe.com",
        version: "latest",
        openapi_version: "3.0",
        requires_auth: true,
    },
    CuratedDefinition {
        id: "slack",
        name: "Slack",
        description: "Messages, channels, files, and workspace conversations.",
        spec_url: "https://raw.githubusercontent.com/slackapi/slack-api-specs/master/web-api/slack_web_openapi_v2.json",
        category: "chat",
        categories: &["chat", "messaging"],
        provider: "slack.com",
        version: "v2",
        openapi_version: "2.0",
        requires_auth: true,
    },
    CuratedDefinition {
        id: "jira",
        name: "Jira",
        description: "Issues, projects, sprints, and delivery tracking.",
        spec_url: "https://developer.atlassian.com/cloud/jira/platform/swagger-v3.v3.json",
        category: "dev",
        categories: &["dev", "developer_tools", "collaboration"],
        provider: "atlassian.com:jira",
        version: "v3",
        openapi_version: "3.0",
        requires_auth: true,
    },
    CuratedDefinition {
        id: "github",
        name: "GitHub",
        description: "Repositories, pull requests, issues, and code review.",
        spec_url: "https://raw.githubusercontent.com/github/rest-api-description/main/descriptions/api.github.com/api.github.com.json",
        category: "dev",
        categories: &["dev", "developer_tools"],
        provider: "github.com",
        version: "v3",
        openapi_version: "3.0",
        requires_auth: true,
    },
    CuratedDefinition {
        id: "petstore",
        name: "Swagger Petstore",
        description: "A small public OpenAPI example with no credentials.",
        spec_url: "https://petstore3.swagger.io/api/v3/openapi.json",
        category: "demo",
        categories: &["demo", "open_data"],
        provider: "petstore.swagger.io",
        version: "3.0.4",
        openapi_version: "3.0",
        requires_auth: false,
    },
];

/// Useful APIs that deserve a place above the alphabetical tail of the
/// directory. They are ordinary directory entries, not pre-installed rows.
const FEATURED_PROVIDERS: &[&str] = &[
    "notion.com",
    "googleapis.com:gmail",
    "googleapis.com:calendar",
    "googleapis.com:drive",
    "googleapis.com:sheets",
    "openai.com",
    "twilio.com:api",
    "zoom.us",
    "asana.com",
    "trello.com",
    "spotify.com",
    "telegram.org",
    "plaid.com",
    "adyen.com:CheckoutService",
    "sendgrid.com",
    "vimeo.com",
    "1forge.com",
];

/// One installable app in the catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub spec_url: String,
    pub category: String,
    pub categories: Vec<String>,
    pub provider: String,
    pub logo: String,
    pub version: String,
    pub openapi_version: String,
    /// `None` means the directory does not carry security metadata; the
    /// imported spec decides once it is parsed.
    pub requires_auth: Option<bool>,
    pub featured: bool,
    /// Internal ordering hint: curated first, then featured, then the rest.
    #[serde(default = "default_rank")]
    pub rank: u32,
    /// `curated` or `apisguru`.
    pub source: String,
}

fn default_rank() -> u32 {
    10_000
}

#[derive(Debug, Clone, Serialize)]
pub struct CatalogCategory {
    pub id: String,
    pub label: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CatalogTarget {
    pub spec_url: String,
    /// Curated apps know their auth requirements even when the public spec
    /// omits a security scheme. Directory apps leave this unknown until the
    /// document is parsed.
    pub requires_auth: Option<bool>,
    pub credential_header_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CatalogPage {
    pub entries: Vec<CatalogEntry>,
    /// Number of entries matching the query and category.
    pub total: usize,
    /// Number of installable apps in the whole catalog.
    pub catalog_total: usize,
    pub categories: Vec<CatalogCategory>,
    /// `live` when this process fetched the directory, `cache` when it read
    /// the on-disk index.
    pub source: String,
    pub fetched_at: String,
    pub stale: bool,
    /// Set when a refresh failed and the cached index was used instead.
    pub warning: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedCatalog {
    schema: u32,
    fetched_at: String,
    source_url: String,
    entries: Vec<CatalogEntry>,
}

struct LoadedIndex {
    index: Arc<CachedCatalog>,
    source: &'static str,
    stale: bool,
    warning: Option<String>,
}

pub struct CatalogService {
    cache_path: PathBuf,
    list_url: String,
    client: reqwest::Client,
    index: RwLock<Option<Arc<CachedCatalog>>>,
    /// Serializes downloads so a fast typist cannot start several 9 MB
    /// directory fetches at once.
    fetch_lock: tokio::sync::Mutex<()>,
    /// Whether this process fetched the directory itself. An in-memory index
    /// from disk is still a cache; an index fetched this session stays "live"
    /// across searches.
    live: AtomicBool,
}

impl CatalogService {
    /// The catalog used by the desktop app, cached under `~/RAVENBOT/cache`.
    pub fn default_cached() -> Self {
        Self::new(default_cache_path())
    }

    pub fn new(cache_path: PathBuf) -> Self {
        Self::with_url(cache_path, APIS_GURU_LIST_URL)
    }

    /// Explicit URL so tests can point at a local directory fixture.
    pub fn with_url(cache_path: PathBuf, list_url: impl Into<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(45))
            .user_agent("RAVENBOT/1.0 OpenAPI catalog")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            cache_path,
            list_url: list_url.into(),
            client,
            index: RwLock::new(None),
            fetch_lock: tokio::sync::Mutex::new(()),
            live: AtomicBool::new(false),
        }
    }

    /// Search, filter, and paginate the catalog.
    ///
    /// `refresh` forces a network fetch. If the fetch fails and a cached
    /// index exists, the cached index is returned with a warning instead of
    /// turning the whole picker into an error state.
    pub async fn page(
        &self,
        query: Option<&str>,
        category: Option<&str>,
        limit: usize,
        offset: usize,
        refresh: bool,
    ) -> Result<CatalogPage, String> {
        let loaded = self.load(refresh).await?;

        let curated = curated_entries();
        let mut entries: Vec<&CatalogEntry> =
            Vec::with_capacity(curated.len() + loaded.index.entries.len());
        entries.extend(curated.iter());
        entries.extend(
            loaded
                .index
                .entries
                .iter()
                .filter(|entry| !is_curated_provider(&entry.provider)),
        );
        let catalog_total = entries.len();
        let categories = category_counts(entries.iter().copied());

        let terms = split_query(query);
        let category = category
            .map(str::trim)
            .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("all"));

        let mut scored: Vec<(i32, &CatalogEntry)> = entries
            .into_iter()
            .filter_map(|entry| score_entry(entry, &terms, category).map(|score| (score, entry)))
            .collect();

        if terms.is_empty() {
            scored.sort_by(|left, right| {
                left.1
                    .rank
                    .cmp(&right.1.rank)
                    .then_with(|| left.1.name.to_lowercase().cmp(&right.1.name.to_lowercase()))
            });
        } else {
            scored.sort_by(|left, right| {
                right
                    .0
                    .cmp(&left.0)
                    .then_with(|| left.1.rank.cmp(&right.1.rank))
                    .then_with(|| left.1.name.to_lowercase().cmp(&right.1.name.to_lowercase()))
            });
        }

        let total = scored.len();
        let limit = limit.clamp(1, MAX_PAGE_SIZE);
        let offset = offset.min(total);
        let page_entries: Vec<CatalogEntry> = scored
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|(_, entry)| entry.clone())
            .collect();
        let has_more = offset + page_entries.len() < total;

        Ok(CatalogPage {
            entries: page_entries,
            total,
            catalog_total,
            categories,
            source: loaded.source.to_string(),
            fetched_at: loaded.index.fetched_at.clone(),
            stale: loaded.stale,
            warning: loaded.warning,
            has_more,
        })
    }

    /// Resolve a catalog id to its spec URL.
    ///
    /// Curated ids work without a network call. Directory ids are looked up
    /// in the cached index; the renderer also passes the URL it displayed so
    /// an entry remains installable if the cached index was refreshed between
    /// the two calls.
    pub async fn spec_url_for(&self, catalog_id: &str) -> Result<Option<CatalogTarget>, String> {
        if let Some(entry) = curated_entries()
            .into_iter()
            .find(|entry| entry.id == catalog_id)
        {
            return Ok(Some(CatalogTarget {
                credential_header_hint: entry
                    .requires_auth
                    .filter(|required| *required)
                    .map(|_| "Authorization".to_string()),
                requires_auth: entry.requires_auth,
                spec_url: entry.spec_url,
            }));
        }
        if !catalog_id.starts_with("apisguru:") {
            return Ok(None);
        }
        let loaded = self.load(false).await?;
        Ok(loaded
            .index
            .entries
            .iter()
            .find(|entry| entry.id == catalog_id)
            .map(|entry| CatalogTarget {
                spec_url: entry.spec_url.clone(),
                requires_auth: None,
                credential_header_hint: None,
            }))
    }

    async fn load(&self, refresh: bool) -> Result<LoadedIndex, String> {
        let cached = self.cached_index().await;

        if !refresh {
            if let Some(index) = cached {
                return Ok(LoadedIndex {
                    stale: is_stale(&index.fetched_at),
                    index,
                    source: if self.live.load(Ordering::Relaxed) {
                        "live"
                    } else {
                        "cache"
                    },
                    warning: None,
                });
            }
        }

        // One download at a time. A search that arrived while another request
        // was fetching picks up the finished index instead of downloading the
        // directory again.
        let _fetch_guard = self.fetch_lock.lock().await;
        if !refresh {
            if let Some(index) = self.cached_index().await {
                return Ok(LoadedIndex {
                    stale: is_stale(&index.fetched_at),
                    index,
                    source: if self.live.load(Ordering::Relaxed) {
                        "live"
                    } else {
                        "cache"
                    },
                    warning: None,
                });
            }
        }

        match self.fetch_index().await {
            Ok(index) => {
                if let Err(error) = self.write_cache(&index) {
                    tracing::warn!("plugin catalog cache could not be written: {error}");
                }
                self.live.store(true, Ordering::Relaxed);
                *self.index.write().await = Some(index.clone());
                Ok(LoadedIndex {
                    index,
                    source: "live",
                    stale: false,
                    warning: None,
                })
            }
            Err(error) => {
                if let Some(index) = cached {
                    Ok(LoadedIndex {
                        stale: is_stale(&index.fetched_at),
                        index,
                        source: "cache",
                        warning: Some(error),
                    })
                } else {
                    Err(format!(
                        "The OpenAPI directory could not be loaded. Check the network and try again. {error}"
                    ))
                }
            }
        }
    }

    async fn cached_index(&self) -> Option<Arc<CachedCatalog>> {
        if let Some(index) = self.index.read().await.clone() {
            return Some(index);
        }
        let index = self.read_cache()?;
        *self.index.write().await = Some(index.clone());
        Some(index)
    }

    fn read_cache(&self) -> Option<Arc<CachedCatalog>> {
        let data = std::fs::read(&self.cache_path).ok()?;
        let cache: CachedCatalog = serde_json::from_slice(&data).ok()?;
        if cache.schema != CACHE_SCHEMA || cache.entries.is_empty() {
            return None;
        }
        Some(Arc::new(cache))
    }

    fn write_cache(&self, cache: &CachedCatalog) -> Result<(), String> {
        if let Some(parent) = self.cache_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!("Could not create the catalog cache directory: {error}")
            })?;
        }
        let data = serde_json::to_vec(cache)
            .map_err(|error| format!("Could not serialize the catalog cache: {error}"))?;
        let temporary = self.cache_path.with_extension("json.tmp");
        std::fs::write(&temporary, data)
            .map_err(|error| format!("Could not write the catalog cache: {error}"))?;
        if std::fs::rename(&temporary, &self.cache_path).is_err() {
            let _ = std::fs::remove_file(&self.cache_path);
            std::fs::rename(&temporary, &self.cache_path)
                .map_err(|error| format!("Could not replace the catalog cache: {error}"))?;
        }
        Ok(())
    }

    async fn fetch_index(&self) -> Result<Arc<CachedCatalog>, String> {
        let mut response = self
            .client
            .get(&self.list_url)
            .header(reqwest::header::ACCEPT, "application/json, */*;q=0.5")
            .send()
            .await
            .map_err(|error| format!("Could not reach the OpenAPI directory: {error}"))?;

        let status = response.status();
        if !status.is_success() {
            return Err(format!(
                "The OpenAPI directory returned HTTP {}.",
                status.as_u16()
            ));
        }
        if let Some(length) = response.content_length() {
            if length > MAX_LIST_BYTES as u64 {
                return Err(format!(
                    "The OpenAPI directory is larger than the {} MB limit.",
                    MAX_LIST_BYTES / (1024 * 1024)
                ));
            }
        }

        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| format!("Could not read the OpenAPI directory: {error}"))?
        {
            if bytes.len() + chunk.len() > MAX_LIST_BYTES {
                return Err(format!(
                    "The OpenAPI directory is larger than the {} MB limit.",
                    MAX_LIST_BYTES / (1024 * 1024)
                ));
            }
            bytes.extend_from_slice(&chunk);
        }

        let root: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|error| format!("The OpenAPI directory returned invalid JSON: {error}"))?;
        let entries = compact_list(&root)?;

        Ok(Arc::new(CachedCatalog {
            schema: CACHE_SCHEMA,
            fetched_at: Utc::now().to_rfc3339(),
            source_url: self.list_url.clone(),
            entries,
        }))
    }
}

pub fn default_cache_path() -> PathBuf {
    ravenbot_core::cache_dir()
        .join("plugins")
        .join("apisguru-index.json")
}

fn curated_entries() -> Vec<CatalogEntry> {
    CURATED
        .iter()
        .enumerate()
        .map(|(rank, definition)| CatalogEntry {
            id: definition.id.to_string(),
            name: definition.name.to_string(),
            description: definition.description.to_string(),
            spec_url: definition.spec_url.to_string(),
            category: definition.category.to_string(),
            categories: definition
                .categories
                .iter()
                .map(|category| category.to_string())
                .collect(),
            provider: definition.provider.to_string(),
            logo: String::new(),
            version: definition.version.to_string(),
            openapi_version: definition.openapi_version.to_string(),
            requires_auth: Some(definition.requires_auth),
            featured: true,
            rank: rank as u32,
            source: "curated".to_string(),
        })
        .collect()
}

/// Known auth for a curated spec URL, even if the app was installed before
/// the curated override existed. Read-time only; nothing is rewritten.
pub fn curated_auth_for_source(source_url: Option<&str>) -> Option<(bool, &'static str)> {
    let source_url = source_url?.trim().trim_end_matches('/');
    CURATED
        .iter()
        .find(|entry| {
            entry
                .spec_url
                .trim_end_matches('/')
                .eq_ignore_ascii_case(source_url)
        })
        .map(|entry| (entry.requires_auth, "Authorization"))
}

fn is_curated_provider(provider: &str) -> bool {
    CURATED
        .iter()
        .any(|definition| definition.provider.eq_ignore_ascii_case(provider))
}

fn featured_rank(provider: &str) -> Option<u32> {
    FEATURED_PROVIDERS
        .iter()
        .position(|featured| featured.eq_ignore_ascii_case(provider))
        .map(|index| 10 + index as u32)
}

fn compact_list(root: &serde_json::Value) -> Result<Vec<CatalogEntry>, String> {
    let providers = root
        .as_object()
        .ok_or_else(|| "The OpenAPI directory is not a JSON object.".to_string())?;
    let mut entries = Vec::with_capacity(providers.len());
    for (provider, value) in providers {
        if let Some(entry) = compact_provider(provider, value) {
            entries.push(entry);
        }
    }
    if entries.is_empty() {
        return Err("The OpenAPI directory contains no usable APIs.".to_string());
    }
    entries.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.provider.cmp(&right.provider))
    });
    Ok(entries)
}

fn compact_provider(provider: &str, value: &serde_json::Value) -> Option<CatalogEntry> {
    let versions = value.get("versions")?.as_object()?;
    let preferred = value.get("preferred").and_then(|value| value.as_str());
    let (version, version_value) = preferred
        .and_then(|preferred| versions.get_key_value(preferred))
        .or_else(|| versions.iter().next())?;
    let info = version_value.get("info")?;
    let spec_url = version_value
        .get("swaggerUrl")
        .and_then(|value| value.as_str())
        .or_else(|| version_value.get("link").and_then(|value| value.as_str()))?;
    if !(spec_url.starts_with("https://") || spec_url.starts_with("http://")) {
        return None;
    }

    let title = info
        .get("title")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or(provider);
    let description = info
        .get("description")
        .and_then(|value| value.as_str())
        .map(strip_markup)
        .filter(|description| !description.is_empty())
        .unwrap_or_else(|| format!("Public OpenAPI document for {title}."));

    let mut categories: Vec<String> = info
        .get("x-apisguru-categories")
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str())
                .map(|value| value.trim().to_lowercase().replace(' ', "_"))
                .filter(|value| !value.is_empty())
                .collect()
        })
        .unwrap_or_default();
    categories.sort();
    categories.dedup();
    if categories.is_empty() {
        categories.push("other".to_string());
    }

    let logo = info
        .get("x-logo")
        .and_then(|logo| {
            logo.get("url")
                .and_then(|value| value.as_str())
                .or_else(|| logo.as_str())
        })
        .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
        .unwrap_or("")
        .to_string();

    let rank = featured_rank(provider).unwrap_or_else(default_rank);
    Some(CatalogEntry {
        id: format!("apisguru:{provider}"),
        name: truncate_text(title, 90),
        description: truncate_text(&description, 360),
        spec_url: spec_url.to_string(),
        category: categories[0].clone(),
        categories,
        provider: provider.to_string(),
        logo,
        version: version.to_string(),
        openapi_version: version_value
            .get("openapiVer")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown")
            .to_string(),
        requires_auth: None,
        featured: rank < 10_000,
        rank,
        source: "apisguru".to_string(),
    })
}

fn split_query(query: Option<&str>) -> Vec<String> {
    let mut seen = HashSet::new();
    query
        .unwrap_or("")
        .split_whitespace()
        .map(|term| term.to_lowercase())
        .filter(|term| !term.is_empty() && seen.insert(term.clone()))
        .collect()
}

/// Return a relevance score when the entry matches every query term and the
/// selected category. Non-matches return `None`.
fn score_entry(entry: &CatalogEntry, terms: &[String], category: Option<&str>) -> Option<i32> {
    if let Some(category) = category {
        let matches = entry.category.eq_ignore_ascii_case(category)
            || entry
                .categories
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(category));
        if !matches {
            return None;
        }
    }
    if terms.is_empty() {
        return Some(0);
    }

    let name = entry.name.to_lowercase();
    let provider = entry.provider.to_lowercase();
    let description = entry.description.to_lowercase();
    let id = entry.id.to_lowercase();
    let categories = entry.categories.join(" ").to_lowercase();
    let name_words: Vec<&str> = name
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();

    let mut score = 0;
    for term in terms {
        let mut term_score = 0;
        if name == *term {
            term_score = 140;
        } else if name.starts_with(term.as_str()) {
            term_score = 110;
        } else if name_words
            .iter()
            .any(|word| word.starts_with(term.as_str()))
        {
            term_score = 85;
        } else if name.contains(term.as_str()) {
            term_score = 60;
        }
        if provider.contains(term.as_str()) {
            term_score = term_score.max(35);
        }
        if categories.contains(term.as_str()) {
            term_score = term_score.max(25);
        }
        if description.contains(term.as_str()) {
            term_score = term_score.max(12);
        }
        if id.contains(term.as_str()) {
            term_score = term_score.max(8);
        }
        if term_score == 0 {
            return None;
        }
        score += term_score;
    }

    // A small boost for curated and featured entries breaks ties without
    // letting an unrelated featured app outrank a strong title match.
    let boost = if entry.source == "curated" {
        30
    } else if entry.featured {
        12
    } else {
        0
    };
    Some(score + boost)
}

fn category_counts<'a>(entries: impl Iterator<Item = &'a CatalogEntry>) -> Vec<CatalogCategory> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for entry in entries {
        let mut seen = HashSet::new();
        for category in std::iter::once(&entry.category).chain(entry.categories.iter()) {
            let id = category.trim().to_lowercase();
            if id.is_empty() || !seen.insert(id.clone()) {
                continue;
            }
            *counts.entry(id).or_default() += 1;
        }
    }
    let mut categories: Vec<CatalogCategory> = counts
        .into_iter()
        .map(|(id, count)| CatalogCategory {
            label: humanize_category(&id),
            id,
            count,
        })
        .collect();
    categories.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.label.cmp(&right.label))
    });
    categories
}

fn humanize_category(id: &str) -> String {
    match id {
        "open_data" => "Open data".to_string(),
        "developer_tools" => "Developer tools".to_string(),
        "machine_learning" => "Machine learning".to_string(),
        "customer_relation" => "Customer relations".to_string(),
        "ecommerce" => "E-commerce".to_string(),
        "iot" => "IoT".to_string(),
        "dev" => "Developer tools".to_string(),
        "chat" => "Chat".to_string(),
        "finance" => "Finance".to_string(),
        "demo" => "Demo".to_string(),
        other => other
            .split('_')
            .map(|word| {
                let mut characters = word.chars();
                let first = characters
                    .next()
                    .map(|character| character.to_uppercase().collect::<String>())
                    .unwrap_or_default();
                format!("{first}{}", characters.as_str())
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn strip_markup(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut in_tag = false;
    for character in value.chars() {
        match character {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                output.push(' ');
            }
            _ if !in_tag => output.push(character),
            _ => {}
        }
    }
    let decoded = output
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let mut output: String = trimmed.chars().take(max_chars.saturating_sub(1)).collect();
    output.push('…');
    output
}

fn is_stale(fetched_at: &str) -> bool {
    match DateTime::parse_from_rfc3339(fetched_at) {
        Ok(fetched_at) => {
            Utc::now()
                .signed_duration_since(fetched_at.with_timezone(&Utc))
                .num_seconds()
                > CACHE_TTL_SECONDS
        }
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn test_entry(
        id: &str,
        name: &str,
        provider: &str,
        description: &str,
        categories: &[&str],
        rank: u32,
    ) -> CatalogEntry {
        CatalogEntry {
            id: id.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            spec_url: format!("https://api.apis.guru/v2/specs/{provider}/1.0/openapi.json"),
            category: categories.first().copied().unwrap_or("other").to_string(),
            categories: categories.iter().map(|value| value.to_string()).collect(),
            provider: provider.to_string(),
            logo: String::new(),
            version: "1.0".to_string(),
            openapi_version: "3.0.0".to_string(),
            requires_auth: None,
            featured: rank < 10_000,
            rank,
            source: "apisguru".to_string(),
        }
    }

    fn temporary_cache() -> PathBuf {
        std::env::temp_dir().join(format!("ravenbot-catalog-{}.json", uuid::Uuid::new_v4()))
    }

    fn directory_fixture() -> serde_json::Value {
        serde_json::json!({
            "notion.com": {
                "preferred": "1.0",
                "versions": {
                    "1.0": {
                        "openapiVer": "3.0.3",
                        "swaggerUrl": "https://api.apis.guru/v2/specs/notion.com/1.0/openapi.json",
                        "info": {
                            "title": "Notion API",
                            "description": "<p>Read and write pages, databases, and comments.</p>",
                            "x-logo": {"url": "https://api.apis.guru/v2/cache/logo/notion.png"},
                            "x-apisguru-categories": ["collaboration"]
                        }
                    }
                }
            },
            "legacy.test": {
                "preferred": "2.0",
                "versions": {
                    "2.0": {
                        "openapiVer": "2.0",
                        "swaggerUrl": "https://api.apis.guru/v2/specs/legacy.test/2.0/swagger.json",
                        "info": {
                            "title": "Legacy API",
                            "description": "",
                            "x-apisguru-categories": []
                        }
                    }
                }
            }
        })
    }

    #[test]
    fn compacts_the_directory_into_installable_entries() {
        let entries = compact_list(&directory_fixture()).expect("entries");
        let notion = entries
            .iter()
            .find(|entry| entry.provider == "notion.com")
            .expect("notion");
        assert_eq!(notion.id, "apisguru:notion.com");
        assert_eq!(notion.name, "Notion API");
        assert_eq!(
            notion.description,
            "Read and write pages, databases, and comments."
        );
        assert_eq!(notion.category, "collaboration");
        assert_eq!(notion.openapi_version, "3.0.3");
        assert_eq!(notion.requires_auth, None);
        assert!(notion.featured);
        assert_eq!(
            notion.spec_url,
            "https://api.apis.guru/v2/specs/notion.com/1.0/openapi.json"
        );

        let legacy = entries
            .iter()
            .find(|entry| entry.provider == "legacy.test")
            .expect("legacy");
        assert_eq!(legacy.category, "other");
        assert_eq!(
            legacy.description,
            "Public OpenAPI document for Legacy API."
        );
        assert!(!legacy.featured);
    }

    #[test]
    fn search_ranks_title_matches_before_description_matches() {
        let entries = [
            test_entry(
                "apisguru:mail.test",
                "Mail Delivery",
                "mail.test",
                "A delivery logistics API",
                &["transport"],
                10_000,
            ),
            test_entry(
                "apisguru:gmail.test",
                "Gmail API",
                "gmail.test",
                "Read mail",
                &["email"],
                20,
            ),
        ];
        let terms = split_query(Some("gmail"));
        let mut matches: Vec<(i32, &CatalogEntry)> = entries
            .iter()
            .filter_map(|entry| score_entry(entry, &terms, None).map(|score| (score, entry)))
            .collect();
        matches.sort_by_key(|right| std::cmp::Reverse(right.0));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].1.name, "Gmail API");
    }

    #[test]
    fn category_filter_and_counts_include_curated_entries() {
        let entries = [
            test_entry(
                "apisguru:pay.test",
                "Payments",
                "pay.test",
                "Take payments",
                &["payment", "finance"],
                10_000,
            ),
            test_entry(
                "apisguru:chat.test",
                "Chat",
                "chat.test",
                "Send messages",
                &["messaging"],
                10_000,
            ),
        ];
        let terms = split_query(Some("payments"));
        let filtered: Vec<&CatalogEntry> = entries
            .iter()
            .filter(|entry| score_entry(entry, &terms, Some("payment")).is_some())
            .collect();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "Payments");

        let counts = category_counts(entries.iter());
        assert_eq!(
            counts
                .iter()
                .find(|category| category.id == "payment")
                .expect("payment")
                .count,
            1
        );
        assert!(counts.iter().any(|category| category.id == "finance"));
    }

    #[tokio::test]
    async fn cached_catalog_works_without_network() {
        let path = temporary_cache();
        let cache = CachedCatalog {
            schema: CACHE_SCHEMA,
            fetched_at: Utc::now().to_rfc3339(),
            source_url: APIS_GURU_LIST_URL.to_string(),
            entries: vec![test_entry(
                "apisguru:gmail.test",
                "Gmail API",
                "gmail.test",
                "Read and send mail",
                &["email"],
                10_000,
            )],
        };
        let service = CatalogService::with_url(path.clone(), "http://127.0.0.1:1/list.json");
        service.write_cache(&cache).expect("write cache");

        let page = service
            .page(None, None, 20, 0, false)
            .await
            .expect("cached page");
        assert_eq!(page.source, "cache");
        assert!(page
            .entries
            .iter()
            .any(|entry| entry.id == "apisguru:gmail.test"));
        assert_eq!(page.catalog_total, CURATED.len() + 1);

        let refreshed = service
            .page(None, None, 20, 0, true)
            .await
            .expect("stale fallback");
        assert!(refreshed.warning.is_some());
        assert!(refreshed
            .entries
            .iter()
            .any(|entry| entry.id == "apisguru:gmail.test"));

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn fetches_and_caches_a_live_directory() {
        let body = directory_fixture().to_string();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut buffer = [0u8; 2048];
            let _ = socket.read(&mut buffer).await;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes()).await;
        });

        let path = temporary_cache();
        let service = CatalogService::with_url(path.clone(), format!("http://{address}/list.json"));
        let page = service
            .page(None, None, 20, 0, false)
            .await
            .expect("live page");
        assert_eq!(page.source, "live");
        assert_eq!(page.catalog_total, CURATED.len() + 2);
        assert!(path.exists(), "the live index should be cached");

        let second = service
            .page(Some("notion"), None, 20, 0, false)
            .await
            .expect("second page");
        assert_eq!(
            second.source, "live",
            "an index fetched this session stays live"
        );
        assert!(service.spec_url_for("notion.com").await.unwrap().is_none());
        assert!(service
            .spec_url_for("apisguru:notion.com")
            .await
            .unwrap()
            .is_some());

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn concurrent_searches_share_one_directory_download() {
        use std::sync::atomic::AtomicUsize;
        use std::sync::Arc as StdArc;

        let body = directory_fixture().to_string();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let requests = StdArc::new(AtomicUsize::new(0));
        let counter = requests.clone();
        tokio::spawn(async move {
            loop {
                let Ok(Ok((mut socket, _))) =
                    tokio::time::timeout(std::time::Duration::from_millis(500), listener.accept())
                        .await
                else {
                    break;
                };
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let mut buffer = [0u8; 2048];
                let _ = socket.read(&mut buffer).await;
                tokio::time::sleep(std::time::Duration::from_millis(40)).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(response.as_bytes()).await;
            }
        });

        let path = temporary_cache();
        let service = StdArc::new(CatalogService::with_url(
            path.clone(),
            format!("http://{address}/list.json"),
        ));
        let first = service.clone();
        let second = service.clone();
        let (left, right) = tokio::join!(
            first.page(Some("notion"), None, 20, 0, false),
            second.page(Some("legacy"), None, 20, 0, false)
        );
        assert!(left.is_ok(), "{left:?}");
        assert!(right.is_ok(), "{right:?}");
        assert_eq!(
            requests.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "concurrent searches must not download the directory twice"
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn compacts_a_real_directory_fixture_when_requested() {
        let Ok(path) = std::env::var("RAVENBOT_CATALOG_FIXTURE") else {
            return;
        };
        let text = std::fs::read_to_string(path).expect("fixture file");
        let root: serde_json::Value = serde_json::from_str(&text).expect("fixture json");
        let entries = compact_list(&root).expect("compacted entries");
        assert!(entries.len() > 2_000, "only {} entries", entries.len());
        assert!(entries.iter().any(|entry| entry.provider == "github.com"));
        assert!(entries.iter().any(|entry| entry.provider == "notion.com"));
        let bytes = serde_json::to_vec(&entries).expect("compact json");
        assert!(
            bytes.len() < 8 * 1024 * 1024,
            "compact index is unexpectedly large: {} bytes",
            bytes.len()
        );
        println!(
            "compacted {} providers into {} bytes",
            entries.len(),
            bytes.len()
        );
    }

    #[tokio::test]
    #[ignore = "downloads the live APIs.guru directory"]
    async fn live_apisguru_directory_is_searchable() {
        let path = temporary_cache();
        let service = CatalogService::new(path.clone());
        let page = service
            .page(Some("stripe"), None, 20, 0, false)
            .await
            .expect("live directory");
        assert!(
            page.catalog_total > 2_000,
            "catalog total {}",
            page.catalog_total
        );
        assert!(
            page.entries
                .iter()
                .any(|entry| entry.name.to_lowercase().contains("stripe")),
            "stripe not found in {:?}",
            page.entries
                .iter()
                .map(|entry| &entry.name)
                .collect::<Vec<_>>()
        );

        let cached = service
            .page(Some("gmail"), None, 20, 0, false)
            .await
            .expect("cached directory");
        assert_eq!(cached.source, "live");
        assert!(cached
            .entries
            .iter()
            .any(|entry| entry.provider == "googleapis.com:gmail"));
        let metadata = std::fs::metadata(&path).expect("cache file");
        assert!(metadata.len() < 8 * 1024 * 1024);
        println!(
            "live directory: {} apps, {} byte cache",
            page.catalog_total,
            metadata.len()
        );
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    #[ignore = "downloads a live OpenAPI spec"]
    async fn live_directory_spec_parses_into_operations() {
        let text = reqwest::Client::new()
            .get("https://api.apis.guru/v2/specs/notion.com/1.0.0/openapi.json")
            .send()
            .await
            .expect("request")
            .text()
            .await
            .expect("body");
        let operations = crate::openapi::parse_openapi_spec(&text).expect("parse");
        assert!(operations.len() > 5, "operations: {}", operations.len());
        let metadata = crate::openapi::spec_metadata(&text).expect("metadata");
        assert_eq!(metadata.title, "Notion API");
        // APIs.guru does not always copy the security scheme into the spec.
        // The catalog labels those entries "auth checked on install"; once
        // installed, the user can still add a credential even when the
        // document does not declare one.
        assert!(!metadata.requires_auth);
        assert_eq!(metadata.credential_header, None);
    }

    #[test]
    fn curated_auth_is_known_for_existing_installs() {
        let stripe = curated_auth_for_source(Some(
            "https://raw.githubusercontent.com/stripe/openapi/master/openapi/spec3.json",
        ))
        .expect("stripe auth");
        assert_eq!(stripe, (true, "Authorization"));

        let petstore =
            curated_auth_for_source(Some("https://petstore3.swagger.io/api/v3/openapi.json"))
                .expect("petstore auth");
        assert_eq!(petstore, (false, "Authorization"));

        assert!(curated_auth_for_source(Some("https://example.test/openapi.json")).is_none());
        assert!(curated_auth_for_source(None).is_none());
    }

    #[tokio::test]
    async fn curated_ids_resolve_without_network() {
        let service = CatalogService::with_url(temporary_cache(), "http://127.0.0.1:1/list.json");
        let target = service
            .spec_url_for("stripe")
            .await
            .expect("curated")
            .expect("stripe target");
        assert_eq!(
            target.spec_url,
            "https://raw.githubusercontent.com/stripe/openapi/master/openapi/spec3.json"
        );
        assert_eq!(target.requires_auth, Some(true));
        assert_eq!(
            target.credential_header_hint.as_deref(),
            Some("Authorization")
        );
        let petstore = service
            .spec_url_for("petstore")
            .await
            .expect("curated")
            .expect("petstore target");
        assert_eq!(petstore.requires_auth, Some(false));
        assert_eq!(petstore.credential_header_hint, None);
        assert!(service.spec_url_for("unknown").await.unwrap().is_none());
    }
}
