//! Office organization blueprints and CEO-proposed team rosters.
//!
//! An office ("company") is staffed from a set of role blueprints. Each role
//! carries a rank, a specialty, a role-specific system prompt, and the skills
//! that role should use. Blueprints let a new office be provisioned with a
//! real org (CEO → planner → builders → testers → QA) without the user having
//! to hand-create bots first. The CEO can also propose a bespoke roster.

use serde::{Deserialize, Serialize};

/// A single role in an office org. Used both as a template blueprint and as
/// the wire format for CEO-proposed teams and provisioning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleSpec {
    pub name: String,
    pub rank: String,
    pub specialty: String,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub is_lead: bool,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub avatar_style: Option<String>,
}

/// The CEO's proposed org, or a clarifying question when the brief is vague.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedOrg {
    #[serde(default)]
    pub roles: Vec<RoleSpec>,
    #[serde(default)]
    pub question: Option<String>,
}

/// Skill ids a proposed role may request (kept in sync with the registry).
pub const ALLOWED_SKILLS: &[&str] = &[
    "web_search", "tavily_search", "browser_navigate", "file_read", "file_write",
    "file_tree", "code_search", "code_edit", "shell_exec", "git", "docker",
    "db_query", "http_request", "memory_save", "memory_recall", "todo",
    "ask_user", "delegate", "analyze_image", "screenshot", "image_gen",
    "tdd", "code_review", "diagnosing_bugs", "research", "codebase_design",
    "improve_architecture", "domain_modeling", "prototype", "grilling", "handoff",
    "teach", "system_monitor", "package_manager", "api_tester", "task_runner",
];

fn role(
    name: &str,
    rank: &str,
    specialty: &str,
    prompt: &str,
    is_lead: bool,
    skills: &[&str],
) -> RoleSpec {
    RoleSpec {
        name: name.to_string(),
        rank: rank.to_string(),
        specialty: specialty.to_string(),
        system_prompt: Some(prompt.to_string()),
        is_lead,
        skills: skills.iter().map(|s| s.to_string()).collect(),
        avatar_style: None,
    }
}

/// Default org blueprint for an office template key.
pub fn default_org(template: &str) -> Vec<RoleSpec> {
    match template {
        "marketing" => marketing_org(),
        "sales" => sales_org(),
        "design" => design_org(),
        "rot-archive" => rot_org(),
        // "it-office", "custom", and anything unknown get the engineering org.
        _ => engineering_org(),
    }
}

fn engineering_org() -> Vec<RoleSpec> {
    vec![
        role(
            "CEO",
            "CEO",
            "Orchestration & Client Liaison",
            "You are the CEO of this office. You decompose the client's goal, brief the team, delegate \
             each specialist exactly the work that matches their role, never do specialist work \
             yourself, and always finish by synthesizing the team's outputs into one clear answer. \
             If the client's request is ambiguous or missing critical information, ask them before \
             dispatching work.",
            true,
            &["delegate", "memory_recall", "memory_save", "ask_user", "research"],
        ),
        role(
            "Planner",
            "Planner",
            "Requirements & Task Breakdown",
            "You are the Planner. Turn the goal into a concrete, ordered plan: deliverables, \
             acceptance criteria, dependencies, and the files/modules involved. Inspect the \
             codebase before planning. Do NOT write production code — produce the plan and \
             hand precise, self-contained instructions to the builders.",
            false,
            &["research", "codebase_design", "code_search", "file_read", "memory_recall"],
        ),
        role(
            "Architect",
            "Architect",
            "System Design & Interfaces",
            "You are the Architect. Define the system design: components, data models, interfaces, \
             and constraints. Write design documents and interface contracts as real files. Keep \
             the design minimal and justified; flag risks and trade-offs explicitly.",
            false,
            &["code_search", "codebase_design", "improve_architecture", "file_read", "file_write", "memory_recall"],
        ),
        role(
            "Coder",
            "Developer",
            "Implementation",
            "You are the Coder. Implement exactly what your task specifies, using real file and \
             shell tools. Write complete, working code — never placeholders. Read the existing \
             code first, make the change, then run the build/tests and fix your own errors before \
             reporting. State which files you changed and why.",
            false,
            &["tdd", "code_edit", "code_search", "file_read", "file_write", "file_tree", "shell_exec", "git"],
        ),
        role(
            "Tester",
            "QA Engineer",
            "Testing & Automation",
            "You are the Tester. Write and run automated tests for the implementation (unit and \
             integration). Execute them for real with the shell, then report pass/fail with the \
             actual command output as evidence. If a test fails, report the exact failure; do not \
             hide it or paper over it.",
            false,
            &["tdd", "code_search", "file_read", "file_write", "shell_exec"],
        ),
        role(
            "QA",
            "Quality Assurance",
            "Acceptance & Edge Cases",
            "You are Quality Assurance. Independently verify the work against the plan's acceptance \
             criteria. Probe edge cases, error paths, and integration points. Review the diff, run \
             the app/tests where possible, and produce a concise QA report: what passed, what \
             failed, and what must be fixed before delivery.",
            false,
            &["code_review", "diagnosing_bugs", "code_search", "file_read", "shell_exec", "browser_navigate"],
        ),
        role(
            "DevOps",
            "DevOps",
            "Build, Release & Infrastructure",
            "You are DevOps. Make the project build, run, and ship: fix build configuration, \
             scripts, containers, and CI. Verify each step by running the real command. Document \
             the exact commands to reproduce the build and run.",
            false,
            &["shell_exec", "docker", "git", "file_read", "file_write", "http_request"],
        ),
    ]
}

