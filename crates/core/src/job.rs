//! The built-in job library.
//!
//! A job is a complete, installable piece of work: a role prompt for a new
//! agent, a routine instruction with a cadence, and an explicit oversight
//! level. Installing one creates a real agent and a **paused** routine — the
//! user still decides when it starts running unattended.
//!
//! The catalog is static and shipped with the binary. It is deliberately
//! curated rather than fetched: a job library that changes under the user is
//! another supply chain, and these entries are small enough to audit.
//!
//! Oversight maps to the agent's interactive approval mode:
//!
//! - `low` → `full` (safe work runs; the hard safety policy still refuses
//!   destructive operations)
//! - `review` → `auto` (high-risk tools park)
//! - `approval` → `ask` (every mutating tool parks)
//!
//! Unattended routine runs always go through the runtime's `Review` policy, so
//! an installed job never silently bypasses the high-risk gate just because its
//! agent was set to `full`.

use crate::approval::ApprovalMode;
use serde::Serialize;

/// The shelf a job sits on. Stable keys; the UI owns the translated labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobGroup {
    Marketing,
    Sales,
    Engineering,
    Operations,
    Finance,
    People,
    Research,
    Personal,
}

impl JobGroup {
    pub const ALL: [JobGroup; 8] = [
        JobGroup::Marketing,
        JobGroup::Sales,
        JobGroup::Engineering,
        JobGroup::Operations,
        JobGroup::Finance,
        JobGroup::People,
        JobGroup::Research,
        JobGroup::Personal,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            JobGroup::Marketing => "marketing",
            JobGroup::Sales => "sales",
            JobGroup::Engineering => "engineering",
            JobGroup::Operations => "operations",
            JobGroup::Finance => "finance",
            JobGroup::People => "people",
            JobGroup::Research => "research",
            JobGroup::Personal => "personal",
        }
    }
}

/// How often the job runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobCadence {
    /// Saved prompt; run it when you want with **Run now**.
    OneOff,
    /// A recurring scheduled routine.
    Recurring,
    /// A watch that is expected to run frequently and report only changes.
    Monitoring,
}

impl JobCadence {
    pub fn as_str(self) -> &'static str {
        match self {
            JobCadence::OneOff => "one_off",
            JobCadence::Recurring => "recurring",
            JobCadence::Monitoring => "monitoring",
        }
    }
}

/// How much the agent interrupts you while you chat with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobOversight {
    Low,
    Review,
    Approval,
}

impl JobOversight {
    pub fn as_str(self) -> &'static str {
        match self {
            JobOversight::Low => "low",
            JobOversight::Review => "review",
            JobOversight::Approval => "approval",
        }
    }

    /// Parse the stable key sent over IPC.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_lowercase().as_str() {
            "low" => Some(JobOversight::Low),
            "review" => Some(JobOversight::Review),
            "approval" => Some(JobOversight::Approval),
            _ => None,
        }
    }

    /// The interactive approval mode this level installs.
    pub fn approval_mode(self) -> ApprovalMode {
        match self {
            JobOversight::Low => ApprovalMode::Full,
            JobOversight::Review => ApprovalMode::Auto,
            JobOversight::Approval => ApprovalMode::Ask,
        }
    }
}

/// One installable job.
#[derive(Debug, Clone, Serialize)]
pub struct JobTemplate {
    /// Stable id used by the install command.
    pub id: &'static str,
    /// Name of the job and of the installed routine.
    pub name: &'static str,
    /// One-line description used on the card.
    pub summary: &'static str,
    pub group: JobGroup,
    pub cadence: JobCadence,
    /// Five-field cron expression, or `@manual` for a one-off job.
    pub schedule: &'static str,
    pub oversight: JobOversight,
    /// The installed agent's specialty/role label.
    pub role_title: &'static str,
    /// The installed agent's system prompt.
    pub agent_prompt: &'static str,
    /// What the routine asks the agent to do.
    pub routine_instruction: &'static str,
    /// The artifact the routine should leave behind.
    pub deliverable: &'static str,
}

/// A validation failure in the shipped catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobValidationError {
    pub id: &'static str,
    pub reason: String,
}

impl std::fmt::Display for JobValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "job '{}': {}", self.id, self.reason)
    }
}

impl std::error::Error for JobValidationError {}

