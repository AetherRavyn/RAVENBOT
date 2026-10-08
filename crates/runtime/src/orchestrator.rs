//! Office orchestration.
//!
//! "Offices" are teams of bots. The previous implementation routed by keyword
//! matching or broadcast the same task to everyone. This module replaces that
//! with a real planner: a lead bot is asked to produce a small JSON task plan
//! (who does what, in what order), which is then executed as a DAG and
//! summarized by the lead.
//!
//! Planning and synthesis are best-effort: if the model fails or returns
//! unparseable output, the caller falls back to a simple fan-out so an office
//! always does *something* useful.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A bot available to work in an office.
#[derive(Debug, Clone)]
pub struct OfficeMember {
    pub bot_id: Uuid,
    pub name: String,
    pub rank: String,
    pub specialty: String,
}

/// One task in a planned office run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlannedTask {
    /// Assignee: bot name (preferred) or bot id string.
    pub bot: String,
    pub instruction: String,
    /// Indices (into the same `tasks` array) this task depends on.
    #[serde(default)]
    pub depends_on: Vec<usize>,
}

/// A parsed office plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OfficePlan {
    #[serde(default)]
    pub tasks: Vec<PlannedTask>,
    /// Optional bot responsible for the final synthesis.
    #[serde(default)]
    pub final_summary_from: Option<String>,
    /// When set, the lead needs information from the client before it can
    /// plan. The caller posts this question and waits for the answer instead
    /// of running the office — the CEO/client clarification loop.
    #[serde(default)]
    pub question: Option<String>,
}

impl OfficePlan {
    /// Whether this plan is a clarification request rather than a task plan.
    pub fn is_clarification(&self) -> bool {
        self.question
            .as_deref()
            .map(|q| !q.trim().is_empty())
            .unwrap_or(false)
    }

    /// Cap a plan to a sane size so one bad roll can't spawn 50 runs.
    pub fn sanitized(mut self, max_tasks: usize) -> Self {
        self.tasks.truncate(max_tasks);
        // Drop dependency indices that fall outside the truncated plan and
        // guard against self/forward references (which would deadlock the DAG).
        let len = self.tasks.len();
        for i in 0..len {
            let mut deps: Vec<usize> = self.tasks[i]
                .depends_on
                .iter()
                .copied()
                .filter(|d| *d < len && *d < i)
                .collect();
            deps.sort_unstable();
            deps.dedup();
            self.tasks[i].depends_on = deps;
        }
        self
    }
}

/// Build the planner prompt from the office context.
pub fn plan_prompt(
    office_goal: Option<&str>,
    office_policy: Option<&str>,
    members: &[OfficeMember],
    request: &str,
) -> String {
    let roster: Vec<String> = members
        .iter()
        .map(|m| {
            let mut line = format!("- {} (rank: {}, specialty: {})", m.name, m.rank, m.specialty);
            if line.trim().is_empty() {
                line = format!("- {}", m.name);
            }
            line
        })
        .collect();

    let mut prompt = String::from(
        "You are the lead orchestrator of an AI team. Break the user's request into a small number of concrete subtasks and assign each to the best-suited teammate.\n\n",
    );
    if let Some(goal) = office_goal.filter(|g| !g.trim().is_empty()) {
        prompt.push_str(&format!("Team objective: {}\n", goal));
    }
    if let Some(policy) = office_policy.filter(|p| !p.trim().is_empty()) {
        prompt.push_str(&format!("Team standards: {}\n", policy));
    }
    prompt.push_str("Teammates:\n");
    prompt.push_str(&roster.join("\n"));
    prompt.push_str("\n\n");
    prompt.push_str(&format!("User request:\n{}\n\n", request));
    prompt.push_str(
        "First, decide whether you have enough information to plan. If the request is ambiguous, or a missing detail (target audience, scope, format, deadline, constraints) would materially change the work, ASK the client instead of guessing.\n\
         If you must ask, respond with ONLY: {\"question\":\"<one clear, specific question>\"}\n\
         Otherwise respond with ONLY a JSON object, no prose and no code fence, in exactly this shape:\n\
         {\"tasks\":[{\"bot\":\"<teammate name>\",\"instruction\":\"<self-contained task>\",\"depends_on\":[]}],\"final_summary_from\":\"<teammate name>\"}\n\
         Rules:\n\
         - Use at most 6 tasks; \"bot\" must be one of the teammate names above.\n\
         - \"depends_on\" is a list of task indices (0-based) that must finish first, so a teammate can build on or review another's output. Independent tasks leave it empty so they run in parallel.\n\
         - Match the task to the teammate's rank/specialty: planners plan, builders implement, testers write/run tests, QA verifies. Never assign specialist work to the lead.\n\
         - For any request that produces or changes an artifact (code, document, design, campaign), include an explicit verification task (tester/QA) that DEPENDS ON the producing task, and a final review/acceptance step.\n\
         - Prefer the natural pipeline: plan/design → implement → test → QA/verify → synthesize.\n\
         - Set \"final_summary_from\" to the teammate who should write the final answer (usually yourself, the lead).\n\
         - Prefer 1-3 tasks for simple requests; do not invent busywork for greetings or chit-chat — answer those with a single task to yourself.\n",
    );
    prompt
}

