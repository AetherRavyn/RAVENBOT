//! Awesome Agent Skills — 1497+ curated skills from VoltAgent/awesome-agent-skills
//! Every skill becomes a native RAVENBOT tool via this dynamic registry.
//! Source: https://github.com/VoltAgent/awesome-agent-skills

use crate::traits::{Skill, SkillContext, SkillError, SkillResult};
use async_trait::async_trait;
use ravenbot_core::Permission;
use std::time::Duration;
use tokio::sync::OnceCell;

#[derive(Debug, Clone)]
pub struct AwesomeSkillMeta {
    pub id: String,   // e.g. "anthropics/docx"
    pub name: String, // e.g. "docx"
    pub description: String,
    pub category: String, // e.g. "Official Claude Skills"
    pub url: String,      // e.g. https://officialskills.sh/anthropics/skills/docx
}

pub struct AwesomeSkill {
    pub meta: AwesomeSkillMeta,
}

const README_URL: &str =
    "https://raw.githubusercontent.com/VoltAgent/awesome-agent-skills/main/README.md";
const MAX_INSTRUCTION_CHARS: usize = 20_000;
static README_CACHE: OnceCell<String> = OnceCell::const_new();

fn clip_instructions(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= MAX_INSTRUCTION_CHARS {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(MAX_INSTRUCTION_CHARS).collect();
    out.push_str("\n\n[Skill instructions truncated at 20,000 characters. Fetch the source URL for the remainder.]");
    out
}

/// Turn a GitHub tree/blob URL into the raw SKILL.md candidates for that path.
///
/// The awesome catalog links to landing pages, and landing pages link to
/// GitHub. The instructions themselves live next to the skill, not in the
/// landing page's HTML, so the fetch path has to walk one more step.
fn github_raw_candidates(url: &str) -> Vec<String> {
    let trimmed = url.trim().trim_end_matches('/');
    if trimmed.starts_with("https://raw.githubusercontent.com/") {
        return vec![trimmed.to_string()];
    }
    let Some(rest) = trimmed.strip_prefix("https://github.com/") else {
        return Vec::new();
    };
    let segments: Vec<&str> = rest.split('/').filter(|part| !part.is_empty()).collect();
    if segments.len() < 2 {
        return Vec::new();
    }
    let owner = segments[0];
    let repo = segments[1].trim_end_matches(".git");
    let mut out = Vec::new();
    if segments.len() >= 5
        && (segments[2] == "tree" || segments[2] == "blob" || segments[2] == "raw")
    {
        let branch = segments[3];
        let path = segments[4..].join("/");
        let base = format!("https://raw.githubusercontent.com/{owner}/{repo}/{branch}/{path}");
        if path.ends_with(".md") {
            out.push(base.clone());
        } else {
            out.push(format!("{base}/SKILL.md"));
            out.push(format!("{base}/skill.md"));
            out.push(format!("{base}/README.md"));
        }
        // The catalog sometimes records a branch that has since been renamed.
        if branch != "main" || path.ends_with(".md") {
            // Keep the explicit path; the caller will try the fallbacks below.
        }
    }
    for branch in ["main", "master"] {
        out.push(format!(
            "https://raw.githubusercontent.com/{owner}/{repo}/{branch}/SKILL.md"
        ));
        out.push(format!(
            "https://raw.githubusercontent.com/{owner}/{repo}/{branch}/README.md"
        ));
    }
    // Preserve priority: a directory's own SKILL.md must win over its README.
    let mut seen = std::collections::HashSet::new();
    out.retain(|candidate| seen.insert(candidate.clone()));
    out
}

/// The repository clone URL for a GitHub tree/blob URL.
fn github_clone_url(url: &str) -> Option<String> {
    let rest = url.trim().strip_prefix("https://github.com/")?;
    let segments: Vec<&str> = rest.split('/').filter(|part| !part.is_empty()).collect();
    if segments.len() < 2 {
        return None;
    }
    Some(format!(
        "https://github.com/{}/{}.git",
        segments[0],
        segments[1].trim_end_matches(".git")
    ))
}

/// Pull the first GitHub source link out of an officialskills.sh page.
fn github_link_from_html(html: &str) -> Option<String> {
    let mut fallback = None;
    let mut rest = html;
    while let Some(start) = rest.find("https://github.com/") {
        let candidate = &rest[start..];
        let end = candidate
            .find(|c: char| {
                c == '"' || c == '\'' || c == '<' || c == '>' || c.is_whitespace() || c == ')'
            })
            .unwrap_or(candidate.len());
        let url = candidate[..end]
            .trim_end_matches(['.', ',', ';'])
            .to_string();
        if url.contains("/tree/") || url.contains("/blob/") {
            return Some(url);
        }
        if fallback.is_none() {
            fallback = Some(url);
        }
        rest = &candidate[end..];
    }
    fallback
}

/// Resolve `**[id](url)**` from a VoltAgent README line.
fn readme_url_for_id(readme: &str, id: &str) -> Option<String> {
    let needle = format!("[{}](", id.to_ascii_lowercase());
    for line in readme.lines() {
        let lower = line.to_ascii_lowercase();
        let Some(start) = lower.find(&needle) else {
            continue;
        };
        let after = &line[start + needle.len()..];
        let end = after.find(')')?;
        let url = after[..end].trim();
        if url.starts_with("http://") || url.starts_with("https://") {
            return Some(url.to_string());
        }
    }
    None
}

async fn fetch_text(url: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(25))
        .build()
        .map_err(|e| format!("Could not create HTTP client: {e}"))?;
    let response = client
        .get(url)
        .header(
            "User-Agent",
            "RAVENBOT/0.2 (+https://github.com/dmtrKovalenko/fframes)",
        )
        .send()
        .await
        .map_err(|e| format!("Could not fetch {url}: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("{url} returned HTTP {}", response.status()));
    }
    response
        .text()
        .await
        .map_err(|e| format!("Could not read {url}: {e}"))
}

async fn readme() -> Result<&'static str, String> {
    README_CACHE
        .get_or_try_init(|| async { fetch_text(README_URL).await })
        .await
        .map(|text| text.as_str())
}

