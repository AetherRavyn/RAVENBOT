//! Network access policies for sandboxes

use serde::{Deserialize, Serialize};

/// A network access rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkRule {
    /// Pattern to match (domain or URL prefix)
    pub pattern: String,
    /// Whether this is an allow or deny rule
    pub allow: bool,
    /// Description of why this rule exists
    pub description: Option<String>,
}

impl NetworkRule {
    /// Create an allow rule
    pub fn allow(pattern: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            allow: true,
            description: None,
        }
    }

    /// Create a deny rule
    pub fn deny(pattern: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            allow: false,
            description: None,
        }
    }

    /// With description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Check if a URL matches this rule
    pub fn matches(&self, url: &str) -> bool {
        // Simple pattern matching - could be enhanced with regex
        if self.pattern == "*" {
            return true;
        }
        url.contains(&self.pattern)
    }
}

/// Network access policy for a sandbox
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPolicy {
    /// Whether network access is allowed at all
    pub enabled: bool,
    /// Rules to apply (evaluated in order)
    pub rules: Vec<NetworkRule>,
    /// Maximum concurrent connections
    pub max_connections: u32,
    /// Maximum requests per minute
    pub max_requests_per_minute: u32,
}

impl Default for NetworkPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            rules: vec![
                // Allow common APIs
                NetworkRule::allow("api.openai.com")
                    .with_description("OpenAI API"),
                NetworkRule::allow("api.anthropic.com")
                    .with_description("Anthropic API"),
                NetworkRule::allow("openrouter.ai")
                    .with_description("OpenRouter API"),
                NetworkRule::allow("localhost")
                    .with_description("Local services (Ollama, etc.)"),
                NetworkRule::allow("127.0.0.1")
                    .with_description("Localhost"),
                // Block sensitive domains
                NetworkRule::deny("metadata.google.internal")
                    .with_description("Cloud metadata endpoint"),
            ],
            max_connections: 10,
            max_requests_per_minute: 60,
        }
    }
}

impl NetworkPolicy {
    /// Create a policy that blocks all network access
    pub fn blocked() -> Self {
        Self {
            enabled: false,
            rules: vec![],
            max_connections: 0,
            max_requests_per_minute: 0,
        }
    }

    /// Create a policy that allows all network access
    pub fn permissive() -> Self {
        Self {
            enabled: true,
            rules: vec![NetworkRule::allow("*")],
            max_connections: 100,
            max_requests_per_minute: 1000,
        }
    }

    /// Check if a URL is allowed
    pub fn is_allowed(&self, url: &str) -> bool {
        if !self.enabled {
            return false;
        }

        // Rules are evaluated in order - first match wins
        for rule in &self.rules {
            if rule.matches(url) {
                return rule.allow;
            }
        }

        // Default: deny if no rule matches
        false
    }
}