/// Build the synthesis prompt from completed task outputs.
pub fn synthesis_prompt(request: &str, results: &[(String, String)]) -> String {
    let mut prompt = String::from(
        "You are the lead of an AI team. Your teammates completed the subtasks below for the user's request. Write the single best final answer now, integrating their work into one coherent response. Do not mention the internal task breakdown unless it helps the user.\n\n",
    );
    prompt.push_str(&format!("User request:\n{}\n\nTeammate results:\n", request));
    for (label, output) in results {
        prompt.push_str(&format!("\n### {}\n{}\n", label, output.trim()));
    }
    prompt
}

/// Extract an `OfficePlan` from raw model text. Tolerant of code fences and
/// surrounding prose; returns `None` when no valid plan can be recovered.
pub fn parse_plan(text: &str) -> Option<OfficePlan> {
    let json = extract_json_object(text)?;
    let plan: OfficePlan = serde_json::from_str(&json).ok()?;
    if plan.is_clarification() {
        // The lead needs client input; tasks may legitimately be empty.
        return Some(plan);
    }
    if plan.tasks.is_empty() {
        return None;
    }
    Some(plan.sanitized(6))
}

/// Find the first balanced `{...}` object in `text`.
fn extract_json_object(text: &str) -> Option<String> {
    // Strip a ```json ... ``` fence if present.
    let cleaned = if let Some(start) = text.find("```") {
        let after = &text[start + 3..];
        let after = after.strip_prefix("json").unwrap_or(after);
        match after.find("```") {
            Some(end) => after[..end].to_string(),
            None => after.to_string(),
        }
    } else {
        text.to_string()
    };

    let bytes = cleaned.as_bytes();
    let start = cleaned.find('{')?;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(cleaned[start..=i].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

/// Resolve a plan task's assignee string to a member (by name, case-insensitive,
/// then by id).
pub fn resolve_member<'a>(members: &'a [OfficeMember], who: &str) -> Option<&'a OfficeMember> {
    let who = who.trim();
    members
        .iter()
        .find(|m| m.name.eq_ignore_ascii_case(who))
        .or_else(|| {
            who.parse::<Uuid>()
                .ok()
                .and_then(|id| members.iter().find(|m| m.bot_id == id))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn members() -> Vec<OfficeMember> {
        vec![
            OfficeMember {
                bot_id: Uuid::new_v4(),
                name: "Chief of Staff".into(),
                rank: "lead".into(),
                specialty: "Coordination".into(),
            },
            OfficeMember {
                bot_id: Uuid::new_v4(),
                name: "Growth Marketer".into(),
                rank: "specialist".into(),
                specialty: "Marketing".into(),
            },
        ]
    }

    #[test]
    fn parses_a_plain_json_plan() {
        let text = r#"{"tasks":[{"bot":"Growth Marketer","instruction":"Draft the campaign","depends_on":[]},{"bot":"Chief of Staff","instruction":"Review it","depends_on":[0]}],"final_summary_from":"Chief of Staff"}"#;
        let plan = parse_plan(text).expect("plan");
        assert_eq!(plan.tasks.len(), 2);
        assert_eq!(plan.tasks[1].depends_on, vec![0]);
        assert_eq!(plan.final_summary_from.as_deref(), Some("Chief of Staff"));
    }

    #[test]
    fn tolerates_code_fences_and_prose() {
        let text = "Here is the plan:\n```json\n{\"tasks\":[{\"bot\":\"Growth Marketer\",\"instruction\":\"x\",\"depends_on\":[]}]}\n```\nDone.";
        let plan = parse_plan(text).expect("plan");
        assert_eq!(plan.tasks.len(), 1);
    }

    #[test]
    fn rejects_empty_or_garbage() {
        assert!(parse_plan("no json here").is_none());
        assert!(parse_plan("{\"tasks\":[]}").is_none());
    }

    #[test]
    fn parses_a_clarifying_question() {
        let plan = parse_plan(r#"{"question":"Which audience should the launch target?"}"#)
            .expect("clarification plan");
        assert!(plan.is_clarification());
        assert_eq!(
            plan.question.as_deref(),
            Some("Which audience should the launch target?")
        );
        assert!(plan.tasks.is_empty());
    }

    #[test]
    fn sanitize_drops_bad_dependencies() {
        let plan = OfficePlan {
            tasks: vec![
                PlannedTask { bot: "a".into(), instruction: "x".into(), depends_on: vec![] },
                PlannedTask { bot: "b".into(), instruction: "y".into(), depends_on: vec![5, 1, 0, 0] },
            ],
            final_summary_from: None,
            question: None,
        }
        .sanitized(6);
        // index 5 is out of range, index 1 is self/forward → keep only 0, deduped.
        assert_eq!(plan.tasks[1].depends_on, vec![0]);
    }

    #[test]
    fn resolve_by_name_and_id() {
        let ms = members();
        assert_eq!(
            resolve_member(&ms, "growth marketer").unwrap().name,
            "Growth Marketer"
        );
        let id = ms[0].bot_id;
        assert_eq!(resolve_member(&ms, &id.to_string()).unwrap().name, "Chief of Staff");
        assert!(resolve_member(&ms, "Nobody").is_none());
    }

    #[test]
    fn plan_prompt_lists_roster_and_request() {
        let prompt = plan_prompt(Some("Ship Q3"), Some("Be concise"), &members(), "Launch a blog");
        assert!(prompt.contains("Chief of Staff"));
        assert!(prompt.contains("Growth Marketer"));
        assert!(prompt.contains("Launch a blog"));
        assert!(prompt.contains("Ship Q3"));
    }
}