/// Fetch the actual SKILL.md behind a catalog URL.
async fn resolve_instructions(url: &str) -> Result<(String, String), String> {
    resolve_instructions_inner(url, Some(MAX_INSTRUCTION_CHARS)).await
}

/// The library keeps the whole document; the prompt path is the only one that
/// needs a cap, because it spends context rather than disk.
async fn resolve_instructions_full(url: &str) -> Result<(String, String), String> {
    resolve_instructions_inner(url, None).await
}

async fn resolve_instructions_inner(
    url: &str,
    limit: Option<usize>,
) -> Result<(String, String), String> {
    let mut target = url.trim().to_string();
    if target.starts_with("https://officialskills.sh/") {
        let html = fetch_text(&target).await?;
        target = github_link_from_html(&html).ok_or_else(|| {
            format!("The catalog page {target} did not link to a GitHub source for the skill.")
        })?;
    }
    let source = target.clone();
    let finish = |text: &str| match limit {
        Some(_) => clip_instructions(text),
        None => text.trim().to_string(),
    };
    let candidates = github_raw_candidates(&target);
    if candidates.is_empty() {
        // A direct Markdown URL (or a non-GitHub host) is still worth trying.
        if target.ends_with(".md") {
            return fetch_text(&target)
                .await
                .map(|text| (finish(&text), target.clone()));
        }
        return Err(format!(
            "The skill URL {target} is not a GitHub tree/blob or Markdown file."
        ));
    }
    let mut last_error = String::new();
    for candidate in candidates {
        match fetch_text(&candidate).await {
            Ok(text) if !text.trim().is_empty() && !text.trim_start().starts_with("404:") => {
                return Ok((finish(&text), source.clone()));
            }
            Ok(_) => {}
            Err(error) => last_error = error,
        }
    }
    Err(format!(
        "No SKILL.md or README.md could be fetched for {target}. {last_error}"
    ))
}

/// A catalog skill with its complete `SKILL.md`, ready to become a versioned
/// local skill. The prompt path clips; this does not.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CatalogSkillDocument {
    pub id: String,
    pub name: String,
    pub description: String,
    pub url: String,
    pub source: String,
    pub content: String,
}

/// A catalog entry shown in the marketplace browser.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CatalogSkillSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub url: String,
    pub category: String,
}

/// Resolve and fetch one catalog skill. `Ok(None)` means the id is not in the
/// live index; an `Err` means the catalog/network failed.
pub async fn fetch_catalog_skill(sid: &str) -> Result<Option<CatalogSkillDocument>, String> {
    let Some(meta) = resolve_meta(sid).await? else {
        return Ok(None);
    };
    let (content, source) = resolve_instructions_full(&meta.url).await?;
    Ok(Some(CatalogSkillDocument {
        id: meta.id,
        name: meta.name,
        description: meta.description,
        url: meta.url,
        source,
        content,
    }))
}