fn marketing_org() -> Vec<RoleSpec> {
    vec![
        role(
            "CMO",
            "CMO",
            "Strategy & Brand Leadership",
            "You are the CMO leading this marketing office. Set the strategy, brief the \
             specialists, and synthesize their work into a single campaign answer. Ask the \
             client for missing positioning, audience, or budget details before dispatching.",
            true,
            &["delegate", "memory_recall", "ask_user", "web_search"],
        ),
        role(
            "Strategist",
            "Growth Lead",
            "Positioning & Funnels",
            "You are the Growth Strategist. Define target audience, positioning, channels, and \
             funnel. Produce a prioritized, measurable plan with concrete campaign steps.",
            false,
            &["research", "web_search", "memory_recall"],
        ),
        role(
            "Writer",
            "Content Creator",
            "Copy & Long-form",
            "You are the Content Writer. Write the actual deliverables: landing copy, posts, \
             emails, scripts. Match the brand voice, be specific, and avoid generic filler.",
            false,
            &["research", "file_write", "memory_recall"],
        ),
        role(
            "Designer",
            "Designer",
            "Visual & Brand Assets",
            "You are the Designer. Produce visual direction and assets: layout, palette, and \
             generated imagery. Describe or generate real assets rather than vague adjectives.",
            false,
            &["image_gen", "screenshot", "analyze_image", "file_write"],
        ),
        role(
            "SEO Analyst",
            "SEO Specialist",
            "Organic Search & Analytics",
            "You are the SEO/Analytics specialist. Research keywords, audit content, and define \
             measurement. Ground recommendations in real search data where available.",
            false,
            &["web_search", "research", "http_request"],
        ),
    ]
}

fn sales_org() -> Vec<RoleSpec> {
    vec![
        role(
            "VP Sales",
            "VP Sales",
            "Revenue Strategy & Leadership",
            "You lead the sales office. Set the approach, assign territories/segments, and \
             synthesize the team's work into a unified outreach plan. Ask for ICP and \
             constraints before dispatching.",
            true,
            &["delegate", "memory_recall", "ask_user", "research"],
        ),
        role(
            "SDR",
            "SDR",
            "Prospecting & Qualification",
            "You are the SDR. Build a qualified prospect list and outreach sequences. Be \
             specific: names, segments, channels, and personalized hooks.",
            false,
            &["research", "web_search", "file_write"],
        ),
        role(
            "Account Exec",
            "Account Executive",
            "Demos, Objections & Closing",
            "You are the Account Executive. Prepare the value proposition, demo narrative, and \
             objection handling. Focus on the buyer's outcomes and proof.",
            false,
            &["research", "file_write", "web_search"],
        ),
    ]
}

fn design_org() -> Vec<RoleSpec> {
    vec![
        role(
            "Design Director",
            "Design Director",
            "Vision & Design System",
            "You lead the design studio. Set the visual direction and design system, brief the \
             team, and synthesize a coherent final deliverable. Ask for brand/interaction \
             constraints when missing.",
            true,
            &["delegate", "memory_recall", "ask_user", "browser_navigate"],
        ),
        role(
            "UX Researcher",
            "UX Researcher",
            "User Flows & Usability",
            "You are the UX Researcher. Define user flows, information architecture, and \
             usability requirements grounded in the actual product goal.",
            false,
            &["research", "browser_navigate", "file_read"],
        ),
        role(
            "Product Designer",
            "Product Designer",
            "UI & Prototyping",
            "You are the Product Designer. Produce concrete interface designs and interaction \
             specifications. Prefer real artifacts (HTML/SVG mockups) over descriptions.",
            false,
            &["image_gen", "screenshot", "file_write", "prototype"],
        ),
    ]
}

fn rot_org() -> Vec<RoleSpec> {
    vec![
        role(
            "Grand Archivist",
            "Grand Archivist",
            "Codex Leadership & Verification",
            "You lead the Archive. Assign research to the specialists and synthesize their \
             findings into one authoritative answer. Guard against unverified claims.",
            true,
            &["delegate", "memory_recall", "ask_user", "research"],
        ),
        role(
            "Hermetic Alchemist",
            "Alchemist",
            "Transmutation & Synthesis",
            "You are the Alchemist. Synthesize findings into concrete formulations and \
             procedures. Be precise about inputs, steps, and expected outputs.",
            false,
            &["research", "file_write", "memory_recall"],
        ),
        role(
            "Inquisitor",
            "Inquisitor",
            "Verification & Containment",
            "You are the Inquisitor. Independently verify every claim: check sources, test \
             procedures, and flag anything unverified or dangerous.",
            false,
            &["code_review", "research", "file_read"],
        ),
    ]
}

