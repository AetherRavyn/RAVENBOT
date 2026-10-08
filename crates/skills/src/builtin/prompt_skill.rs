//! Prompt-based skills — adapts mattpocock-style Markdown workflow skills
//! into RAVENBOT's Skill trait. These skills return structured instructions
//! that the LLM follows as a multi-turn workflow.

use async_trait::async_trait;
use ravenbot_core::Permission;
use crate::traits::{Skill, SkillContext, SkillError, SkillResult, SkillKind};

/// A prompt-based skill that returns a structured workflow guide to the LLM.
/// Unlike tool skills that execute code, these provide instructions for the
/// LLM to follow across multiple conversation turns.
pub struct PromptSkill {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    version: &'static str,
    permissions: Vec<Permission>,
    prompt: &'static str,
    input_schema: serde_json::Value,
}

impl PromptSkill {
    pub const fn new(
        id: &'static str,
        name: &'static str,
        description: &'static str,
        version: &'static str,
        permissions: Vec<Permission>,
        prompt: &'static str,
        input_schema: serde_json::Value,
    ) -> Self {
        Self { id, name, description, version, permissions, prompt, input_schema }
    }
}

#[async_trait]
impl Skill for PromptSkill {
    fn id(&self) -> &str { self.id }
    fn name(&self) -> &str { self.name }
    fn description(&self) -> &str { self.description }
    fn version(&self) -> &str { self.version }
    fn required_permissions(&self) -> Vec<Permission> { self.permissions.clone() }
    fn input_schema(&self) -> serde_json::Value { self.input_schema.clone() }
    fn kind(&self) -> SkillKind { SkillKind::Workflow }
    fn prompt(&self) -> Option<&str> { Some(self.prompt) }

    async fn execute(
        &self,
        _context: &SkillContext,
        arguments: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        let arg_context = if arguments.is_null() || arguments == serde_json::json!({}) {
            String::new()
        } else {
            format!("\n\n## User-provided context\n{}", serde_json::to_string_pretty(&arguments).unwrap_or_default())
        };

        Ok(SkillResult::success(serde_json::json!({
            "skill_type": "prompt_workflow",
            "instructions": format!("{}{}", self.prompt, arg_context),
            "message": "Follow the workflow instructions below. This is a multi-turn guided process."
        })))
    }
}

impl Default for PromptSkill {
    fn default() -> Self {
        Self::new(
            "prompt_skill",
            "Prompt Skill",
            "Base prompt-based skill",
            "1.0.0",
            vec![],
            "",
            serde_json::json!({"type":"object","properties":{}}),
        )
    }
}

// ─── Engineering Skills ──────────────────────────────────────────────────────

pub fn tdd_skill() -> PromptSkill {
    PromptSkill::new(
        "tdd",
        "Test-Driven Development",
        "Test-driven development with red-green-refactor loop. Use when the user wants to build features or fix bugs test-first, mentions 'red-green-refactor', or wants integration tests.",
        "1.0.0",
        vec![Permission::FileSystem { paths: vec![".".to_string()] }, Permission::Shell],
        include_str!("./prompts/tdd.md"),
        serde_json::json!({
            "type": "object",
            "properties": {
                "feature": {"type": "string", "description": "Feature or bug to implement"},
                "seams": {"type": "string", "description": "Pre-agreed seams to test at (optional)"},
                "language": {"type": "string", "description": "Programming language (auto-detected if omitted)"}
            },
            "required": ["feature"]
        }),
    )
}

pub fn code_review_skill() -> PromptSkill {
    PromptSkill::new(
        "code_review",
        "Code Review",
        "Two-axis review of changes: Standards (repo coding standards + Fowler smell baseline) and Spec (faithful implementation of originating issue/spec). Runs as parallel sub-agents.",
        "1.0.0",
        vec![Permission::FileSystem { paths: vec![".".to_string()] }, Permission::Shell],
        include_str!("./prompts/code_review.md"),
        serde_json::json!({
            "type": "object",
            "properties": {
                "fixed_point": {"type": "string", "description": "Commit SHA, branch, tag, or HEAD~N to compare against"},
                "spec_path": {"type": "string", "description": "Path to spec file (optional)"},
                "scope": {"type": "string", "description": "Files or directories to limit review to (optional)"}
            },
            "required": ["fixed_point"]
        }),
    )
}