/// Search the live catalog. The README is cached after the first fetch, so a
/// search field can call this repeatedly without re-downloading it.
pub async fn search_catalog(query: &str, limit: usize) -> Result<Vec<CatalogSkillSummary>, String> {
    let text = readme().await?;
    Ok(parse_catalog_readme(text, query, limit))
}

/// Parse the VoltAgent README's `**[owner/name](url)**` lines.
fn parse_catalog_readme(readme: &str, query: &str, limit: usize) -> Vec<CatalogSkillSummary> {
    let needle = query.trim().to_lowercase();
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut category = String::new();
    for line in readme.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            category = trimmed.trim_start_matches('#').trim().to_string();
        }
        let mut rest = line;
        while let Some(close) = rest.find("](") {
            let Some(open) = rest[..close].rfind('[') else {
                rest = &rest[close + 2..];
                continue;
            };
            let id = rest[open + 1..close].trim();
            let after = &rest[close + 2..];
            let Some(end) = after.find(')') else { break };
            let url = after[..end].trim();
            rest = &after[end + 1..];
            let source_like = url.starts_with("https://github.com/")
                || url.starts_with("https://raw.githubusercontent.com/")
                || url.starts_with("https://officialskills.sh/");
            if id.is_empty()
                || id.contains(' ')
                || !id.contains('/')
                || !source_like
                || !seen.insert(id.to_ascii_lowercase())
            {
                continue;
            }
            let description = rest
                .trim()
                .trim_start_matches('*')
                .trim()
                .trim_start_matches(['-', '–', '—', ':', ' '])
                .split(" — ")
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            let haystack = format!("{id} {description}").to_lowercase();
            if !needle.is_empty() && !haystack.contains(&needle) {
                continue;
            }
            let name = id
                .rsplit('/')
                .next()
                .unwrap_or(id)
                .replace(['-', '_'], " ")
                .trim()
                .to_string();
            out.push(CatalogSkillSummary {
                id: id.to_string(),
                name,
                description,
                url: url.to_string(),
                category: category.clone(),
            });
            if out.len() >= limit.clamp(1, 100) {
                return out;
            }
        }
    }
    out
}

impl AwesomeSkill {
    pub fn new(meta: AwesomeSkillMeta) -> Self {
        Self { meta }
    }
}

#[async_trait]
impl Skill for AwesomeSkill {
    fn id(&self) -> &str {
        &self.meta.id
    }
    fn name(&self) -> &str {
        &self.meta.name
    }
    fn description(&self) -> &str {
        &self.meta.description
    }
    fn version(&self) -> &str {
        "1.0.0-awesome"
    }
    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::Network {
            domains: vec!["*".into()],
        }]
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type":"object","properties":{
                "task":{"type":"string","description":"Specific task for this skill"},
                "context":{"type":"string","description":"Extra context"}
            }
        })
    }
    async fn execute(
        &self,
        _ctx: &SkillContext,
        args: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        let task = args.get("task").and_then(|v| v.as_str()).unwrap_or("");
        let (instructions, source) = match resolve_instructions(&self.meta.url).await {
            Ok(value) => value,
            Err(error) => {
                return Ok(SkillResult::failure(format!(
                    "Skill '{}' could not be loaded, so its knowledge was NOT applied. {error} Use web_search for '{}' instead, or fetch the source URL directly.",
                    self.meta.id, task
                )));
            }
        };
        Ok(SkillResult::success(serde_json::json!({
            "skill": self.meta.id,
            "category": self.meta.category,
            "url": self.meta.url,
            "source": source,
            "clone_url": github_clone_url(&source),
            "task": task,
            "content_chars": instructions.chars().count(),
            "instructions": instructions,
            "note": "These are the skill's authoritative workflow instructions. Follow them for the task above rather than summarizing them. If they reference scripts/, references/, or assets/ that are not present in the workspace, clone `clone_url` into the workspace and run the skill from its directory."
        })))
    }
}

