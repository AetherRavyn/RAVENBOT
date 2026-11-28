//! Agent/profile/routine administration tools.
//!
//! These mirror the desktop IPC surface, but deliberately narrower:
//! `create_agent` starts with no permissions, no delegation targets, no host
//! control and Ask approvals; `update_agent` changes identity/model fields
//! only. Permissions, skills, delegate lists and approval modes stay in the
//! owner-only Settings surface. Routine creation starts paused unless the owner
//! explicitly asks for it to be enabled.
//!
//! The runtime intercepts every name and enforces the actor role. A paired chat
//! or unknown role cannot use this surface; an operator can read agents/routines,
//! interrupt a running agent and test a routine, but cannot create or change
//! configuration.

use async_trait::async_trait;
use ravenbot_core::Permission;
use serde_json::{json, Value};

use crate::traits::{Skill, SkillContext, SkillError, SkillResult, SkillRisk};

fn runtime_only(id: &str) -> SkillError {
    SkillError::Execution(format!(
        "{id} needs the desktop/daemon runtime's agent and routine store; standalone registry execution is unsupported."
    ))
}

macro_rules! admin_skill {
    ($struct_name:ident, $id:literal, $name:literal, $description:literal, $schema:expr) => {
        pub struct $struct_name;

        impl $struct_name {
            pub fn new() -> Self {
                Self
            }
        }

        impl Default for $struct_name {
            fn default() -> Self {
                Self::new()
            }
        }

        #[async_trait]
        impl Skill for $struct_name {
            fn id(&self) -> &str {
                $id
            }
            fn name(&self) -> &str {
                $name
            }
            fn description(&self) -> &str {
                $description
            }
            fn version(&self) -> &str {
                "1.0.0"
            }
            fn required_permissions(&self) -> Vec<Permission> {
                vec![]
            }
            fn input_schema(&self) -> Value {
                $schema
            }
            fn risk(&self) -> SkillRisk {
                // Every tool here can reveal or change workspace configuration,
                // or start/cancel work. The runtime applies the finer role gate.
                SkillRisk::High
            }
            async fn execute(
                &self,
                _ctx: &SkillContext,
                _args: Value,
            ) -> Result<SkillResult, SkillError> {
                Err(runtime_only($id))
            }
        }
    };
}

admin_skill!(
    ListAgentsSkill,
    "list_agents",
    "List agents",
    "List the agents in this workspace with id, name, description, rank, specialty, status, model and orchestrator flag. Read-only and available to every workspace role; it does not return custom prompts or working-folder paths.",
    json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
);

admin_skill!(
    ReadAgentSkill,
    "read_agent",
    "Read an agent",
    "Read one agent's safe profile: identity, rank, specialty, status, model, approval mode, enabled skill ids, permission names, delegation targets and host-control flag. It never returns the system prompt or working-folder path. Read-only.",
    json!({
        "type": "object",
        "properties": {
            "agent": { "type": "string", "description": "Agent id or exact name." }
        },
        "required": ["agent"],
        "additionalProperties": false
    })
);

admin_skill!(
    CreateAgentSkill,
    "create_agent",
    "Create an agent",
    "Create a new agent with a name and description. Owner-only. The new agent starts with no permissions, no delegation targets, no host control, Ask approvals and a minimal skill set; configure capabilities in Settings. Provider and model default to the caller's. It cannot create an orchestrator or grant privileges.",
    json!({
        "type": "object",
        "properties": {
            "name": { "type": "string", "description": "Agent name, 1-120 characters." },
            "description": { "type": "string", "description": "What the agent is for, up to 2,000 characters." },
            "rank": { "type": "string", "description": "Optional office rank or lane, up to 120 characters." },
            "specialty": { "type": "string", "description": "Optional specialty, up to 120 characters." },
            "provider": { "type": "string", "description": "Optional model provider; defaults to the caller's provider." },
            "model": { "type": "string", "description": "Optional model id; defaults to the caller's model." }
        },
        "required": ["name", "description"],
        "additionalProperties": false
    })
);

admin_skill!(
    UpdateAgentSkill,
    "update_agent",
    "Update an agent",
    "Update an existing agent's name, description, rank, specialty, provider or model. Owner-only. Permissions, skills, delegation targets, approval mode, host control and working folder are intentionally not editable here; use the owner-only Settings surface for those.",
    json!({
        "type": "object",
        "properties": {
            "agent": { "type": "string", "description": "Agent id or exact name." },
            "name": { "type": "string", "description": "New name, 1-120 characters." },
            "description": { "type": "string", "description": "New description, up to 2,000 characters." },
            "rank": { "type": "string", "description": "New rank, up to 120 characters." },
            "specialty": { "type": "string", "description": "New specialty, up to 120 characters." },
            "provider": { "type": "string", "description": "New model provider." },
            "model": { "type": "string", "description": "New model id." }
        },
        "required": ["agent"],
        "additionalProperties": false
    })
);

admin_skill!(
    InterruptAgentSkill,
    "interrupt_agent",
    "Interrupt an agent",
    "Ask every active non-paused run for an agent to stop at its next boundary. Owner/operator only, matching `cancel_run`. Paused runs stay paused. Returns the count and run ids; it does not delete work.",
    json!({
        "type": "object",
        "properties": {
            "agent": { "type": "string", "description": "Agent id or exact name." },
            "reason": { "type": "string", "description": "Why the work is being interrupted, up to 500 characters." }
        },
        "required": ["agent"],
        "additionalProperties": false
    })
);