pub fn diagnosing_bugs_skill() -> PromptSkill {
    PromptSkill::new(
        "diagnosing_bugs",
        "Diagnosing Bugs",
        "Disciplined diagnosis loop for hard bugs and performance regressions: build a feedback loop that goes red on this bug, minimise, hypothesise, instrument, fix, regression-test.",
        "1.0.0",
        vec![Permission::FileSystem { paths: vec![".".to_string()] }, Permission::Shell],
        include_str!("./prompts/diagnosing_bugs.md"),
        serde_json::json!({
            "type": "object",
            "properties": {
                "symptom": {"type": "string", "description": "What is the observed failure or performance issue?"},
                "reproduction": {"type": "string", "description": "Steps to reproduce, if known"},
                "environment": {"type": "string", "description": "Environment details (OS, runtime, etc.)"}
            },
            "required": ["symptom"]
        }),
    )
}

pub fn research_skill() -> PromptSkill {
    PromptSkill::new(
        "research",
        "Research",
        "Investigate a question against high-trust primary sources and capture findings as a cited Markdown file. Spins up a background agent for parallel work.",
        "1.0.0",
        vec![Permission::Network { domains: vec!["*".to_string()] }, Permission::FileSystem { paths: vec![".".to_string()] }],
        include_str!("./prompts/research.md"),
        serde_json::json!({
            "type": "object",
            "properties": {
                "question": {"type": "string", "description": "The research question to investigate"},
                "output_path": {"type": "string", "description": "Where to save findings (optional)"},
                "sources": {"type": "string", "description": "Preferred source types (docs, source, specs, etc.)"}
            },
            "required": ["question"]
        }),
    )
}

pub fn codebase_design_skill() -> PromptSkill {
    PromptSkill::new(
        "codebase_design",
        "Codebase Design",
        "Shared vocabulary for designing deep modules: a lot of behaviour behind a small interface, placed at a clean seam, testable through that interface.",
        "1.0.0",
        vec![Permission::FileSystem { paths: vec![".".to_string()] }],
        include_str!("./prompts/codebase_design.md"),
        serde_json::json!({
            "type": "object",
            "properties": {
                "module": {"type": "string", "description": "Module or component to design/improve"},
                "goal": {"type": "string", "description": "What you want to achieve (interface design, deepening, seam placement)"}
            },
            "required": ["module"]
        }),
    )
}

pub fn improve_architecture_skill() -> PromptSkill {
    PromptSkill::new(
        "improve_architecture",
        "Improve Codebase Architecture",
        "Scan a codebase for deepening opportunities, present them as a visual HTML report, then grill through whichever one you pick.",
        "1.0.0",
        vec![Permission::FileSystem { paths: vec![".".to_string()] }, Permission::Shell],
        include_str!("./prompts/improve_architecture.md"),
        serde_json::json!({
            "type": "object",
            "properties": {
                "scope": {"type": "string", "description": "Module, subsystem, or pain point to focus on (optional)"},
                "depth": {"type": "string", "description": "How far to scan: quick, normal, deep (default: normal)"}
            }
        }),
    )
}

pub fn domain_modeling_skill() -> PromptSkill {
    PromptSkill::new(
        "domain_modeling",
        "Domain Modeling",
        "Build and sharpen a project's domain model: challenge terms against the glossary, stress-test with edge-case scenarios, update CONTEXT.md and ADRs inline.",
        "1.0.0",
        vec![Permission::FileSystem { paths: vec![".".to_string()] }],
        include_str!("./prompts/domain_modeling.md"),
        serde_json::json!({
            "type": "object",
            "properties": {
                "topic": {"type": "string", "description": "Domain area or concept to model"},
                "update_context": {"type": "boolean", "description": "Whether to update CONTEXT.md inline (default: true)"}
            },
            "required": ["topic"]
        }),
    )
}

