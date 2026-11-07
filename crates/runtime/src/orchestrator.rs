//! Office orchestration.
//!
//! "Offices" are teams of bots. The previous implementation routed by keyword
//! matching or broadcast the same task to everyone. This module replaces that
//! with a real planner: a lead bot is asked to produce a small JSON task plan
//! (who does what, in what order), which is then executed as a DAG and
//! summarized by the lead.
//!
//! Planning and synthesis are best-effort: if the model fails or returns
//! unparseable output, the caller uses a deterministic role-aware plan so an
//! office always does something useful — without every member receiving the
//! same instruction.

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
            let mut line = format!(
                "- {} (rank: {}, specialty: {})",
                m.name, m.rank, m.specialty
            );
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
         - Prefer 1-3 tasks for simple requests; do not invent busywork for greetings or chit-chat — answer those with a single task to yourself.\n\
         - Every teammate starts with no memory of this conversation. Each instruction must name the files to read, the artifact to produce, and how to prove it.\n\
         - Name only files that exist or that the task explicitly asks to create. If a referenced file may be missing, tell the teammate to inspect the workspace tree with file_tree and adapt — never to search the whole machine.\n\
         - If the workspace snapshot says Implementation is MISSING, the first task MUST create the minimal runnable skeleton (entrypoint, one module, one test, manifest). Do not assign another read-only inspection or verification task before code exists.\n\
         - If the last cycle ended blocked or partial and the snapshot is unchanged, do not repeat the same plan. Change the approach or assign the smallest concrete file-creating step.\n\
         - Keep each task achievable in roughly 20 tool rounds. Split a large deliverable into a task per artifact rather than one open-ended instruction.\n\
         - If the request is vague (for example \"do next work\"), use the team objective and the office's Shared/STATUS.md to assign distinct work. Never give two teammates the same instruction.\n\
         - Do not assign work that repeats a command or file already marked done in Shared/STATUS.md.\n",
    );
    prompt
}

/// A deterministic, role-aware plan used when the lead's model cannot produce
/// a parseable one.
///
/// The old fallback broadcast the same instruction to every member. With a
/// vague request ("do next work") every agent explored the same files, ran the
/// same commands, and repeated each other. This plan gives the lead the first
/// move — write the milestone and owners into `Shared/STATUS.md` — then assigns
/// each specialist a distinct, specialty-scoped task that depends on it. The
/// graph still runs specialists in parallel; they share state through the
/// status file rather than through duplicated exploration.
pub fn fallback_plan(
    office_goal: Option<&str>,
    request: &str,
    members: &[OfficeMember],
    lead_id: Uuid,
) -> OfficePlan {
    let goal = office_goal
        .map(str::trim)
        .filter(|g| !g.is_empty())
        .unwrap_or_else(|| request.trim());
    let Some(lead) = members
        .iter()
        .find(|m| m.bot_id == lead_id)
        .or_else(|| members.first())
    else {
        return OfficePlan {
            tasks: Vec::new(),
            final_summary_from: None,
            question: None,
        };
    };

    let request = request.trim();
    let mut tasks = vec![PlannedTask {
        bot: lead.name.clone(),
        instruction: format!(
            "[Office lead — {}: {}]\n\
             Client request: {request}\n\
             Office goal: {goal}\n\n\
             Read OFFICE.md, POLICY.md, and Shared/STATUS.md in your workspace first. \
             Decide the next concrete milestone for this office: the deliverable, its \
             acceptance criteria, and which teammate owns which part. Write that \
             milestone to Shared/STATUS.md. Do not do specialist work. Report the \
             milestone, the owners, and the exact files you read.",
            lead.rank, lead.specialty
        ),
        depends_on: Vec::new(),
    }];

    for member in members.iter().filter(|m| m.bot_id != lead.bot_id).take(4) {
        tasks.push(PlannedTask {
            bot: member.name.clone(),
            instruction: format!(
                "[{} — {}]\n\
                 Client request: {request}\n\
                 Office goal: {goal}\n\n\
                 Read OFFICE.md, POLICY.md, and Shared/STATUS.md first. If the lead \
                 has written a milestone there, follow it; otherwise work from the \
                 office goal. If the workspace has no implementation directory or \
                 manifest, create the smallest runnable skeleton for your specialty \
                 now (entrypoint + one module + one test); do not spend the turn \
                 reading docs. Otherwise find the next open item in your specialty \
                 that is not already marked done. Produce one concrete artifact for it \
                 in deliverables/ and update Shared/STATUS.md with the artifact path \
                 and a one-line result. Do not rerun a command or reread a file that \
                 STATUS says is already done. Do not do another teammate's job. If you \
                 are blocked, write the blocker to Shared/STATUS.md and stop. Stay \
                 inside the office workspace.",
                member.rank, member.specialty
            ),
            // Deliberately independent: a provider failure on the lead must not
            // skip every specialist. Coordination is through Shared/STATUS.md,
            // which the lead writes and the specialists read.
            depends_on: Vec::new(),
        });
    }

    OfficePlan {
        tasks,
        final_summary_from: Some(lead.name.clone()),
        question: None,
    }
}