/// Starter catalog of flagship skills.
///
/// The full 1,497+ catalog is resolved live from the VoltAgent README by
/// `awesome_fetch`; shipping every entry as a static row would bloat the binary
/// and go stale the day after it was generated.
pub fn catalog() -> Vec<AwesomeSkillMeta> {
    let mut v = Vec::new();
    let mut add = |id: &str, name: &str, desc: &str, cat: &str, url: &str| {
        v.push(AwesomeSkillMeta {
            id: id.to_string(),
            name: name.to_string(),
            description: desc.to_string(),
            category: cat.to_string(),
            url: url.to_string(),
        });
    };
    // Official Claude (17)
    add(
        "anthropics/docx",
        "docx",
        "Create, edit, analyze Word documents",
        "Official Claude",
        "https://officialskills.sh/anthropics/skills/docx",
    );
    add(
        "anthropics/pptx",
        "pptx",
        "Create PowerPoint presentations",
        "Official Claude",
        "https://officialskills.sh/anthropics/skills/pptx",
    );
    add(
        "anthropics/xlsx",
        "xlsx",
        "Create Excel spreadsheets",
        "Official Claude",
        "https://officialskills.sh/anthropics/skills/xlsx",
    );
    add(
        "anthropics/pdf",
        "pdf",
        "Extract text, create PDFs",
        "Official Claude",
        "https://officialskills.sh/anthropics/skills/pdf",
    );
    add(
        "anthropics/frontend-design",
        "frontend-design",
        "Frontend design and UI/UX",
        "Official Claude",
        "https://officialskills.sh/anthropics/skills/frontend-design",
    );
    add(
        "anthropics/mcp-builder",
        "mcp-builder",
        "Create MCP servers",
        "Official Claude",
        "https://officialskills.sh/anthropics/skills/mcp-builder",
    );
    add(
        "anthropics/webapp-testing",
        "webapp-testing",
        "Test web apps with Playwright",
        "Official Claude",
        "https://officialskills.sh/anthropics/skills/webapp-testing",
    );
    add(
        "anthropics/skill-creator",
        "skill-creator",
        "Guide for creating skills",
        "Official Claude",
        "https://officialskills.sh/anthropics/skills/skill-creator",
    );
    // Vercel / Cloudflare / Stripe etc.
    add(
        "vercel/vercel",
        "vercel",
        "Vercel deployment & Next.js",
        "Vercel",
        "https://officialskills.sh/vercel/skills/vercel",
    );
    add(
        "stripe/stripe",
        "stripe",
        "Stripe best practices",
        "Stripe",
        "https://officialskills.sh/stripe/skills/stripe-best-practices",
    );
    add(
        "cloudflare/workers",
        "cloudflare-workers",
        "Cloudflare Workers",
        "Cloudflare",
        "https://officialskills.sh/cloudflare/skills/workers",
    );
    add(
        "supabase/postgres",
        "supabase-postgres",
        "PostgreSQL best practices",
        "Supabase",
        "https://officialskills.sh/supabase/skills/postgres-best-practices",
    );
    add(
        "sentry/sentry",
        "sentry",
        "Sentry error tracking",
        "Sentry",
        "https://officialskills.sh/sentry/skills/sentry",
    );
    add(
        "figma/figma",
        "figma",
        "Figma design",
        "Figma",
        "https://officialskills.sh/figma/skills/figma",
    );
    add(
        "expo/expo",
        "expo",
        "Expo React Native",
        "Expo",
        "https://officialskills.sh/expo/skills/expo",
    );
    add(
        "huggingface/huggingface",
        "huggingface",
        "Hugging Face models",
        "Hugging Face",
        "https://officialskills.sh/huggingface/skills/huggingface",
    );
    add(
        "notion/notion",
        "notion",
        "Notion API",
        "Notion",
        "https://officialskills.sh/notion/skills/notion",
    );
    add(
        "browserbase/browserbase",
        "browserbase",
        "Browser automation",
        "Browserbase",
        "https://officialskills.sh/browserbase/skills/browserbase",
    );
    add(
        "firebase/firebase",
        "firebase",
        "Firebase",
        "Firebase",
        "https://officialskills.sh/firebase/skills/firebase",
    );
    add(
        "mongodb/mongodb",
        "mongodb",
        "MongoDB",
        "MongoDB",
        "https://officialskills.sh/mongodb/skills/mongodb",
    );
    add(
        "redis/redis",
        "redis",
        "Redis",
        "Redis",
        "https://officialskills.sh/redis/skills/redis",
    );
    add(
        "nvidia/nvidia",
        "nvidia",
        "NVIDIA AI",
        "NVIDIA",
        "https://officialskills.sh/nvidia/skills/nvidia",
    );
    // TestMu AI — 50+ test frameworks (representative, covers all 1497 shape)
    for (id, desc) in [
        ("testmu-ai/playwright", "Playwright E2E tests"),
        ("testmu-ai/cypress", "Cypress E2E"),
        ("testmu-ai/jest", "Jest unit tests"),
        ("testmu-ai/vitest", "Vitest"),
        ("testmu-ai/selenium", "Selenium WebDriver"),
        ("testmu-ai/appium", "Appium mobile"),
        ("testmu-ai/api-skill", "REST/GraphQL API tests"),
        ("testmu-ai/cucumber", "Cucumber BDD"),
    ] {
        add(
            &format!("testmu/{}", id.split('/').nth(1).unwrap()),
            id.split('/').nth(1).unwrap(),
            desc,
            "TestMu AI",
            &format!(
                "https://github.com/LambdaTest/agent-skills/tree/main/{}-skill",
                id.split('/').nth(1).unwrap()
            ),
        );
    }
    // HashiCorp / Terraform
    for (id, desc) in [
        ("hashicorp/terraform-style-guide", "Terraform style"),
        ("hashicorp/new-terraform-provider", "New Terraform provider"),
    ] {
        add(
            id,
            id.split('/').nth(1).unwrap(),
            desc,
            "HashiCorp",
            &format!(
                "https://officialskills.sh/{}/skills/{}",
                "hashicorp",
                id.split('/').nth(1).unwrap()
            ),
        );
    }
    // Google / Stripe / others
    add(
        "google/gemini-api",
        "gemini-api",
        "Gemini API",
        "Google",
        "https://officialskills.sh/google-gemini/skills/gemini-api-dev",
    );
    add(
        "google/workspace-cli",
        "workspace-cli",
        "Google Workspace CLI",
        "Google",
        "https://officialskills.sh/google/skills/workspace-cli",
    );
    // Community (representative)
    add(
        "community/pr-review",
        "pr-review",
        "PR review workflow",
        "Community",
        "https://github.com/VoltAgent/awesome-agent-skills",
    );
    add(
        "community/code-rabbit",
        "code-rabbit",
        "CodeRabbit AI review",
        "Community",
        "https://officialskills.sh/coderabbit/skills/coderabbit",
    );

    v
}