admin_skill!(
    ListRoutinesSkill,
    "list_routines",
    "List routines",
    "List an agent's schedules, newest metadata first: id, name, description, schedule, instruction, enabled state and last run. Defaults to the caller; pass `bot` for another agent in this workspace. Read-only.",
    json!({
        "type": "object",
        "properties": {
            "bot": { "type": "string", "description": "Optional agent id or exact name; defaults to the caller." }
        },
        "additionalProperties": false
    })
);

admin_skill!(
    CreateRoutineSkill,
    "create_routine",
    "Create a routine",
    "Create a cron routine for an agent. Owner-only, matching `create_routine`. It starts paused by default so the scheduler cannot fire unreviewed work; pass `enabled: true` to arm it immediately. The schedule and instruction are validated before storage.",
    json!({
        "type": "object",
        "properties": {
            "bot": { "type": "string", "description": "Optional agent id or exact name; defaults to the caller." },
            "name": { "type": "string", "description": "Routine name, 1-120 characters." },
            "schedule": { "type": "string", "description": "Cron expression or the scheduler's @manual form." },
            "description": { "type": "string", "description": "Optional description, up to 2,000 characters." },
            "instruction": { "type": "string", "description": "What the agent should do, 1-8,000 characters." },
            "enabled": { "type": "boolean", "description": "Arm the routine immediately. Default false." }
        },
        "required": ["name", "schedule", "instruction"],
        "additionalProperties": false
    })
);

admin_skill!(
    UpdateRoutineSkill,
    "update_routine",
    "Update a routine",
    "Update a routine's name, description, schedule, instruction or enabled state. Owner-only. It cannot move the routine to another agent. Schedule and instruction are validated before storage.",
    json!({
        "type": "object",
        "properties": {
            "routine_id": { "type": "string", "description": "Routine id from list_routines." },
            "name": { "type": "string", "description": "New name, 1-120 characters." },
            "description": { "type": "string", "description": "New description, up to 2,000 characters." },
            "schedule": { "type": "string", "description": "New cron expression or @manual form." },
            "instruction": { "type": "string", "description": "New instruction, 1-8,000 characters." },
            "enabled": { "type": "boolean", "description": "Arm or pause the routine." }
        },
        "required": ["routine_id"],
        "additionalProperties": false
    })
);

admin_skill!(
    DeleteRoutineSkill,
    "delete_routine",
    "Delete a routine",
    "Delete a routine and its MCP event triggers. Owner-only, matching `delete_routine`. This does not delete past runs or their threads.",
    json!({
        "type": "object",
        "properties": {
            "routine_id": { "type": "string", "description": "Routine id from list_routines." }
        },
        "required": ["routine_id"],
        "additionalProperties": false
    })
);

admin_skill!(
    TestRoutineSkill,
    "test_routine",
    "Test a routine",
    "Run a routine now on a hidden routine thread without changing its schedule or last-run time. Owner/operator only, matching `run_routine_now`. Returns the run state and a bounded final result; high-risk tools inside the test use the normal Review gates.",
    json!({
        "type": "object",
        "properties": {
            "routine_id": { "type": "string", "description": "Routine id from list_routines." }
        },
        "required": ["routine_id"],
        "additionalProperties": false
    })
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::Skill;

    #[test]
    fn every_admin_tool_is_high_risk_and_documented() {
        let tools: Vec<Box<dyn Skill>> = vec![
            Box::new(ListAgentsSkill::new()),
            Box::new(ReadAgentSkill::new()),
            Box::new(CreateAgentSkill::new()),
            Box::new(UpdateAgentSkill::new()),
            Box::new(InterruptAgentSkill::new()),
            Box::new(ListRoutinesSkill::new()),
            Box::new(CreateRoutineSkill::new()),
            Box::new(UpdateRoutineSkill::new()),
            Box::new(DeleteRoutineSkill::new()),
            Box::new(TestRoutineSkill::new()),
        ];
        for tool in tools {
            assert_eq!(tool.risk(), SkillRisk::High, "{}", tool.id());
            assert_eq!(tool.input_schema()["type"], "object", "{}", tool.id());
            assert!(!tool.description().is_empty(), "{}", tool.id());
            assert!(
                tool.description().contains("Owner")
                    || tool.description().contains("Read-only")
                    || tool.description().contains("Owner/operator"),
                "{}",
                tool.id()
            );
        }
    }

    #[test]
    fn the_update_agent_schema_has_no_privilege_fields() {
        let schema = UpdateAgentSkill::new().input_schema();
        let properties = schema["properties"].as_object().expect("properties");
        for forbidden in [
            "permissions",
            "skills",
            "delegate_to",
            "approval_mode",
            "host_control",
            "working_folder",
            "is_orchestrator",
        ] {
            assert!(!properties.contains_key(forbidden), "{forbidden}: {schema}");
        }
    }

    #[test]
    fn create_agent_schema_cannot_grant_capabilities() {
        let schema = CreateAgentSkill::new().input_schema();
        let properties = schema["properties"].as_object().expect("properties");
        for forbidden in [
            "permissions",
            "skills",
            "delegate_to",
            "approval_mode",
            "host_control",
            "working_folder",
            "is_orchestrator",
        ] {
            assert!(!properties.contains_key(forbidden), "{forbidden}: {schema}");
        }
    }

    #[tokio::test]
    async fn standalone_execution_fails_honestly() {
        let ctx = SkillContext::with_default_tier(
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
        );
        let error = ListAgentsSkill::new()
            .execute(&ctx, json!({}))
            .await
            .expect_err("standalone");
        assert!(error.to_string().contains("agent and routine store"));
    }
}