/// A deterministic, creation-first plan for an office with no implementation.
///
/// The symptom this exists for: every heartbeat re-read the same design docs,
/// hit the tool-round limit, and produced no file. The planner was obeying the
/// conversation ("inspect first") instead of the workspace ("there is nothing
/// to inspect"). This plan makes writing the first artifact the first task and
/// the verification dependent on it.
pub fn bootstrap_plan(goal: &str, members: &[OfficeMember], lead_id: Uuid) -> OfficePlan {
    let Some(lead) = members
        .iter()
        .find(|member| member.bot_id == lead_id)
        .or_else(|| members.first())
    else {
        return OfficePlan {
            tasks: Vec::new(),
            final_summary_from: None,
            question: None,
        };
    };
    let goal = goal.trim();
    let goal = if goal.chars().count() > 300 {
        let mut short: String = goal.chars().take(300).collect();
        short.push('…');
        short
    } else {
        goal.to_string()
    };
    let specialists: Vec<&OfficeMember> = members
        .iter()
        .filter(|member| member.bot_id != lead.bot_id)
        .collect();

    let create = |member: &OfficeMember, first: bool| PlannedTask {
        bot: member.name.clone(),
        instruction: if first {
            format!(
                "[Bootstrap — create files first]\n                 Office goal: {goal}\n\n                 Create the minimal runnable skeleton for this goal in the project root NOW.                  Use the stack named in the goal and the one design document that names it.                  At minimum: the dependency manifest, one entrypoint, one module, and one                  smoke test. Read only Shared/STATUS.md and at most ONE design document; do                  not inspect the whole workspace. Write the first file within your first four                  tool rounds. Run the import/smoke test if the environment allows it. Update                  Shared/STATUS.md with the exact paths and result. If a dependency is missing,                  install it with the project's package manager."
            )
        } else {
            format!(
                "[Bootstrap — first specialty module]\n                 Office goal: {goal}\n                 Specialty: {specialty}\n\n                 The skeleton task created the project structure. Add the first concrete module                  for your specialty and one test for it. Read only the skeleton and at most ONE                  design document; write files early. Update Shared/STATUS.md with the path and                  the test result. If the skeleton is missing or broken, report that blocker                  instead of starting another inspection.",
                specialty = member.specialty
            )
        },
        depends_on: if first { Vec::new() } else { vec![0] },
    };

    let mut tasks = Vec::new();
    if specialists.is_empty() {
        tasks.push(create(lead, true));
        tasks.push(PlannedTask {
            bot: lead.name.clone(),
            instruction: format!(
                "[Verify the bootstrap]\n                 Run the smoke test/import from the project root and read only the files the                  bootstrap created. If it fails, fix the smallest defect or write the exact                  blocker. Update Shared/STATUS.md with the command, output, and remaining work.                  Do not audit unrelated docs. Office goal: {goal}"
            ),
            depends_on: vec![0],
        });
    } else {
        tasks.push(create(specialists[0], true));
        if let Some(second) = specialists.get(1) {
            tasks.push(create(second, false));
        }
        let producer_count = tasks.len();
        tasks.push(PlannedTask {
            bot: lead.name.clone(),
            instruction: format!(
                "[Verify the bootstrap]\n                 Run the smoke test/import from the project root and read only the files the                  team created. If it fails, fix the smallest defect or write the exact blocker.                  Update Shared/STATUS.md with the command, output, and remaining work. Do not                  audit unrelated docs. Office goal: {goal}"
            ),
            depends_on: (0..producer_count).collect(),
        });
    }

    OfficePlan {
        tasks,
        final_summary_from: Some(lead.name.clone()),
        question: None,
    }
}