/// Resolve a catalog id to a concrete skill source.
///
/// The static catalog is a starter set. Everything else is resolved from the
/// live VoltAgent README, which is the canonical index for the 1,497+ skills.
async fn resolve_meta(sid: &str) -> Result<Option<AwesomeSkillMeta>, String> {
    if let Some(meta) = catalog()
        .into_iter()
        .find(|meta| meta.id.eq_ignore_ascii_case(sid))
    {
        return Ok(Some(meta));
    }
    let url = if sid.starts_with("http://") || sid.starts_with("https://") {
        sid.to_string()
    } else {
        let text = readme().await?;
        match readme_url_for_id(text, sid) {
            Some(url) => url,
            None => return Ok(None),
        }
    };
    let name = url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|part| !part.is_empty())
        .unwrap_or(sid)
        .to_string();
    Ok(Some(AwesomeSkillMeta {
        id: sid.to_string(),
        name,
        description: "Skill from the live VoltAgent awesome-agent-skills catalog".to_string(),
        category: "Live VoltAgent catalog".to_string(),
        url,
    }))
}

/// A meta-skill that lazily fetches any awesome skill by id or URL.
pub struct AwesomeFetchSkill;

impl AwesomeFetchSkill {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Skill for AwesomeFetchSkill {
    fn id(&self) -> &str {
        "awesome_fetch"
    }
    fn name(&self) -> &str {
        "Awesome Fetch"
    }
    fn description(&self) -> &str {
        "Load a skill from the live 1,497+ VoltAgent awesome-agent-skills catalog by id or URL and follow its real SKILL.md (e.g. anthropics/docx, vercel-labs/next-best-practices)."
    }
    fn version(&self) -> &str {
        "1.0.0"
    }
    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::Network {
            domains: vec!["*".into()],
        }]
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({"type":"object","properties":{"skill_id":{"type":"string","description":"Catalog id or source URL, e.g. anthropics/pptx, vercel-labs/next-best-practices"},"task":{"type":"string","description":"The task to apply the skill to"}},"required":["skill_id"]})
    }
    async fn execute(
        &self,
        ctx: &SkillContext,
        args: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        let sid = args
            .get("skill_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing skill_id".into()))?;
        let task = args.get("task").and_then(|v| v.as_str()).unwrap_or("");
        match resolve_meta(sid).await {
            Ok(Some(meta)) => {
                AwesomeSkill::new(meta)
                    .execute(ctx, serde_json::json!({"task": task}))
                    .await
            }
            Ok(None) => Ok(SkillResult::success(serde_json::json!({
                "skill_id": sid,
                "found": false,
                "total_catalog": catalog().len(),
                "hint": "Id not found in the live VoltAgent index. Try the exact owner/name from https://github.com/VoltAgent/awesome-agent-skills, or pass the skill's GitHub or officialskills.sh URL directly."
            }))),
            Err(error) => Ok(SkillResult::failure(format!(
                "The live skill catalog could not be reached, so '{sid}' was not loaded. {error} Try again with a network connection, or pass a direct skill URL."
            ))),
        }
    }
}
impl Default for AwesomeFetchSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod fetch_tests {
    use super::*;

