//! Web search skill

use async_trait::async_trait;
use ravenbot_core::Permission;

use crate::traits::{Skill, SkillContext, SkillError, SkillResult};

pub struct WebSearchSkill {
    client: reqwest::Client,
}

#[allow(dead_code)]
struct SearchRequest {
    q: String,
    format: String,
    no_redirect: bool,
}

#[allow(dead_code)]
struct SearchResponse {
    results: Vec<SearchResult>,
}

#[allow(dead_code)]
struct SearchResult {
    title: String,
    url: String,
    snippet: Option<String>,
}

impl WebSearchSkill {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl Skill for WebSearchSkill {
    fn id(&self) -> &str {
        "web_search"
    }

    fn name(&self) -> &str {
        "Web Search"
    }

    fn description(&self) -> &str {
        "Search the web for information"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::Network {
            domains: vec!["*".to_string()],
        }]
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results (default: 5)",
                    "minimum": 1,
                    "maximum": 20
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(
        &self,
        _context: &SkillContext,
        arguments: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        let query = arguments
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'query' field".to_string()))?;

        let max_results = arguments
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as usize;

        // Use DuckDuckGo Lite for privacy
        let url = format!(
            "https://lite.duckduckgo.com/lite/?q={}",
            urlencoding::encode(query)
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "RAVENBOT/1.0")
            .send()
            .await
            .map_err(|e| SkillError::Network(e.to_string()))?;

        let html = response
            .text()
            .await
            .map_err(|e| SkillError::Network(e.to_string()))?;

        // Simple HTML parsing for results
        let results = parse_ddg_results(&html, max_results);

        Ok(SkillResult::success(serde_json::json!({
            "query": query,
            "results": results
        })))
    }
}

fn parse_ddg_results(html: &str, max_results: usize) -> Vec<serde_json::Value> {
    let mut results = Vec::new();
    
    // Simple parsing - in production, use a proper HTML parser
    let lines: Vec<&str> = html.lines().collect();
    let mut i = 0;
    
    while i < lines.len() && results.len() < max_results {
        let line = lines[i].trim();
        
        // Look for result links
        if line.starts_with("<a rel=\"nofollow\" class=\"result-link\"") {
            if let Some(start) = line.find("href=\"") {
                let start = start + 6;
                if let Some(end) = line[start..].find("\"") {
                    let url = &line[start..start + end];
                    
                    // Get title from next line
                    if let Some(title_start) = line.find(">") {
                        let title = line[title_start + 1..]
                            .replace("</a>", "")
                            .trim()
                            .to_string();
                        
                        // Get snippet if available
                        let snippet = if i + 1 < lines.len() {
                            let snippet_line = lines[i + 1].trim();
                            if snippet_line.starts_with("<td class=\"result-snippet\">") {
                                Some(snippet_line
                                    .replace("<td class=\"result-snippet\">", "")
                                    .replace("</td>", "")
                                    .trim()
                                    .to_string())
                            } else {
                                None
                            }
                        } else {
                            None
                        };
                        
                        results.push(serde_json::json!({
                            "title": title,
                            "url": url,
                            "snippet": snippet
                        }));
                    }
                }
            }
        }
        
        i += 1;
    }
    
    results
}

impl Default for WebSearchSkill {
    fn default() -> Self {
        Self::new()
    }
}