/// Build the synthesis prompt from completed task outputs.
pub fn synthesis_prompt(request: &str, results: &[(String, String)]) -> String {
    let mut prompt = String::from(
        "You are the lead of an AI team. Your teammates worked on the subtasks below for the user's request. Write the single best final answer now, integrating what is complete into one coherent response. Some work may be incomplete, blocked, or failed; say so plainly instead of implying completion. Do not mention the internal task breakdown unless it helps the user.\n\n",
    );
    prompt.push_str(&format!(
        "User request:\n{}\n\nTeammate results:\n",
        request
    ));
    for (label, output) in results {
        prompt.push_str(&format!("\n### {}\n{}\n", label, output.trim()));
    }
    prompt
}

/// Extract an `OfficePlan` from raw model text. Tolerant of code fences,
/// surrounding prose, and a response truncated mid-task.
pub fn parse_plan(text: &str) -> Option<OfficePlan> {
    if let Some(json) = extract_json_object(text) {
        if let Ok(plan) = serde_json::from_str::<OfficePlan>(&json) {
            if plan.is_clarification() {
                // The lead needs client input; tasks may legitimately be empty.
                return Some(plan);
            }
            if !plan.tasks.is_empty() {
                return Some(plan.sanitized(6));
            }
        }
    }
    // A planner that writes a long rationale can hit the output-token cap
    // mid-array. The complete tasks before the cut are still a usable plan.
    parse_plan_truncated(text)
}

fn parse_plan_truncated(text: &str) -> Option<OfficePlan> {
    let question = json_string_field(text, "question");
    if question
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .is_some()
    {
        return Some(OfficePlan {
            tasks: Vec::new(),
            final_summary_from: None,
            question,
        });
    }

    let mut tasks = plan_tasks_from_truncated(text);
    if tasks.is_empty() {
        return None;
    }
    tasks.truncate(6);
    let plan = OfficePlan {
        tasks,
        final_summary_from: json_string_field(text, "final_summary_from"),
        question: None,
    };
    Some(plan.sanitized(6))
}

/// Read a JSON string field without requiring the surrounding object to close.
fn json_string_field(text: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{key}\"");
    let start = text.find(&pattern)?;
    let after_key = &text[start + pattern.len()..];
    let colon = after_key.find(':')?;
    let rest = after_key[colon + 1..].trim_start();
    let rest = rest.strip_prefix('"')?;
    let mut out = String::new();
    let mut escaped = false;
    for c in rest.chars() {
        if escaped {
            out.push(match c {
                'n' => '\n',
                't' => '\t',
                'r' => '\r',
                other => other,
            });
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' {
            return Some(out);
        } else {
            out.push(c);
        }
    }
    None
}