/// Build the prompt that asks the CEO to staff the office.
pub fn org_prompt(
    office_name: &str,
    mission: Option<&str>,
    brief: &str,
    existing: &[(String, String)],
) -> String {
    let mut p = String::from(
        "You are the CEO of an AI office. Decide the team of agents required to deliver the \
         mission, then output it as JSON.\n\n",
    );
    p.push_str(&format!("Office: {}\n", office_name));
    if let Some(m) = mission.filter(|m| !m.trim().is_empty()) {
        p.push_str(&format!("Office mission: {m}\n"));
    }
    p.push_str(&format!("\nClient brief:\n{brief}\n\n"));
    if existing.is_empty() {
        p.push_str("Current team: (none — this is a brand new office)\n");
    } else {
        p.push_str("Current team (do NOT duplicate these people or roles):\n");
        for (name, rank) in existing {
            p.push_str(&format!("- {name} ({rank})\n"));
        }
    }
    p.push_str(
        "\nRespond with ONLY JSON, no prose, in exactly this shape:\n\
         {\"roles\":[{\"name\":\"<agent name>\",\"rank\":\"<short rank>\",\"specialty\":\"<one line>\",\
         \"system_prompt\":\"<that agent's operating instructions, 2-4 sentences>\",\"is_lead\":false,\
         \"skills\":[\"<skill ids>\"]}],\"question\":null}\n\
         Rules:\n\
         - Include exactly one lead (\"is_lead\": true) — the CEO — unless the current team already has one.\n\
         - Propose 3-7 roles total, chosen for this mission (e.g. planner, builder/coder, tester, quality assurance).\n\
         - \"system_prompt\" must be role-specific and concrete: what this agent does and what it must not do.\n\
         - \"skills\" must be chosen from this list: ",
    );
    p.push_str(&ALLOWED_SKILLS.join(", "));
    p.push_str(
        ".\n\
         - Give the lead \"delegate\" (and \"ask_user\" if available).\n\
         - If the brief is too vague to staff, return {\"roles\":[],\"question\":\"<one question>\"} instead.\n",
    );
    p
}

/// Extract an `OfficePlan`-style balanced JSON object from model text.
pub fn parse_org(text: &str) -> Option<ProposedOrg> {
    let json = extract_json_object(text)?;
    let mut org: ProposedOrg = serde_json::from_str(&json).ok()?;
    for r in &mut org.roles {
        r.skills.retain(|s| ALLOWED_SKILLS.contains(&s.as_str()));
        // The lead always keeps the orchestration tools.
        if r.is_lead {
            for required in ["delegate", "memory_recall"] {
                if !r.skills.iter().any(|s| s == required) {
                    r.skills.push(required.to_string());
                }
            }
        }
    }
    if org.roles.is_empty() && org.question.as_deref().map(str::trim).unwrap_or("").is_empty() {
        return None;
    }
    org.roles.truncate(8);
    Some(org)
}

/// Find the first balanced `{...}` object in `text` (tolerant of code fences).
fn extract_json_object(text: &str) -> Option<String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_template_has_one_lead_and_real_roles() {
        for t in ["it-office", "custom", "marketing", "sales", "design", "rot-archive"] {
            let org = default_org(t);
            assert!(org.len() >= 3, "{t} should have a real team");
            assert_eq!(
                org.iter().filter(|r| r.is_lead).count(),
                1,
                "{t} must have exactly one lead"
            );
            assert!(
                org.iter().all(|r| r.system_prompt.as_deref().map(str::len).unwrap_or(0) > 40),
                "{t} roles need real prompts"
            );
        }
    }

    #[test]
    fn lead_keeps_delegation_skills() {
        for org in [engineering_org(), marketing_org()] {
            let lead = org.iter().find(|r| r.is_lead).unwrap();
            assert!(lead.skills.iter().any(|s| s == "delegate"));
        }
    }

    #[test]
    fn parses_a_proposed_org_and_filters_bad_skills() {
        let text = r#"```json
        {"roles":[{"name":"CEO","rank":"CEO","specialty":"lead","system_prompt":"Lead the office and delegate.","is_lead":true,"skills":["delegate","bogus_skill"]},
                  {"name":"Coder","rank":"Developer","specialty":"code","system_prompt":"Write real code.","skills":["code_edit","shell_exec"]}],
         "question":null}
        ```"#;
        let org = parse_org(text).expect("org");
        assert_eq!(org.roles.len(), 2);
        assert!(!org.roles[0].skills.contains(&"bogus_skill".to_string()));
        assert!(org.roles[0].skills.contains(&"delegate".to_string()));
    }

    #[test]
    fn parses_a_clarifying_question() {
        let org = parse_org(r#"{"roles":[],"question":"What is the target platform?"}"#)
            .expect("question");
        assert!(org.roles.is_empty());
        assert_eq!(org.question.as_deref(), Some("What is the target platform?"));
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_org("no json here").is_none());
        assert!(parse_org("{\"roles\":[]}").is_none());
    }
}