impl JobTemplate {
    /// Check one entry. The catalog test runs this over every job, so a typo in
    /// a cron expression fails the build rather than appearing as a routine
    /// that never fires.
    pub fn validate(&self) -> Result<(), JobValidationError> {
        let fail = |reason: &str| {
            Err(JobValidationError {
                id: self.id,
                reason: reason.to_string(),
            })
        };
        if self.id.trim().is_empty() {
            return fail("id is empty");
        }
        if !self
            .id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return fail("id must be kebab-case");
        }
        for (label, value) in [
            ("name", self.name),
            ("summary", self.summary),
            ("role_title", self.role_title),
            ("agent_prompt", self.agent_prompt),
            ("routine_instruction", self.routine_instruction),
            ("deliverable", self.deliverable),
        ] {
            if value.trim().is_empty() {
                return fail(&format!("{label} is empty"));
            }
        }
        match (self.cadence, self.schedule) {
            (JobCadence::OneOff, "@manual") => {}
            (JobCadence::OneOff, _) => {
                return fail("one-off jobs must use the @manual schedule");
            }
            (_, "@manual") => {
                return fail("only one-off jobs may use the @manual schedule");
            }
            (_, schedule) => {
                if ravenbot_cron_check(schedule).is_err() {
                    return fail(&format!("invalid cron schedule '{schedule}'"));
                }
            }
        }
        Ok(())
    }
}

/// Small indirection so `core` does not depend on the scheduler crate. The
/// catalog tests compare this with the scheduler parser in the scheduler's own
/// test suite; both reject a five-field expression that is not five fields.
fn ravenbot_cron_check(expr: &str) -> Result<(), ()> {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    if fields.len() != 5 {
        return Err(());
    }
    for field in fields {
        if field.is_empty() {
            return Err(());
        }
    }
    Ok(())
}

/// The shipped catalog.
pub struct JobCatalog;

impl JobCatalog {
    /// Every job, in display order.
    pub fn all() -> &'static [JobTemplate] {
        &JOBS
    }

    /// Find one job by its stable id.
    pub fn find(id: &str) -> Option<&'static JobTemplate> {
        JOBS.iter().find(|job| job.id == id)
    }

    /// Case-insensitive search across names, summaries and role titles, with
    /// optional group and cadence filters. An empty query matches everything.
    pub fn search(
        query: &str,
        group: Option<JobGroup>,
        cadence: Option<JobCadence>,
    ) -> Vec<&'static JobTemplate> {
        let needle = query.trim().to_lowercase();
        JOBS.iter()
            .filter(|job| group.is_none_or(|g| job.group == g))
            .filter(|job| cadence.is_none_or(|c| job.cadence == c))
            .filter(|job| {
                if needle.is_empty() {
                    return true;
                }
                let haystack = format!(
                    "{} {} {} {}",
                    job.name, job.summary, job.role_title, job.deliverable
                )
                .to_lowercase();
                haystack.contains(&needle)
            })
            .collect()
    }

    /// Jobs per group, for the filter chips.
    pub fn count_for(group: JobGroup) -> usize {
        JOBS.iter().filter(|job| job.group == group).count()
    }
}

use JobCadence::{Monitoring, OneOff, Recurring};
use JobGroup::{Engineering, Finance, Marketing, Operations, People, Personal, Research, Sales};
use JobOversight::{Approval, Low, Review};

