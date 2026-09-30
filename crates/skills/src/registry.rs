//! Skill registry for managing available skills

use crate::traits::Skill;
use crate::builtin::*;
use std::collections::HashMap;
use std::sync::Arc;

/// Registry of all available skills
pub struct SkillRegistry {
    skills: HashMap<String, Arc<dyn Skill>>,
}

impl SkillRegistry {
    /// Create a new registry with built-in skills
    pub fn new_builtin() -> Self {
        let mut registry = Self {
            skills: HashMap::new(),
        };

        // Register built-in skills — 9 originals
        registry.register(Arc::new(WebSearchSkill::new()));
        registry.register(Arc::new(FileReadSkill::new()));
        registry.register(Arc::new(FileWriteSkill::new()));
        registry.register(Arc::new(ShellExecSkill::new()));
        registry.register(Arc::new(DelegateSkill::new()));
        registry.register(Arc::new(ScreenshotSkill::new()));
        registry.register(Arc::new(AnalyzeImageSkill::new()));
        registry.register(Arc::new(VoiceInputSkill::new()));
        registry.register(Arc::new(VoiceOutputSkill::new()));
        // Tier-1 lane closers — beat anyone
        registry.register(Arc::new(CodeSearchSkill::new()));
        registry.register(Arc::new(GitSkill::new()));
        registry.register(Arc::new(BrowserSkill::new()));
        registry.register(Arc::new(DbQuerySkill::new()));
        registry.register(Arc::new(TavilySearchSkill::new()));
        registry.register(Arc::new(MemorySaveSkill::new()));
        registry.register(Arc::new(MemoryRecallSkill::new()));
        // More — perfect office
        registry.register(Arc::new(FileTreeSkill::new()));
        registry.register(Arc::new(CodeEditSkill::new()));
        registry.register(Arc::new(HttpRequestSkill::new()));
        registry.register(Arc::new(TodoSkill::new()));
        registry.register(Arc::new(YoutubeSkill::new()));
        registry.register(Arc::new(ArxivSkill::new()));
        registry.register(Arc::new(CalendarSkill::new()));
        registry.register(Arc::new(DockerSkill::new()));
        registry.register(Arc::new(crate::builtin::image_gen::ImageGenSkill::new()));
        // Awesome — 1497 curated (30 flagship + fetcher for all)
        for meta in crate::awesome::catalog() {
            registry.register(Arc::new(crate::awesome::AwesomeSkill::new(meta)));
        }
        registry.register(Arc::new(crate::awesome::AwesomeFetchSkill::new()));

        registry
    }

    /// Register a new skill
    pub fn register(&mut self, skill: Arc<dyn Skill>) {
        self.skills
            .insert(skill.id().to_string(), skill);
    }

    /// Get a skill by ID
    pub fn get(&self, id: &str) -> Option<Arc<dyn Skill>> {
        self.skills.get(id).cloned()
    }

    /// List all registered skills
    pub fn list(&self) -> Vec<Arc<dyn Skill>> {
        self.skills.values().cloned().collect()
    }

    /// Get skill definitions for the model
    pub fn get_tool_definitions(&self) -> Vec<serde_json::Value> {
        self.skills
            .values()
            .map(|skill| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": skill.id(),
                        "description": skill.description(),
                        "parameters": skill.input_schema()
                    }
                })
            })
            .collect()
    }

    /// Execute a skill by ID
    pub async fn execute(
        &self,
        skill_id: &str,
        context: &crate::traits::SkillContext,
        arguments: serde_json::Value,
    ) -> Result<crate::traits::SkillResult, crate::traits::SkillError> {
        let skill = self
            .get(skill_id)
            .ok_or_else(|| crate::traits::SkillError::Execution(
                format!("Skill not found: {}", skill_id)
            ))?;

        // Validate arguments
        skill.validate_arguments(&arguments)?;

        // Execute
        skill.execute(context, arguments).await
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new_builtin()
    }
}