pub fn prototype_skill() -> PromptSkill {
    PromptSkill::new(
        "prototype",
        "Prototype",
        "Build a throwaway prototype to answer a design question. Either a single HTML file for state/logic questions, or several UI variations toggleable from one route.",
        "1.0.0",
        vec![Permission::FileSystem { paths: vec![".".to_string()] }, Permission::Shell],
        include_str!("./prompts/prototype.md"),
        serde_json::json!({
            "type": "object",
            "properties": {
                "question": {"type": "string", "description": "What design question does this prototype answer?"},
                "branch": {"type": "string", "description": "logic (state model) or ui (look & feel)"}
            },
            "required": ["question"]
        }),
    )
}

pub fn resolving_merge_conflicts_skill() -> PromptSkill {
    PromptSkill::new(
        "resolving_merge_conflicts",
        "Resolving Merge Conflicts",
        "Work through an in-progress git merge or rebase conflict hunk by hunk, resolving by intent traced to each side's primary source, then finish the operation.",
        "1.0.0",
        vec![Permission::FileSystem { paths: vec![".".to_string()] }, Permission::Shell],
        include_str!("./prompts/resolving_merge_conflicts.md"),
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {"type": "string", "description": "merge or rebase"},
                "strategy": {"type": "string", "description": "Prefer 'ours', 'theirs', or 'manual' (default: manual)"}
            }
        }),
    )
}

// ─── Productivity Skills ─────────────────────────────────────────────────────

pub fn grilling_skill() -> PromptSkill {
    PromptSkill::new(
        "grilling",
        "Grilling",
        "Interview the user relentlessly about a plan, decision, or idea until every branch of the design tree is resolved. Works the tree in rounds with a frontier.",
        "1.0.0",
        vec![],
        include_str!("./prompts/grilling.md"),
        serde_json::json!({
            "type": "object",
            "properties": {
                "topic": {"type": "string", "description": "The plan, decision, or idea to grill"},
                "depth": {"type": "string", "description": "How thorough: quick, normal, exhaustive (default: normal)"}
            },
            "required": ["topic"]
        }),
    )
}

pub fn handoff_skill() -> PromptSkill {
    PromptSkill::new(
        "handoff",
        "Handoff",
        "Compact the current conversation into a handoff document so another agent can continue the work. Saves to OS temp directory.",
        "1.0.0",
        vec![],
        include_str!("./prompts/handoff.md"),
        serde_json::json!({
            "type": "object",
            "properties": {
                "next_focus": {"type": "string", "description": "What the next session will focus on"},
                "include_artifacts": {"type": "boolean", "description": "Include references to specs, ADRs, issues (default: true)"}
            }
        }),
    )
}

pub fn teach_skill() -> PromptSkill {
    PromptSkill::new(
        "teach",
        "Teach",
        "Teach the user a new skill or concept over multiple sessions, using the current directory as a stateful teaching workspace.",
        "1.0.0",
        vec![Permission::FileSystem { paths: vec![".".to_string()] }],
        include_str!("./prompts/teach.md"),
        serde_json::json!({
            "type": "object",
            "properties": {
                "subject": {"type": "string", "description": "What to teach"},
                "level": {"type": "string", "description": "beginner, intermediate, advanced (auto-detected if omitted)"}
            },
            "required": ["subject"]
        }),
    )
}

pub fn wait_what_skill() -> PromptSkill {
    PromptSkill::new(
        "wait_what",
        "Wait What",
        "Fire this the moment a message doesn't land. The agent re-pitches it with the context you're missing, in plain English, using your CONTEXT.md vocabulary.",
        "1.0.0",
        vec![],
        include_str!("./prompts/wait_what.md"),
        serde_json::json!({
            "type": "object",
            "properties": {
                "message": {"type": "string", "description": "The message or explanation that didn't land"},
                "audience": {"type": "string", "description": "Who you're explaining to (optional)"}
            },
            "required": ["message"]
        }),
    )
}