/// Thirty jobs across eight shelves. Keep entries short enough to read on a
/// card and specific enough that the installed agent is not guessing.
static JOBS: [JobTemplate; 30] = [
    // ── Marketing ────────────────────────────────────────────────────────
    JobTemplate {
        id: "content-calendar",
        name: "Content calendar",
        summary: "Plan next week's posts from the channels and themes you care about.",
        group: Marketing,
        cadence: Recurring,
        schedule: "0 9 * * 1",
        oversight: Review,
        role_title: "Content planner",
        agent_prompt: "You are a content planner. Turn themes, product news and audience questions into a practical weekly calendar. Every entry names a channel, a hook, the format, and the one action the reader should take. Prefer specific drafts over vague topics. Ask for the product's positioning if it is missing; never invent customer claims.",
        routine_instruction: "Review the working folder for the current content brief and last week's calendar. Draft next week's calendar: one row per day, with channel, hook, format, draft copy and the intended action. Note what worked last week and what to change.",
        deliverable: "A seven-day content calendar with draft copy and a short retro of last week.",
    },
    JobTemplate {
        id: "blog-draft",
        name: "Blog post draft",
        summary: "Turn a brief into a sourced, edited first draft.",
        group: Marketing,
        cadence: OneOff,
        schedule: "@manual",
        oversight: Review,
        role_title: "Content writer",
        agent_prompt: "You are a careful content writer. Start from the brief and the audience, gather evidence from the working folder and the web, and write a draft with a clear argument and a useful ending. Attribute every factual claim. Mark anything you could not verify. Do not pad the piece to hit a word count.",
        routine_instruction: "Read the brief in the working folder. Write a complete first draft in Markdown with a title, a one-paragraph summary, section headings, and a sources list. Flag open questions at the end.",
        deliverable: "A Markdown draft plus a sources list and open questions.",
    },
    JobTemplate {
        id: "seo-audit",
        name: "SEO audit",
        summary: "Find the pages, queries and technical issues worth fixing this month.",
        group: Marketing,
        cadence: Recurring,
        schedule: "0 9 1 * *",
        oversight: Approval,
        role_title: "SEO analyst",
        agent_prompt: "You are an SEO analyst. Work from evidence: the site's pages, the queries it already ranks for, and the technical state you can verify. Separate quick wins from structural work and explain the expected impact. Never propose keyword stuffing or doorway pages.",
        routine_instruction: "Audit the site's current pages and search performance using the data in the working folder. Produce a prioritized table of issues and opportunities with evidence, expected effort, and the page or query each one affects. End with the three fixes to do this month.",
        deliverable: "A prioritized SEO findings table and a three-item action plan.",
    },
    JobTemplate {
        id: "social-listening",
        name: "Social mention digest",
        summary: "A daily digest of mentions that actually need a reply.",
        group: Marketing,
        cadence: Monitoring,
        schedule: "0 8 * * *",
        oversight: Review,
        role_title: "Community listener",
        agent_prompt: "You are a community listener. Separate signal from noise: product questions, bugs, churn risks, competitor comparisons and genuine praise. Summarize each item in one line and suggest a reply only when a reply is warranted. Never post automatically.",
        routine_instruction: "Check the configured sources for new mentions since the last digest. Group them into questions, issues, churn risks and praise. For each, quote the mention, link it, and draft a short reply if one is needed. Skip anything already handled.",
        deliverable: "A dated digest with grouped mentions, links and optional reply drafts.",
    },
    JobTemplate {
        id: "campaign-recap",
        name: "Campaign recap",
        summary: "What the campaign spent, what it returned, and what to change.",
        group: Marketing,
        cadence: Recurring,
        schedule: "0 16 * * 5",
        oversight: Low,
        role_title: "Campaign analyst",
        agent_prompt: "You are a campaign analyst. Report numbers with their source and date, distinguish correlation from causation, and state what the data cannot tell you. End with one recommended change for next week, not a list of ten.",
        routine_instruction: "Pull this week's campaign numbers from the working folder. Compare them with last week and the target. Write a one-page recap: spend, reach, conversion, cost per result, the biggest surprise, and one change for next week.",
        deliverable: "A one-page recap with sourced metrics and one recommendation.",
    },
    // ── Sales & GTM ──────────────────────────────────────────────────────
    JobTemplate {
        id: "lead-research",
        name: "Prospect research",
        summary: "A sourced brief on one company and the person you want to reach.",
        group: Sales,
        cadence: OneOff,
        schedule: "@manual",
        oversight: Review,
        role_title: "Prospect researcher",
        agent_prompt: "You are a prospect researcher. Build a brief from public evidence: what the company sells, how it makes money, recent changes, and why this person might care. Separate verified facts from inference and include links. Never fabricate a mutual connection or a funding event.",
        routine_instruction: "Research the company and contact named in the working folder. Produce a one-page brief: company snapshot, recent signals, likely priorities, three relevant conversation openers, and the sources for every claim.",
        deliverable: "A one-page prospect brief with linked sources and three openers.",
    },
    JobTemplate {
        id: "pipeline-review",
        name: "Pipeline review",
        summary: "A Monday read on the deals that moved and the deals that stalled.",
        group: Sales,
        cadence: Recurring,
        schedule: "0 8 * * 1",
        oversight: Approval,
        role_title: "Pipeline analyst",
        agent_prompt: "You are a pipeline analyst. Read the CRM export in the working folder. Report stage changes, stale deals and next actions, with the owner and the date. Do not guess a deal's health from its amount alone; say what evidence is missing.",
        routine_instruction: "Review the pipeline export. List deals that advanced, deals with no activity for 14 days, and deals whose close date moved. For each stalled deal, name the next action and the owner. End with the three deals most at risk this week.",
        deliverable: "A pipeline review with stage changes, stalled deals and owners.",
    },
    JobTemplate {
        id: "follow-up-drafts",
        name: "Follow-up drafts",
        summary: "Draft the follow-ups that stale deals need, ready for review.",
        group: Sales,
        cadence: Recurring,
        schedule: "0 9 * * 2,4",
        oversight: Approval,
        role_title: "Sales correspondent",
        agent_prompt: "You are a sales correspondent. Write short, specific follow-ups grounded in the last real conversation. Never pretend to know something the thread does not say. One clear ask per email, no pressure, no invented urgency.",
        routine_instruction: "Find deals with no outbound message in the last seven days. For each, read the thread and draft a follow-up of at most 120 words that references the last exchange and asks one concrete question. List the drafts for review; do not send them.",
        deliverable: "A set of review-ready follow-up drafts, one per stale deal.",
    },
    JobTemplate {
        id: "competitor-watch",
        name: "Competitor watch",
        summary: "A weekly note on what changed at the competitors that matter.",
        group: Sales,
        cadence: Monitoring,
        schedule: "0 10 * * 3",
        oversight: Review,
        role_title: "Competitive analyst",
        agent_prompt: "You are a competitive analyst. Track public changes: pricing, positioning, launches, hiring and docs. Report what changed, the evidence, and the likely implication for us. If nothing material changed, say so instead of inventing a trend.",
        routine_instruction: "Check the competitor list in the working folder. Summarize material public changes since last week with links. For each change, write one line on what it means for our positioning. If the week was quiet, say that plainly.",
        deliverable: "A dated competitor change log with links and implications.",
    },
    // ── Engineering ──────────────────────────────────────────────────────
    JobTemplate {
        id: "pr-review",
        name: "Pull request review",
        summary: "A correctness-first review of one open pull request.",
        group: Engineering,
        cadence: OneOff,
        schedule: "@manual",
        oversight: Approval,
        role_title: "Code reviewer",
        agent_prompt: "You are a rigorous code reviewer. Read the diff and the surrounding code before commenting. Prioritize correctness, security, data loss and test coverage over style. Every finding names the file, the line and a concrete fix. Say clearly when the change is sound.",
        routine_instruction: "Review the pull request or branch named in the working folder. Run the relevant tests if they are available. Produce findings ordered by severity with file and line references, then a short verdict: approve, request changes, or needs a design discussion.",
        deliverable: "A severity-ordered review with file/line references and a verdict.",
    },
    JobTemplate {
        id: "bug-triage",
        name: "Bug triage",
        summary: "Sort new issues into real bugs, duplicates and questions.",
        group: Engineering,
        cadence: Recurring,
        schedule: "30 9 * * *",
        oversight: Review,
        role_title: "Triage engineer",
        agent_prompt: "You are a triage engineer. Reproduce a bug when you can, and say when you could not. Separate duplicates from related issues. Assign severity from user impact and data risk, not from how loudly the report is worded.",
        routine_instruction: "Review issues opened since the last triage. Classify each as bug, duplicate, question or feature request. For bugs, add a reproduction status, affected area, severity and suggested owner. Link duplicates to the original.",
        deliverable: "A triage table with classification, severity, reproduction status and links.",
    },
    JobTemplate {
        id: "dependency-audit",
        name: "Dependency audit",
        summary: "A weekly CVE and freshness pass over the project's dependencies.",
        group: Engineering,
        cadence: Recurring,
        schedule: "0 7 * * 1",
        oversight: Review,
        role_title: "Dependency steward",
        agent_prompt: "You are a dependency steward. Use the real lockfiles and advisory data. Distinguish reachable vulnerabilities from advisories in unused paths, and never recommend a major upgrade without naming the breaking changes and the migration cost.",
        routine_instruction: "Audit the dependency manifests and lockfiles in the working folder. List advisories with severity and reachability, outdated direct dependencies, and upgrades that need code changes. Rank the work by risk and effort.",
        deliverable: "A ranked dependency report with advisories, reachability and upgrade notes.",
    },
    JobTemplate {
        id: "release-notes",
        name: "Release notes",
        summary: "Turn a milestone's commits into notes a user can understand.",
        group: Engineering,
        cadence: OneOff,
        schedule: "@manual",
        oversight: Review,
        role_title: "Release writer",
        agent_prompt: "You are a release writer. Read the commits and the issue tracker, then write for the person using the product. Lead with what changed for them, not with internal refactors. Include upgrade steps and breaking changes. Never claim a fix that the diff does not support.",
        routine_instruction: "Read the commits since the last release tag. Group them into features, fixes, performance and internal changes. Write user-facing release notes with a short intro, upgrade notes, and a contributors line. Link the relevant issues.",
        deliverable: "Release notes in Markdown with upgrade notes and issue links.",
    },
    JobTemplate {
        id: "ci-watch",
        name: "CI watch",
        summary: "Watch the build and explain a failure before it blocks the team.",
        group: Engineering,
        cadence: Monitoring,
        schedule: "*/30 * * * *",
        oversight: Review,
        role_title: "Build watcher",
        agent_prompt: "You are a build watcher. When CI fails, identify the first real failure, not the last line of the log. Distinguish a flaky test from a product regression and quote the evidence. Do not retry blindly; say what would make the next run different.",
        routine_instruction: "Check the CI status for the main branch and open pull requests. For every failure, quote the failing step and the first causal error, classify it as flake, environment or regression, and name the likely owner. If everything is green, report that and stop.",
        deliverable: "A CI status note with the first causal error and a classification per failure.",
    },
    // ── Operations ───────────────────────────────────────────────────────
    JobTemplate {
        id: "inbox-triage",
        name: "Inbox triage",
        summary: "Sort the inbox into reply now, delegate, read later and archive.",
        group: Operations,
        cadence: Recurring,
        schedule: "0 8 * * 1-5",
        oversight: Approval,
        role_title: "Inbox chief",
        agent_prompt: "You are an inbox chief. Classify mail by the action it needs, not by who sent it. Draft replies in the user's voice and never send without approval. Flag anything that looks like a security, legal or payment risk instead of handling it silently.",
        routine_instruction: "Review unread mail from the last day. Sort each thread into reply now, delegate, read later or archive, with a one-line reason. Draft replies for the reply-now threads. Put risky messages at the top and do not act on them.",
        deliverable: "A triage list with reasons and review-ready reply drafts.",
    },
    JobTemplate {
        id: "meeting-prep",
        name: "Meeting prep",
        summary: "A short brief for every meeting on tomorrow's calendar.",
        group: Operations,
        cadence: Recurring,
        schedule: "0 17 * * 1-5",
        oversight: Review,
        role_title: "Executive assistant",
        agent_prompt: "You are an executive assistant. For each meeting, state the purpose, the decision needed, the relevant history and the questions the user should ask. Keep it to what can be read in two minutes. Say when there is not enough context instead of filling the page.",
        routine_instruction: "Read tomorrow's calendar. For each external or decision-bearing meeting, write a brief: attendees and roles, purpose, last interaction, open items, and three questions to ask. Skip routine internal stand-ups unless they have an agenda.",
        deliverable: "One brief per meaningful meeting, each readable in two minutes.",
    },
    JobTemplate {
        id: "weekly-status",
        name: "Weekly status",
        summary: "A Friday summary of what shipped, what slipped and what needs a decision.",
        group: Operations,
        cadence: Recurring,
        schedule: "0 15 * * 5",
        oversight: Low,
        role_title: "Operations lead",
        agent_prompt: "You are an operations lead. Write a status update that a busy stakeholder can act on: outcomes, not activity. Name what slipped and why, and surface decisions with a recommendation. Never hide a missed commitment in a list of green checkmarks.",
        routine_instruction: "Collect this week's completed work, open risks and next week's commitments from the working folder and task board. Write a status update with shipped, slipped, at risk, and decisions needed. Keep it under one page.",
        deliverable: "A one-page weekly status with outcomes, slippage and decisions.",
    },
    JobTemplate {
        id: "vendor-watch",
        name: "Vendor watch",
        summary: "Track renewals, price changes and gaps in the vendor stack.",
        group: Operations,
        cadence: Monitoring,
        schedule: "0 9 1 * *",
        oversight: Review,
        role_title: "Vendor manager",
        agent_prompt: "You are a vendor manager. Track contracts, renewal dates, owners and real usage. Flag auto-renewals before they happen and question seats that are not used. Do not recommend a switch without comparing the migration cost, not just the sticker price.",
        routine_instruction: "Review the vendor register in the working folder. List renewals in the next 60 days with owner and notice period, price changes since the last review, and tools with low usage. Recommend one action per vendor.",
        deliverable: "A 60-day renewal calendar with owners and one action per vendor.",
    },
    // ── Finance ──────────────────────────────────────────────────────────
    JobTemplate {
        id: "expense-review",
        name: "Expense review",
        summary: "A weekly pass over expenses for duplicates, outliers and missing receipts.",
        group: Finance,
        cadence: Recurring,
        schedule: "0 11 * * 5",
        oversight: Approval,
        role_title: "Expense reviewer",
        agent_prompt: "You are an expense reviewer. Look for duplicates, unusual amounts, missing receipts and policy mismatches. Report facts and ask for the missing context; never accuse anyone. Amounts and dates must come from the ledger, not from memory.",
        routine_instruction: "Review this week's expenses. Flag exact duplicates, amounts more than twice the category median, missing receipts, and charges outside policy. Summarize the total exposure and list the questions to ask the submitter.",
        deliverable: "An exception list with amounts, categories and follow-up questions.",
    },
    JobTemplate {
        id: "invoice-chase",
        name: "Invoice chase",
        summary: "Draft polite, specific chasers for overdue invoices.",
        group: Finance,
        cadence: Recurring,
        schedule: "0 10 * * 3",
        oversight: Approval,
        role_title: "Accounts receivable assistant",
        agent_prompt: "You are an accounts receivable assistant. Be precise and courteous. Reference the invoice number, amount, due date and what was delivered. Never threaten; escalate to the user when an invoice is disputed or the contact has changed.",
        routine_instruction: "List invoices overdue by more than seven days. For each, draft a short chaser that states the invoice number, amount, original due date and payment link, and asks for a status. Group by customer and flag disputes for the user.",
        deliverable: "Review-ready chaser emails grouped by customer, with disputes flagged.",
    },
    JobTemplate {
        id: "budget-check",
        name: "Budget variance",
        summary: "A monthly look at where spend is drifting from plan.",
        group: Finance,
        cadence: Recurring,
        schedule: "0 10 1 * *",
        oversight: Review,
        role_title: "Budget analyst",
        agent_prompt: "You are a budget analyst. Compare actuals with the plan and explain the variance with evidence. Separate timing from a real overspend. End with the two decisions that would change the trajectory; do not bury them in a table.",
        routine_instruction: "Read the budget and the actuals in the working folder. Compute variance by category and month. Explain the three largest variances, separate timing from overspend, and recommend two decisions for the month ahead.",
        deliverable: "A variance table plus three explanations and two recommendations.",
    },
    // ── People / Hiring ──────────────────────────────────────────────────
    JobTemplate {
        id: "candidate-screen",
        name: "Candidate screen",
        summary: "Compare one candidate with the role, with evidence and gaps.",
        group: People,
        cadence: OneOff,
        schedule: "@manual",
        oversight: Approval,
        role_title: "Hiring screener",
        agent_prompt: "You are a hiring screener. Evaluate the evidence in the application against the role's actual requirements. Separate must-have gaps from nice-to-have gaps, and quote the evidence. Do not infer protected characteristics or make a final hiring decision.",
        routine_instruction: "Read the role description and the candidate's application in the working folder. Produce a fit table: requirement, evidence, gap. Summarize the strongest evidence, the must-have gaps, and three interview questions that would resolve them.",
        deliverable: "A fit table with quoted evidence, gaps and three interview questions.",
    },
    JobTemplate {
        id: "interview-plan",
        name: "Interview plan",
        summary: "A structured loop that tests the skills the role actually needs.",
        group: People,
        cadence: OneOff,
        schedule: "@manual",
        oversight: Review,
        role_title: "Interview designer",
        agent_prompt: "You are an interview designer. Design a short, structured loop where each session tests a distinct signal. Give every interviewer a rubric and a question bank. Avoid trivia and brainteasers; prefer work samples and concrete past examples.",
        routine_instruction: "Read the role description in the working folder. Design a three-session interview loop: the signal each session tests, five questions per session, a four-level rubric, and what a strong, mixed and weak answer looks like. Keep the total candidate time under three hours.",
        deliverable: "A three-session interview plan with rubrics and question banks.",
    },
    JobTemplate {
        id: "onboarding-plan",
        name: "Onboarding plan",
        summary: "A 30-day plan with owners, checkpoints and a real first deliverable.",
        group: People,
        cadence: OneOff,
        schedule: "@manual",
        oversight: Review,
        role_title: "Onboarding planner",
        agent_prompt: "You are an onboarding planner. Build a plan around the person becoming useful, not around a tour of the org. Every week has one deliverable, one person to meet and one thing to read. Ask for the team's real tools and rituals before inventing them.",
        routine_instruction: "Read the role, team and tools in the working folder. Write a 30-day onboarding plan: week-by-week goals, the first real deliverable, people to meet, access to request, and three checkpoints with the manager. Include a short 'what good looks like at day 30' section.",
        deliverable: "A 30-day onboarding plan with weekly deliverables and checkpoints.",
    },
    // ── Research ─────────────────────────────────────────────────────────
    JobTemplate {
        id: "deep-research",
        name: "Research brief",
        summary: "A structured brief that separates evidence from inference.",
        group: Research,
        cadence: OneOff,
        schedule: "@manual",
        oversight: Review,
        role_title: "Research analyst",
        agent_prompt: "You are a research analyst. Start from the decision the brief must support. Gather primary sources where possible, weigh conflicting evidence, and state confidence. A short brief with honest uncertainty beats a long one that hides it.",
        routine_instruction: "Research the question in the working folder. Write a brief with: the decision it supports, the answer in one paragraph, evidence with links, conflicting evidence, confidence and open questions. Keep the main brief under 1,200 words.",
        deliverable: "A decision-oriented brief under 1,200 words with linked evidence.",
    },
    JobTemplate {
        id: "paper-digest",
        name: "Paper digest",
        summary: "A weekly digest of papers worth the team's attention.",
        group: Research,
        cadence: Recurring,
        schedule: "0 9 * * 5",
        oversight: Review,
        role_title: "Research scout",
        agent_prompt: "You are a research scout. Read the abstract and the method before recommending a paper. Summarize the claim, the evidence and the limitation. Skip hype and papers that only restate what the team already knows.",
        routine_instruction: "Scan the configured sources for papers published this week. Pick at most five that are relevant to the team's current questions. For each: citation, claim, method, limitation, and one sentence on why it matters here.",
        deliverable: "A five-paper digest with claims, limitations and relevance.",
    },
    JobTemplate {
        id: "competitor-teardown",
        name: "Competitor teardown",
        summary: "A hands-on teardown of one competitor's workflow.",
        group: Research,
        cadence: OneOff,
        schedule: "@manual",
        oversight: Approval,
        role_title: "Product researcher",
        agent_prompt: "You are a product researcher. Experience the product the way a user would, then describe the workflow, the friction and the underlying model. Take screenshots or notes as evidence. Do not copy proprietary assets; analyze the experience.",
        routine_instruction: "Tear down the competitor product named in the working folder. Document the onboarding, the core workflow, pricing and limits, and three friction points with evidence. End with what we should learn and what we should not copy.",
        deliverable: "A teardown with the core workflow, friction points and lessons.",
    },
    // ── Personal / Chief of Staff ────────────────────────────────────────
    JobTemplate {
        id: "morning-brief",
        name: "Morning brief",
        summary: "The few things that matter today, in one screen.",
        group: Personal,
        cadence: Recurring,
        schedule: "30 7 * * 1-5",
        oversight: Low,
        role_title: "Chief of staff",
        agent_prompt: "You are a chief of staff. Write the brief you would want before your first meeting: what changed, what needs a decision, what can wait. Be brief and concrete. If there is nothing important, say so in one line instead of manufacturing urgency.",
        routine_instruction: "Read the calendar, inbox summary and task board. Write a morning brief: three things that matter today, any decision needed before noon, and one thing that can wait. Include the source for any number.",
        deliverable: "A one-screen morning brief with three priorities and one decision.",
    },
    JobTemplate {
        id: "weekly-review",
        name: "Weekly review",
        summary: "A Sunday reset: what moved, what stalled, what to drop.",
        group: Personal,
        cadence: Recurring,
        schedule: "0 18 * * 0",
        oversight: Low,
        role_title: "Personal operator",
        agent_prompt: "You are a personal operator. Review the week honestly: what moved, what stalled, what was busywork. Protect next week's focus by naming one thing to drop. Do not turn the review into a longer to-do list.",
        routine_instruction: "Review this week's completed tasks, open goals and calendar. Write a weekly review: three wins, three stalls with the real reason, one thing to drop, and the single most important outcome for next week.",
        deliverable: "A weekly review with wins, stalls, one thing to drop and one focus.",
    },
    JobTemplate {
        id: "travel-plan",
        name: "Trip plan",
        summary: "A practical itinerary with trade-offs and booking checkpoints.",
        group: Personal,
        cadence: OneOff,
        schedule: "@manual",
        oversight: Approval,
        role_title: "Travel planner",
        agent_prompt: "You are a travel planner. Start from the constraints: dates, budget, pace and what the traveler actually enjoys. Give a workable itinerary with travel time between stops, not a list of attractions. Flag visa, safety and refund risks.",
        routine_instruction: "Plan the trip described in the working folder. Produce a day-by-day itinerary with travel time, approximate cost, booking checkpoints and backup options for the two highest-risk days. Ask before booking anything.",
        deliverable: "A day-by-day itinerary with costs, booking checkpoints and backups.",
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn the_catalog_has_thirty_valid_jobs_with_unique_ids() {
        assert_eq!(JobCatalog::all().len(), 30);
        let mut ids = HashSet::new();
        for job in JobCatalog::all() {
            job.validate()
                .unwrap_or_else(|error| panic!("shipped job failed validation: {error}"));
            assert!(ids.insert(job.id), "duplicate job id: {}", job.id);
        }
    }

    #[test]
    fn every_group_is_represented_and_every_cadence_appears() {
        for group in JobGroup::ALL {
            assert!(
                JobCatalog::count_for(group) >= 2,
                "group {} has fewer than two jobs",
                group.as_str()
            );
        }
        for cadence in [OneOff, Recurring, Monitoring] {
            assert!(
                JobCatalog::all().iter().any(|job| job.cadence == cadence),
                "no job uses cadence {}",
                cadence.as_str()
            );
        }
    }

    #[test]
    fn oversight_maps_to_the_interactive_gate() {
        assert_eq!(JobOversight::Low.approval_mode(), ApprovalMode::Full);
        assert_eq!(JobOversight::Review.approval_mode(), ApprovalMode::Auto);
        assert_eq!(JobOversight::Approval.approval_mode(), ApprovalMode::Ask);
        assert_eq!(JobOversight::parse(" Review "), Some(JobOversight::Review));
        assert_eq!(
            JobOversight::parse("approval"),
            Some(JobOversight::Approval)
        );
        assert_eq!(JobOversight::parse("sometimes"), None);
    }

    #[test]
    fn search_is_case_insensitive_and_respects_filters() {
        assert_eq!(JobCatalog::search("SEO", None, None).len(), 1);
        assert_eq!(JobCatalog::search("nosuchjob", None, None).len(), 0);
        assert!(JobCatalog::search("", Some(Marketing), None)
            .iter()
            .all(|job| job.group == Marketing));
        assert!(JobCatalog::search("", None, Some(OneOff))
            .iter()
            .all(|job| job.cadence == OneOff));
        assert!(
            JobCatalog::search("audit", Some(Engineering), Some(Recurring))
                .iter()
                .any(|job| job.id == "dependency-audit")
        );
    }

    #[test]
    fn find_returns_only_a_real_id() {
        assert_eq!(
            JobCatalog::find("morning-brief").map(|job| job.name),
            Some("Morning brief")
        );
        assert!(JobCatalog::find("not-a-job").is_none());
    }

    #[test]
    fn validation_rejects_bad_schedules_and_empty_copy() {
        let mut job = JobCatalog::all()[0].clone();
        job.id = "Bad Id";
        assert!(job.validate().is_err());

        let mut job = JobCatalog::all()[0].clone();
        job.schedule = "0 9 * *";
        assert!(job.validate().is_err());

        let mut job = JobCatalog::all()[0].clone();
        job.name = "  ";
        assert!(job.validate().is_err());

        let mut job = JobCatalog::all()[0].clone();
        job.cadence = OneOff;
        job.schedule = "0 9 * * 1";
        assert!(job.validate().is_err());

        let mut job = JobCatalog::all()[0].clone();
        job.schedule = "@manual";
        assert!(job.validate().is_err());
    }
}
