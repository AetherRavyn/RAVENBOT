//! Durable agent-task and event-wait tools.
//!
//! The runtime intercepts all six names: the tools need the database, the
//! current run, and the approval/depth state, so the registry versions exist
//! for discovery and fail honestly when executed outside the runtime.

use async_trait::async_trait;
use ravenbot_core::Permission;
use serde_json::{json, Value};

use crate::traits::{Skill, SkillContext, SkillError, SkillResult, SkillRisk};

fn runtime_only(id: &str) -> SkillError {
    SkillError::Execution(format!(
        "{id} needs the desktop/daemon runtime's task store; standalone registry execution is unsupported."
    ))
}

macro_rules! runtime_skill {
    ($struct_name:ident, $id:literal, $name:literal, $description:literal, $schema:expr, $risk:expr) => {
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
                $risk
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

runtime_skill!(
    StartTaskSkill,
    "start_task",
    "Start a background task",
    "Start durable background work for another agent (or yourself). Give a title, instruction, optional expected result, explicit lock resources (`browser` or `workspace:<absolute path>`) and optional task ids this task depends on. The host schedules it under those locks and dependencies, creates a real task thread, and returns a task id. High risk and approval-gated; a person's request is still the reason the work exists. Use `await_event` with `{\"task_id\":\"…\"}` to wait for the result.",
    json!({
        "type": "object",
        "properties": {
            "target": {
                "type": "string",
                "description": "Target agent id or exact name."
            },
            "title": {
                "type": "string",
                "description": "Short task title, at most 120 characters. Defaults to the first part of the instruction."
            },
            "instruction": {
                "type": "string",
                "description": "What to do, up to 8,000 characters."
            },
            "expected_result": {
                "type": "string",
                "description": "What a useful result must contain, up to 2,000 characters."
            },
            "resources": {
                "type": "array",
                "items": { "type": "string" },
                "maxItems": 16,
                "description": "Exclusive locks: `browser` or `workspace:<absolute path>`. Tasks touching the same resource run one at a time. A workspace lock does not grant filesystem access."
            },
            "depends_on": {
                "type": "array",
                "items": { "type": "string" },
                "maxItems": 16,
                "description": "Task ids owned by you that must complete first."
            }
        },
        "required": ["target", "instruction"],
        "additionalProperties": false
    }),
    SkillRisk::High
);

runtime_skill!(
    MessageTaskSkill,
    "message_task",
    "Message a background task",
    "Send a message to a task you own. If the task is queued, the message is part of its history when it starts. If its run is at a tool boundary, the message is steered into the next model call; otherwise it is recorded in the task thread and the result says it was queued. High risk; only the task owner may message it.",
    json!({
        "type": "object",
        "properties": {
            "task_id": { "type": "string", "description": "Task id returned by start_task." },
            "message": { "type": "string", "description": "Message for the task, up to 8,000 characters." }
        },
        "required": ["task_id", "message"],
        "additionalProperties": false
    }),
    SkillRisk::High
);

runtime_skill!(
    CloseTaskSkill,
    "close_task",
    "Close a background task",
    "Close a task you own. Queued, starting and running tasks become cancelled and an active run is asked to stop; a terminal task is reported as already closed. A close does not invent a result. High risk and owner-only.",
    json!({
        "type": "object",
        "properties": {
            "task_id": { "type": "string", "description": "Task id returned by start_task." },
            "reason": { "type": "string", "description": "Why the task is no longer needed." }
        },
        "required": ["task_id"],
        "additionalProperties": false
    }),
    SkillRisk::High
);

runtime_skill!(
    AwaitEventSkill,
    "await_event",
    "Wait for a task event or timer",
    "Block this run until a matching task transition/message or a timer fires. `event_kind: task` accepts an exact filter over task_id, state and target_bot_id; an empty filter matches any task event owned by you. `event_kind: timer` waits for the deadline. The wait fires once, is consumed once, and returns `expired` at the timeout instead of hanging. High risk.",
    json!({
        "type": "object",
        "properties": {
            "event_kind": {
                "type": "string",
                "enum": ["task", "timer"],
                "description": "task or timer."
            },
            "filter": {
                "type": "object",
                "description": "Exact-match task fields: task_id, state (queued/starting/running/completed/failed/cancelled), target_bot_id. Empty for a timer."
            },
            "timeout_ms": {
                "type": "integer",
                "minimum": 1000,
                "maximum": 600000,
                "description": "Maximum wait, default 60000 ms, capped at 600000 ms."
            }
        },
        "required": ["event_kind"],
        "additionalProperties": false
    }),
    SkillRisk::High
);

runtime_skill!(
    ListWaitsSkill,
    "list_waits",
    "List this run's waits",
    "List the event waits created by this run, newest first. Shows the wait id, event kind, filter, deadline, state and whether a payload is ready. It never shows another run's waits. High risk.",
    json!({
        "type": "object",
        "properties": {
            "include_finished": {
                "type": "boolean",
                "description": "Also include consumed, expired and cancelled waits. Default false."
            }
        },
        "additionalProperties": false
    }),
    SkillRisk::High
);

runtime_skill!(
    CloseWaitSkill,
    "close_wait",
    "Close this run's wait",
    "Cancel a waiting or fired wait owned by this run. The blocked `await_event` returns `cancelled` instead of waiting for the deadline. High risk.",
    json!({
        "type": "object",
        "properties": {
            "wait_id": { "type": "string", "description": "Wait id from await_event or list_waits." },
            "reason": { "type": "string", "description": "Why the wait is no longer needed." }
        },
        "required": ["wait_id"],
        "additionalProperties": false
    }),
    SkillRisk::High
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::Skill;

    #[test]
    fn every_task_tool_is_high_risk_and_has_a_schema() {
        let tools: Vec<Box<dyn Skill>> = vec![
            Box::new(StartTaskSkill::new()),
            Box::new(MessageTaskSkill::new()),
            Box::new(CloseTaskSkill::new()),
            Box::new(AwaitEventSkill::new()),
            Box::new(ListWaitsSkill::new()),
            Box::new(CloseWaitSkill::new()),
        ];
        for tool in tools {
            assert_eq!(tool.risk(), SkillRisk::High, "{}", tool.id());
            assert_eq!(tool.input_schema()["type"], "object", "{}", tool.id());
            assert!(!tool.description().is_empty(), "{}", tool.id());
        }
    }

    #[test]
    fn the_schemas_do_not_carry_secrets_or_extra_fields() {
        for schema in [
            StartTaskSkill::new().input_schema(),
            MessageTaskSkill::new().input_schema(),
            CloseTaskSkill::new().input_schema(),
            AwaitEventSkill::new().input_schema(),
            ListWaitsSkill::new().input_schema(),
            CloseWaitSkill::new().input_schema(),
        ] {
            let text = serde_json::to_string(&schema).unwrap();
            assert!(!text.contains("secret"));
            assert!(!text.contains("token"));
            assert_eq!(schema["additionalProperties"], false);
        }
    }

    #[tokio::test]
    async fn standalone_execution_fails_honestly() {
        let ctx = SkillContext::with_default_tier(
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
        );
        let error = StartTaskSkill::new()
            .execute(&ctx, json!({"target": "Target", "instruction": "work"}))
            .await
            .expect_err("standalone");
        assert!(error.to_string().contains("task store"));
    }
}