pub fn writing_for_agents_skill() -> PromptSkill {
    PromptSkill::new(
        "writing_for_agents",
        "Writing For Agents",
        "Writing documents for agents: skills, AGENTS.md/CLAUDE.md, and any doc an agent reaches by a pointer.",
        "1.0.0",
        vec![Permission::FileSystem { paths: vec![".".to_string()] }],
        include_str!("./prompts/writing_for_agents.md"),
        serde_json::json!({
            "type": "object",
            "properties": {
                "doc_type": {"type": "string", "description": "Type of document: skill, agents_md, context_md, spec, adr"},
                "topic": {"type": "string", "description": "What the document is about"}
            },
            "required": ["doc_type"]
        }),
    )
}

pub fn to_questionnaire_skill() -> PromptSkill {
    PromptSkill::new(
        "to_questionnaire",
        "To Questionnaire",
        "Turn a decision you can't answer alone into a Markdown questionnaire for the one person who can, filled in async or together over a meeting.",
        "1.0.0",
        vec![],
        include_str!("./prompts/to_questionnaire.md"),
        serde_json::json!({
            "type": "object",
            "properties": {
                "decision": {"type": "string", "description": "The decision that needs input"},
                "recipient": {"type": "string", "description": "Who can answer (role or person)"}
            },
            "required": ["decision"]
        }),
    )
}

// ─── Remaining Engineering Skills ────────────────────────────────────────────

pub fn ask_matt_skill() -> PromptSkill {
    PromptSkill::new(
        "ask_matt",
        "Ask Matt",
        "Ask which skill or flow fits your situation. A router over all skills in the mattpocock/skills repo.",
        "1.0.0",
        vec![],
        include_str!("./prompts/ask_matt.md"),
        serde_json::json!({
            "type": "object",
            "properties": {
                "situation": {"type": "string", "description": "Describe your current situation or what you're trying to do"}
            },
            "required": ["situation"]
        }),
    )
}

pub fn grill_with_docs_skill() -> PromptSkill {
    PromptSkill::new(
        "grill_with_docs",
        "Grill With Docs",
        "A relentless interview to sharpen a plan or design, which also creates docs (ADRs and glossary) as you go.",
        "1.0.0",
        vec![Permission::FileSystem { paths: vec![".".to_string()] }],
        include_str!("./prompts/grill_with_docs.md"),
        serde_json::json!({
            "type": "object",
            "properties": {
                "topic": {"type": "string", "description": "The plan, design, or idea to grill"},
                "create_issues": {"type": "boolean", "description": "Whether to create ADRs for decisions (default: true)"}
            },
            "required": ["topic"]
        }),
    )
}

pub fn triage_skill() -> PromptSkill {
    PromptSkill::new(
        "triage",
        "Triage",
        "Move issues and external PRs through a state machine of triage roles, categorize, verify, grill if needed, and write agent-ready briefs.",
        "1.0.0",
        vec![Permission::FileSystem { paths: vec![".".to_string()] }, Permission::Shell],
        include_str!("./prompts/triage.md"),
        serde_json::json!({
            "type": "object",
            "properties": {
                "issue": {"type": "string", "description": "Issue number or PR to triage (optional, lists unlabeled if omitted)"},
                "action": {"type": "string", "description": "Specific action: show, triage, move-to-ready, etc."}
            }
        }),
    )
}

pub fn setup_matt_pocock_skills_skill() -> PromptSkill {
    PromptSkill::new(
        "setup_matt_pocock_skills",
        "Setup Matt Pocock Skills",
        "Configure this repo for the engineering skills: set up its issue tracker, triage label vocabulary, and domain doc layout.",
        "1.0.0",
        vec![Permission::FileSystem { paths: vec![".".to_string()] }, Permission::Shell],
        include_str!("./prompts/setup_matt_pocock_skills.md"),
        serde_json::json!({
            "type": "object",
            "properties": {
                "tracker": {"type": "string", "description": "Issue tracker: github, gitlab, local, other (auto-detected if omitted)"},
                "skip_if_configured": {"type": "boolean", "description": "Skip if already configured (default: false)"}
            }
        }),
    )
}