/// Collect every complete task object from a possibly truncated `tasks` array.
fn plan_tasks_from_truncated(text: &str) -> Vec<PlannedTask> {
    let Some(key) = text.find("\"tasks\"") else {
        return Vec::new();
    };
    let Some(open) = text[key..].find('[') else {
        return Vec::new();
    };
    let rest = &text[key + open + 1..];

    let mut tasks = Vec::new();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut obj_start: Option<usize> = None;

    for (i, c) in rest.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' => {
                if depth == 0 {
                    obj_start = Some(i);
                }
                depth += 1;
            }
            '}' => {
                if depth == 0 {
                    continue;
                }
                depth -= 1;
                if depth == 0 {
                    if let Some(start) = obj_start.take() {
                        if let Ok(task) = serde_json::from_str::<PlannedTask>(&rest[start..=i]) {
                            tasks.push(task);
                        }
                    }
                }
            }
            ']' if depth == 0 => break,
            _ => {}
        }
    }
    tasks
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
                PlannedTask {
                    bot: "a".into(),
                    instruction: "x".into(),
                    depends_on: vec![],
                },
                PlannedTask {
                    bot: "b".into(),
                    instruction: "y".into(),
                    depends_on: vec![5, 1, 0, 0],
                },
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
        assert_eq!(
            resolve_member(&ms, &id.to_string()).unwrap().name,
            "Chief of Staff"
        );
        assert!(resolve_member(&ms, "Nobody").is_none());
    }

    #[test]
    fn plan_prompt_lists_roster_and_request() {
        let prompt = plan_prompt(
            Some("Ship Q3"),
            Some("Be concise"),
            &members(),
            "Launch a blog",
        );
        assert!(prompt.contains("Chief of Staff"));
        assert!(prompt.contains("Growth Marketer"));
        assert!(prompt.contains("Launch a blog"));
        assert!(prompt.contains("Ship Q3"));
    }

    #[test]
    fn bootstrap_plan_creates_before_it_verifies() {
        let ms = members();
        let plan = bootstrap_plan("Build the checkout", &ms, ms[0].bot_id);
        assert_eq!(plan.tasks.len(), 2, "{plan:?}");
        assert_eq!(plan.tasks[0].bot, "Growth Marketer");
        assert!(plan.tasks[0]
            .instruction
            .contains("Create the minimal runnable skeleton"));
        assert_eq!(plan.tasks[0].depends_on, Vec::<usize>::new());
        assert_eq!(plan.tasks[1].bot, "Chief of Staff");
        assert!(plan.tasks[1].instruction.contains("Verify the bootstrap"));
        assert_eq!(plan.tasks[1].depends_on, vec![0]);
        assert_eq!(plan.final_summary_from.as_deref(), Some("Chief of Staff"));
    }

    #[test]
    fn fallback_plan_is_role_specific_and_resilient() {
        let ms = members();
        let plan = fallback_plan(Some("Ship Q3"), "do next work", &ms, ms[0].bot_id);
        assert_eq!(plan.tasks.len(), 2);
        assert_eq!(plan.tasks[0].bot, "Chief of Staff");
        assert!(plan.tasks[0].instruction.contains("Shared/STATUS.md"));
        assert_eq!(plan.tasks[1].bot, "Growth Marketer");
        assert!(
            plan.tasks[1].depends_on.is_empty(),
            "specialists survive a lead failure"
        );
        assert!(plan.tasks[1].instruction.contains("Ship Q3"));
        assert!(plan.tasks[1].instruction.contains("deliverables/"));
        assert_eq!(plan.final_summary_from.as_deref(), Some("Chief of Staff"));
    }

    #[test]
    fn a_truncated_plan_keeps_complete_tasks() {
        let text = r#"{"tasks":[
          {"bot":"A","instruction":"do a","depends_on":[]},
          {"bot":"B","instruction":"do b","depends_on":[0]},
          {"bot":"C","instruction":"cut off"#;
        let plan = parse_plan(text).expect("salvage the complete tasks");
        assert_eq!(plan.tasks.len(), 2);
        assert_eq!(plan.tasks[1].depends_on, vec![0]);
    }
}
