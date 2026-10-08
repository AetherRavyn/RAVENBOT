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

        // ── Original 25 built-in skills ──────────────────────────────────
        registry.register(Arc::new(WebSearchSkill::new()));
        registry.register(Arc::new(FileReadSkill::new()));
        registry.register(Arc::new(FileWriteSkill::new()));
        registry.register(Arc::new(ShellExecSkill::new()));
        registry.register(Arc::new(DelegateSkill::new()));
        registry.register(Arc::new(ScreenshotSkill::new()));
        registry.register(Arc::new(AnalyzeImageSkill::new()));
        registry.register(Arc::new(VoiceInputSkill::new()));
        registry.register(Arc::new(VoiceOutputSkill::new()));
        registry.register(Arc::new(CodeSearchSkill::new()));
        registry.register(Arc::new(GitSkill::new()));
        registry.register(Arc::new(BrowserSkill::new()));
        registry.register(Arc::new(DbQuerySkill::new()));
        registry.register(Arc::new(TavilySearchSkill::new()));
        registry.register(Arc::new(MemorySaveSkill::new()));
        registry.register(Arc::new(MemoryRecallSkill::new()));
        registry.register(Arc::new(FileTreeSkill::new()));
        registry.register(Arc::new(CodeEditSkill::new()));
        registry.register(Arc::new(HttpRequestSkill::new()));
        registry.register(Arc::new(TodoSkill::new()));
        registry.register(Arc::new(AskUserSkill::new()));
        registry.register(Arc::new(ComputerControlSkill::new()));
        registry.register(Arc::new(YoutubeSkill::new()));
        registry.register(Arc::new(ArxivSkill::new()));
        registry.register(Arc::new(CalendarSkill::new()));
        registry.register(Arc::new(DockerSkill::new()));
        registry.register(Arc::new(ImageGenSkill::new()));

        // ── mattpocock/skills: Engineering (prompt-based) ─────────────────
        registry.register(Arc::new(prompt_skill::tdd_skill()));
        registry.register(Arc::new(prompt_skill::code_review_skill()));
        registry.register(Arc::new(prompt_skill::diagnosing_bugs_skill()));
        registry.register(Arc::new(prompt_skill::research_skill()));
        registry.register(Arc::new(prompt_skill::codebase_design_skill()));
        registry.register(Arc::new(prompt_skill::improve_architecture_skill()));
        registry.register(Arc::new(prompt_skill::domain_modeling_skill()));
        registry.register(Arc::new(prompt_skill::prototype_skill()));
        registry.register(Arc::new(prompt_skill::resolving_merge_conflicts_skill()));
        registry.register(Arc::new(prompt_skill::ask_matt_skill()));
        registry.register(Arc::new(prompt_skill::grill_with_docs_skill()));
        registry.register(Arc::new(prompt_skill::triage_skill()));
        registry.register(Arc::new(prompt_skill::setup_matt_pocock_skills_skill()));
        registry.register(Arc::new(prompt_skill::to_spec_skill()));
        registry.register(Arc::new(prompt_skill::to_tickets_skill()));
        registry.register(Arc::new(prompt_skill::implement_skill()));
        registry.register(Arc::new(prompt_skill::wayfinder_skill()));

        // ── mattpocock/skills: Productivity (prompt-based) ────────────────
        registry.register(Arc::new(prompt_skill::grilling_skill()));
        registry.register(Arc::new(prompt_skill::handoff_skill()));
        registry.register(Arc::new(prompt_skill::teach_skill()));
        registry.register(Arc::new(prompt_skill::wait_what_skill()));
        registry.register(Arc::new(prompt_skill::writing_for_agents_skill()));
        registry.register(Arc::new(prompt_skill::to_questionnaire_skill()));
        registry.register(Arc::new(prompt_skill::grill_me_skill()));
        registry.register(Arc::new(prompt_skill::wizard_skill()));

        // ── New tool skills ──────────────────────────────────────────────
        registry.register(Arc::new(SystemMonitorSkill::new()));
        registry.register(Arc::new(PackageManagerSkill::new()));
        registry.register(Arc::new(SshRemoteSkill::new()));
        registry.register(Arc::new(ApiTesterSkill::new()));
        registry.register(Arc::new(EnvManagerSkill::new()));
        registry.register(Arc::new(NoteManagerSkill::new()));
        registry.register(Arc::new(TaskRunnerSkill::new()));

        // ── Awesome: 1497 curated skills ─────────────────────────────────
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