pub fn to_spec_skill() -> PromptSkill {
    PromptSkill::new(
        "to_spec",
        "To Spec",
        "Turn the current conversation into a spec and publish it to the project issue tracker. No interview, just synthesis.",
        "1.0.0",
        vec![Permission::FileSystem { paths: vec![".".to_string()] }, Permission::Shell],
        include_str!("./prompts/to_spec.md"),
        serde_json::json!({
            "type": "object",
            "properties": {
                "title": {"type": "string", "description": "Spec title"},
                "seams": {"type": "string", "description": "Pre-agreed seams to test at"},
                "publish": {"type": "boolean", "description": "Publish to issue tracker (default: true)"}
            },
            "required": ["title"]
        }),
    )
}

pub fn to_tickets_skill() -> PromptSkill {
    PromptSkill::new(
        "to_tickets",
        "To Tickets",
        "Break a plan, spec, or conversation into a set of tracer-bullet tickets, each declaring its blocking edges.",
        "1.0.0",
        vec![Permission::FileSystem { paths: vec![".".to_string()] }, Permission::Shell],
        include_str!("./prompts/to_tickets.md"),
        serde_json::json!({
            "type": "object",
            "properties": {
                "source": {"type": "string", "description": "Spec path or issue number to break into tickets"},
                "max_tickets": {"type": "integer", "description": "Maximum number of tickets (default: 10)"}
            }
        }),
    )
}

pub fn implement_skill() -> PromptSkill {
    PromptSkill::new(
        "implement",
        "Implement",
        "Implement a piece of work based on a spec or set of tickets. Uses TDD at pre-agreed seams, code-review before committing.",
        "1.0.0",
        vec![Permission::FileSystem { paths: vec![".".to_string()] }, Permission::Shell],
        include_str!("./prompts/implement.md"),
        serde_json::json!({
            "type": "object",
            "properties": {
                "spec_or_ticket": {"type": "string", "description": "Spec path or ticket number to implement"},
                "seams": {"type": "string", "description": "Pre-agreed seams to test at"}
            },
            "required": ["spec_or_ticket"]
        }),
    )
}

pub fn wayfinder_skill() -> PromptSkill {
    PromptSkill::new(
        "wayfinder",
        "Wayfinder",
        "Plan a huge chunk of work as a shared map of decision tickets on the issue tracker, resolve them one at a time.",
        "1.0.0",
        vec![Permission::FileSystem { paths: vec![".".to_string()] }, Permission::Shell],
        include_str!("./prompts/wayfinder.md"),
        serde_json::json!({
            "type": "object",
            "properties": {
                "destination": {"type": "string", "description": "What this effort is finding its way to"},
                "mode": {"type": "string", "description": "chart (new map) or work (existing map)"},
                "map_id": {"type": "string", "description": "Map issue number (for work mode)"}
            },
            "required": ["destination"]
        }),
    )
}

// ─── Remaining Productivity Skills ───────────────────────────────────────────

pub fn grill_me_skill() -> PromptSkill {
    PromptSkill::new(
        "grill_me",
        "Grill Me",
        "A relentless interview to sharpen a plan or design. Stateless — saves nothing locally, builds no CONTEXT.md.",
        "1.0.0",
        vec![],
        include_str!("./prompts/grill_me.md"),
        serde_json::json!({
            "type": "object",
            "properties": {
                "topic": {"type": "string", "description": "The plan, design, or idea to grill"},
                "depth": {"type": "string", "description": "How thorough: quick, normal, exhaustive (default: normal)"}
            },
            "required": ["topic"]
        }),
    )
}

pub fn wizard_skill() -> PromptSkill {
    PromptSkill::new(
        "wizard",
        "Wizard",
        "Generate an interactive bash wizard that walks a human through steps only they can perform: provisioning, credentials, CI secrets, migrations.",
        "1.0.0",
        vec![Permission::Shell, Permission::FileSystem { paths: vec![".".to_string()] }],
        include_str!("./prompts/wizard.md"),
        serde_json::json!({
            "type": "object",
            "properties": {
                "procedure": {"type": "string", "description": "What procedure to walk through"},
                "output_path": {"type": "string", "description": "Where to save the wizard script (default: scripts/wizard.sh)"}
            },
            "required": ["procedure"]
        }),
    )
}