    #[test]
    fn catalog_readme_search_filters_and_dedups() {
        let readme = "# Catalog\n\n- **[Awesome](https://awesome.re)** — bad link.\n- **[Discord](https://s.voltagent.dev/discord)** — sponsor.\n\n## Documents\n\n- **[anthropics/docx](https://github.com/anthropics/skills/tree/main/skills/docx)** — Create and edit Word documents.\n- **[anthropics/pptx](https://github.com/anthropics/skills/tree/main/skills/pptx)** — Build slide decks.\n\n## Web\n\n- **[vercel-labs/next-best-practices](https://github.com/vercel-labs/next-best-practices)** — Next.js guidance.\n- **[anthropics/docx](https://github.com/anthropics/skills/tree/main/skills/docx)** — duplicate.\n";
        let all = parse_catalog_readme(readme, "", 50);
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].id, "anthropics/docx");
        assert_eq!(all[0].category, "Documents");
        assert_eq!(all[0].description, "Create and edit Word documents.");
        let docs = parse_catalog_readme(readme, "document", 50);
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].name, "docx");
        let web = parse_catalog_readme(readme, "next", 50);
        assert_eq!(web[0].category, "Web");
        assert!(parse_catalog_readme(readme, "", 2).len() == 2);
    }

    #[test]
    fn github_tree_url_becomes_raw_skill_candidates() {
        let candidates =
            github_raw_candidates("https://github.com/anthropics/skills/tree/main/skills/docx");
        assert!(
            candidates.contains(
                &"https://raw.githubusercontent.com/anthropics/skills/main/skills/docx/SKILL.md"
                    .to_string()
            ),
            "got: {candidates:?}"
        );
    }

    #[test]
    fn github_blob_markdown_is_fetched_directly() {
        let candidates =
            github_raw_candidates("https://github.com/o/r/blob/main/skills/x/SKILL.md");
        assert!(
            candidates.contains(
                &"https://raw.githubusercontent.com/o/r/main/skills/x/SKILL.md".to_string()
            ),
            "got: {candidates:?}"
        );
    }

    #[test]
    fn officialskills_page_exposes_its_github_source() {
        let html = r#"<a href="https://github.com/anthropics/skills/tree/main/skills/docx">View on GitHub</a>"#;
        assert_eq!(
            github_link_from_html(html).as_deref(),
            Some("https://github.com/anthropics/skills/tree/main/skills/docx")
        );
    }

    #[test]
    fn readme_resolves_a_live_catalog_id() {
        let readme = "- **[anthropics/docx](https://officialskills.sh/anthropics/skills/docx)** - Create, edit, and analyze Word documents";
        assert_eq!(
            readme_url_for_id(readme, "anthropics/docx").as_deref(),
            Some("https://officialskills.sh/anthropics/skills/docx")
        );
    }

    #[tokio::test]
    #[ignore = "network access: verifies the live catalog resolves a real SKILL.md"]
    async fn live_catalog_fetch_resolves_anthropics_docx() {
        let (text, source) =
            resolve_instructions("https://officialskills.sh/anthropics/skills/docx")
                .await
                .expect("live skill fetch");
        assert!(text.to_ascii_lowercase().contains("docx"), "got: {text}");
        assert!(
            source.starts_with("https://github.com/"),
            "source: {source}"
        );
    }

    #[test]
    fn long_instructions_are_truncated_with_a_marker() {
        let text = "x".repeat(MAX_INSTRUCTION_CHARS + 20);
        let clipped = clip_instructions(&text);
        assert!(clipped.contains("Skill instructions truncated"));
        assert!(clipped.chars().count() > MAX_INSTRUCTION_CHARS);
    }
}
