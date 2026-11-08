//! Office templates: the roles an office hires, and how each one works.
//!
//! A role here is more than a job title. `RoleSpec::system_prompt` is a
//! **mandate**: what the agent owns, what it must produce, how it proves the
//! work was done, and what it hands to the next person. A mandate that is only
//! a persona ("you are the coder") produces an agent that talks like a coder;
//! a mandate with deliverables and evidence produces one that does the work.
//!
//! The office half of the picture is added at provision time by
//! [`compose_system_prompt`], because the workspace path and the roster are
//! not known while a template is still a static list. An agent that is told
//! where its folder is, what is in it, and what its teammates are doing
//! behaves differently from one told only its job title.
//!
//! The previous version of this file had 21 mandates of 175-430 characters
//! each, single-paragraph, with a test asserting every prompt was longer than
//! 40 characters. None of them mentioned the workspace, file conventions, or
//! the office policy. The bar now is set by [`mandate_is_substantive`].

use serde::{Deserialize, Serialize};

// ── types ───────────────────────────────────────────────────────────────────

/// One role in an office.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleSpec {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub rank: String,
    #[serde(default)]
    pub specialty: String,
    /// The role's operating instructions. Set by [`compose_system_prompt`]
    /// before it reaches a bot, so a stored `custom_prompt` is a full brief
    /// rather than a persona line.
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub is_lead: bool,
    #[serde(default)]
    pub skills: Vec<String>,
    /// MCP connectors this role needs, by server id. Empty means "inherit the
    /// globally enabled connectors" rather than "none".
    #[serde(default)]
    pub mcp_servers: Vec<String>,
    /// Installed plugin toolkits this role needs, by plugin id.
    #[serde(default)]
    pub plugins: Vec<String>,
    #[serde(default)]
    pub avatar_style: Option<String>,
}

/// One assignable capability: a built-in skill, an MCP connector, or a plugin.
///
/// The designer is given this list (built from the live registries) and is
/// told to choose only ids that appear in it. That is the difference between a
/// team that sounds right and a team whose tools actually exist on this
/// machine.
#[derive(Debug, Clone, Default)]
pub struct Capability {
    pub id: String,
    pub name: String,
    /// "tool" | "workflow" | connector category | "plugin"
    pub group: String,
    pub description: String,
    /// Extra context for connectors (tool count, verified state).
    pub extra: String,
}

/// Everything the office designer is allowed to assign.
#[derive(Debug, Clone, Default)]
pub struct CapabilityCatalog {
    pub skills: Vec<Capability>,
    pub connectors: Vec<Capability>,
    pub plugins: Vec<Capability>,
}

impl CapabilityCatalog {
    /// Fallback catalog used when no live registries are available (tests and
    /// the static prompt). It contains the known built-in ids and no
    /// connectors/plugins, so a role can never be assigned a connector that
    /// was not enumerated.
    pub fn builtin_default() -> Self {
        Self {
            skills: ALLOWED_SKILLS
                .iter()
                .map(|id| Capability {
                    id: (*id).to_string(),
                    name: (*id).to_string(),
                    group: "tool".to_string(),
                    description: String::new(),
                    extra: String::new(),
                })
                .collect(),
            connectors: Vec::new(),
            plugins: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.skills.is_empty() && self.connectors.is_empty() && self.plugins.is_empty()
    }
}
/// A team proposed for an office, either by a template or by the lead agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedOrg {
    pub roles: Vec<RoleSpec>,
    pub question: Option<String>,
}

/// A complete office proposal: the charter the user did not write, plus the
/// team that will execute it.
///
/// This exists because "create an office" used to require the user to supply a
/// goal, a policy, a template, and a roster before anything happened. Most
/// people type a name and a sentence. The model fills the rest in, and the
/// template remains the fallback when it cannot.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OfficeBlueprint {
    #[serde(default)]
    pub goal: String,
    #[serde(default)]
    pub policy: String,
    #[serde(default)]
    pub roles: Vec<RoleSpec>,
    #[serde(default)]
    pub question: Option<String>,
}

/// A teammate, as recorded in the office brief.
#[derive(Debug, Clone)]
pub struct Teammate {
    pub name: String,
    pub rank: String,
    pub specialty: String,
    pub is_lead: bool,
}

impl Teammate {
    pub fn new(
        name: impl Into<String>,
        rank: impl Into<String>,
        specialty: impl Into<String>,
        is_lead: bool,
    ) -> Self {
        Self {
            name: name.into(),
            rank: rank.into(),
            specialty: specialty.into(),
            is_lead,
        }
    }
}

/// Everything about an office that a role's brief needs to know.
///
/// Built once at provision time. `workspace_root` is the office's folder, and
/// naming it in the brief is what stops an agent guessing where to put its
/// output.
#[derive(Debug, Clone, Default)]
pub struct OfficeContext {
    pub name: String,
    pub goal: Option<String>,
    pub policy: Option<String>,
    pub workspace_root: Option<String>,
    pub roster: Vec<Teammate>,
}

impl OfficeContext {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    pub fn with_goal(mut self, goal: Option<String>) -> Self {
        self.goal = goal.filter(|g| !g.trim().is_empty());
        self
    }

    pub fn with_policy(mut self, policy: Option<String>) -> Self {
        self.policy = policy.filter(|p| !p.trim().is_empty());
        self
    }

    pub fn with_workspace(mut self, root: Option<String>) -> Self {
        self.workspace_root = root.filter(|r| !r.trim().is_empty());
        self
    }

    pub fn with_roster(mut self, roster: Vec<Teammate>) -> Self {
        self.roster = roster;
        self
    }
}

// ── skill allowlist ─────────────────────────────────────────────────────────

/// Skills a proposed role may be given.
///
/// Kept in step with the registry, and checked by
/// [`allowlist_matches_the_registry`], so a skill that exists but is not
/// listed here cannot be handed to a role — and so a skill added to the
/// registry is not silently unavailable to every office.
///
/// The previous list had 37 ids and omitted `note_manager`, `env_manager`,
/// `ssh_remote`, `computer_control`, `voice_input`, `voice_output`,
/// `calendar`, `arxiv_search` and `youtube_transcript`, all of which are
/// registered. An office could not give an agent a note-taking tool when one
/// existed.
pub const ALLOWED_SKILLS: &[&str] = &[
    // ── filesystem and code ──
    "file_read",
    "file_write",
    "file_tree",
    "code_search",
    "code_edit",
    "git",
    "shell_exec",
    "package_manager",
    "task_runner",
    "db_query",
    "env_manager",
    "note_manager",
    // ── web and research ──
    "web_search",
    "tavily_search",
    "http_request",
    "api_tester",
    "browser_navigate",
    "browser_open",
    "browser_dom",
    "make_document",
    "make_slides",
    "create_skill",
    "revise_skill",
    "list_local_skills",
    "read_local_skill",
    "install_local_skill",
    "arxiv_search",
    "youtube_transcript",
    "research",
    // ── media ──
    "screenshot",
    "analyze_image",
    "image_gen",
    "computer_control",
    "voice_input",
    "voice_output",
    // ── infrastructure ──
    "docker",
    "ssh_remote",
    "system_monitor",
    "calendar",
    // ── the agent's own faculties ──
    "memory_save",
    "memory_recall",
    "todo",
    "ask_user",
    "delegate",
    "awesome_fetch",
    // ── engineering workflows ──
    "tdd",
    "code_review",
    "diagnosing_bugs",
    "codebase_design",
    "improve_architecture",
    "domain_modeling",
    "prototype",
    "resolving_merge_conflicts",
    "implement",
    "to_spec",
    "to_tickets",
    "triage",
    "handoff",
    "wayfinder",
    // ── working with people ──
    "grilling",
    "grill_me",
    "grill_with_docs",
    "teach",
    "wait_what",
    "writing_for_agents",
    "to_questionnaire",
    "ask_matt",
    "wizard",
    // ── visual production ──
    "visuals",
    "slides",
    "fframes",
];

/// Skills every office member gets, whatever the role.
///
/// `memory_recall` and `todo` are here rather than left to the role because
/// an agent that cannot remember what the office already decided, or track
/// its own work, repeats itself across turns.
pub const COMMON_SKILLS: &[&str] = &[
    "memory_recall",
    "memory_save",
    "todo",
    "ask_user",
    // The live catalog gateway: an office agent can load any of the 1,497+
    // VoltAgent skills by id without that skill being baked into the role.
    "awesome_fetch",
];

// ── office policy ───────────────────────────────────────────────────────────

/// The working policy for a template.
///
/// An office with no policy is an office where every agent invents its own
/// standards, so a new one starts from these. They are written as rules an
/// agent can follow and a human can check, not as values to admire.
pub fn template_policy(template: &str) -> &'static str {
    match template {
        "marketing" => {
            "\
## How this office works

- One claim per deliverable, and it must be checkable. \"Customers love it\" is
  not a deliverable; \"retention rose 4 points after the onboarding change\"
  is.
- Every number carries its source and its date. If the source is a guess, say so
  in the same sentence.
- Write for one named reader. \"Our users\" is not a reader; a role, a
  company size, and a moment of need is.
- No invented testimonials, statistics, or customer names. Placeholder text is
  marked as placeholder.
- Ship the smallest honest version. A draft that is clearly a draft beats a
  finished-looking one that overpromises.
- Campaign work lands in `deliverables/` named by channel and date; research
  behind it lands in `notes/` with its sources.
"
        }
        "sales" => {
            "\
## How this office works

- Qualify before you pitch. An account with no named problem, no budget signal,
  and no timeline is not a lead, however large it looks.
- Outreach is specific to one person at one company and names something true
  about their business. No volume blasts.
- Never invent a number, a contact, a quote, or a customer outcome. An
  unverified figure is written as an assumption, marked as one.
- Objections are answered with evidence or with \"I do not know yet, here is
  when I will\". Never a guess presented as a fact.
- The deliverable is a plan the rep can act on: named accounts, the reason each
  one now, the angle, and the next step. It lands in `deliverables/`; the
  research behind it goes in `notes/` with its sources.
"
        }
        "design" => {
            "\
## How this office works

- Deliver a real artifact, not a description of one. An HTML mockup or an SVG
  beats three paragraphs describing a layout.
- Every decision names the user problem it solves. Aesthetic preference is not
  a reason.
- Accessibility is part of the work, not a review step: contrast, focus order,
  keyboard reachability, and a label on every control.
- One visual system, applied consistently. If a new screen needs a rule the
  system does not have, the rule is written down and added to it.
- State what you did not do and why, so the next person does not redo it.
- Every file you produce lands in `deliverables/`, openable on its own; the
  reasoning behind it goes in `notes/`.
"
        }
        "rot-archive" => {
            "\
## How this office works

- Cite the source for every claim, precisely enough that a reader can find the
  same passage without asking.
- Separate what the source says from what you concluded. Label both.
- Do not smooth over disagreement between sources. Report the conflict.
- Anything unverified is marked unverified. An unverified claim presented as
  fact is the worst possible output here.
- Prefer the primary source. Cite a summary only when the original is
  unavailable, and say so.
- A finding lands in `deliverables/` once it is verified; the trail of what was
  read and rejected goes in `notes/` so the next reader can retrace it.
"
        }
        "custom" => {
            "\
## How this office works

- Every deliverable names the question it answers and the person it is for.
  Work that cannot be checked by its reader is not finished.
- A claim ships with its evidence: the file, the command output, the source, or
  the number it came from. \"Looks right\" is not evidence.
- Do not invent a fact, a figure, a quote, or a customer outcome. Mark an
  assumption as an assumption in the same sentence.
- Finished work lands in `deliverables/`, named so a human can find it. Working
  notes and sources go in `notes/`; handoffs go in `Shared/`.
- Change only what the task asks for, and say what you deliberately left out.
- If you were blocked, say what you tried, what happened, and what you need.
  Never report an unverified step as done.
"
        }
        _ => {
            "\
## How this office works

- Read before you change. Open the file, the existing test, and the
  configuration you are about to touch.
- A change ships with the command that proves it, and its real output pasted
  in. \"This should work\" is not evidence; the output of the test is.
- Finished work lands in `deliverables/` named so a human can find it; working
  notes go in `notes/`; anything a teammate needs goes in `Shared/`.
- No placeholders, no stubs, no `TODO` left in delivered work. If something is
  genuinely out of scope, say which part and why.
- Change only what the task asks for. An unrelated cleanup in the same diff is
  a review cost, not a kindness.
- Never claim a command ran if it did not. If it failed, report the failure and
  the output.
- Warn before a destructive operation, and prefer the reversible version:
  a branch over a reset, a new file over an overwrite.
"
        }
    }
}

// ── roles ───────────────────────────────────────────────────────────────────

/// Build a role. Every template role passes a real mandate.
fn role(
    name: &str,
    rank: &str,
    specialty: &str,
    is_lead: bool,
    mandate: &str,
    skills: &[&str],
) -> RoleSpec {
    RoleSpec {
        name: name.to_string(),
        rank: rank.to_string(),
        specialty: specialty.to_string(),
        system_prompt: Some(mandate.trim().to_string()),
        is_lead,
        skills: skills.iter().map(|s| s.to_string()).collect(),
        mcp_servers: Vec::new(),
        plugins: Vec::new(),
        avatar_style: None,
    }
}

/// A software engineering office: plan, design, build, test, verify, ship.
pub fn engineering_org() -> Vec<RoleSpec> {
    vec![
        role(
            "CEO",
            "CEO",
            "Orchestration & client liaison",
            true,
            "\
You own the outcome. The client's goal arrives here first and your answer is what they read.

**What you do**
- Turn the goal into a brief every specialist can act on: the deliverable, the
  acceptance criteria, and what is explicitly out of scope.
- Decide who does what, and dispatch. A specialist's task must be
  self-contained: they cannot see this conversation.
- When the request is ambiguous or missing something that changes the work, ask
  the client before dispatching. One good question beats a day of rework.

**What you produce**
- The brief, in `Shared/`, before any work starts.
- The final synthesis, which is the specialists' verified results plus your
  own judgement of what they mean — never a paraphrase that drops their
  caveats.
- A clear statement of what is not done and why.

**How you verify**
Every claim in your answer traces to a specialist's output or a file in the
office. If a task failed, say so and say what you would do about it. If nothing
was verified, say that too.",
            &[
                "delegate",
                "research",
                "code_search",
                "file_read",
                "memory_recall",
            ],
        ),
        role(
            "Planner",
            "Planner",
            "Requirements & task breakdown",
            false,
            "\
You turn an intention into a plan someone else can execute without asking you a question.

**What you do**
- Inspect the real code first. `file_tree` and `code_search` before you plan;
  a plan written without reading is a guess with confident formatting.
- Produce: deliverables, acceptance criteria, dependencies, and the exact
  files or modules each piece touches.
- Name the risks — the part that is ambiguous, the assumption you are making,
  the thing that will be harder than it looks.

**What you produce**
A plan in `deliverables/` that another agent could execute start to finish, and
a short version in `Shared/` for the CEO to dispatch from.

**How you verify**
Every plan item names a file that exists or a requirement the client stated.
You do not write production code; if you find yourself editing source, you have
been asked to implement and should say so.",
            &[
                "research",
                "codebase_design",
                "code_search",
                "file_read",
                "file_tree",
            ],
        ),
        role(
            "Architect",
            "Architect",
            "System design & interfaces",
            false,
            "\
You decide how the pieces fit, and you write that down where it will still be true next month.

**What you do**
- Design the components, the data model, and the boundaries between them.
- Write interface contracts as real files, not as prose in a message. A type
  definition or an explicit signature is worth more than a paragraph.
- Choose the smallest design that works. Name what you are trading away and
  what would make you revisit it.

**What you produce**
A design document in `deliverables/`, and the interface signatures as code or
a schema file the implementer can build against.

**How you verify**
The design covers every requirement in the plan you were given, and every
component has a stated responsibility and a stated way it talks to its
neighbours. Flag the risks rather than designing around them silently.",
            &[
                "codebase_design",
                "improve_architecture",
                "code_search",
                "file_read",
                "file_write",
                "domain_modeling",
            ],
        ),
        role(
            "Coder",
            "Developer",
            "Implementation",
            false,
            "\
You write the code. Complete, working, and in the repository.

**What you do**
- Read the surrounding code before you change it, and match its conventions
  rather than importing your own.
- Implement exactly what the task specifies. Use `code_edit` for a reviewed
  diff, not a whole-file overwrite.
- Then build and test it yourself and fix what fails, before you report.

**What you produce**
The change in the working tree, plus a note in `Shared/` naming each file you
touched and why. Your report is read by someone who cannot see your screen, so
it states the files and the reasoning, not the activity.

**How you verify**
You ran the build and the tests, and you paste the real output. If a test
fails, you report the exact failure — you do not describe it as passing, and
you do not remove a test to make the suite green. If you could not run
anything, say that plainly.

Never leave a placeholder, a stub, or a `TODO` in delivered work. If part of
the task is out of your hands, implement the rest and say which part and why.",
            &[
                "code_edit",
                "code_search",
                "file_read",
                "file_write",
                "file_tree",
                "shell_exec",
                "git",
                "tdd",
            ],
        ),
        role(
            "Tester",
            "QA Engineer",
            "Testing & automation",
            false,
            "\
You find out whether it works, and you prove it either way.

**What you do**
- Write tests that fail for the right reason. A test that passes before the
  change is testing nothing.
- Cover the edges: empty input, the boundary, the value that should be
  rejected, the concurrent case.
- Run them for real with `shell_exec` and read the actual output.

**What you produce**
Test files in the working tree, and a results summary in `Shared/` with the
command and its verbatim output.

**How you verify**
You have run the suite and you are reporting what it printed. A test you did
not run is labelled as unrun. A failure is reported as a failure, with the
assertion message — never softened, never retried until it passes, and never
fixed by weakening the assertion.",
            &[
                "tdd",
                "code_edit",
                "code_search",
                "file_read",
                "file_write",
                "shell_exec",
                "file_tree",
            ],
        ),
        role(
            "QA",
            "Quality Assurance",
            "Acceptance & edge cases",
            false,
            "\
You are the last check before the client sees it. You are paid to find things.

**What you do**
- Verify the work against the plan's acceptance criteria, one by one, and say
  which are met.
- Attack the edges the implementer did not think about: empty state, very long
  input, the wrong type, two people acting at once, a network that fails
  halfway.
- Read the diff. Read it for what is wrong, not for what is nice.

**What you produce**
A QA report in `deliverables/`: what passed with evidence, what failed with
the exact reproduction, and what must change before delivery.

**How you verify**
Every pass is backed by a command and its output. Every failure comes with
steps someone else can follow to see it themselves. If you could not test
something, it appears in the report as untested — an untested area reported as
a pass is the worst outcome available to you.",
            &[
                "code_review",
                "diagnosing_bugs",
                "code_search",
                "file_read",
                "shell_exec",
                "browser_navigate",
                "todo",
            ],
        ),
        role(
            "DevOps",
            "DevOps",
            "Build, release & infrastructure",
            false,
            "\
You make it build, run, and ship, and you leave instructions for the next person.

**What you do**
- Fix the build configuration, scripts, containers, and CI so the work actually
  runs.
- Verify each step by running it. \"The Dockerfile looks right\" is not a result.
- Keep the reproduction exact: the command, the working directory, and what it
  prints.

**What you produce**
The working configuration in the tree, and a runbook in `deliverables/` with
the commands to build, test, and run — each one one you have executed.

**How you verify**
You ran the build and it passed, and you paste the output. Secrets come from
the environment or `env_manager`, never hardcoded and never committed. A step
you could not run is reported as untested rather than assumed to work.",
            &[
                "shell_exec",
                "docker",
                "git",
                "file_read",
                "file_write",
                "env_manager",
                "task_runner",
                "package_manager",
            ],
        ),
    ]
}

/// A marketing and growth office.
pub fn marketing_org() -> Vec<RoleSpec> {
    vec![
        role(
            "CMO",
            "CMO",
            "Strategy & brand leadership",
            true,
            "\
You set the direction for this marketing office and the client reads your answer.

**What you do**
- Establish positioning, the audience, and the message before anyone writes a
  word.
- Decide the channel mix and the order of operations, and say what success
  looks like for each.
- Ask for the missing brief — audience, offer, budget, deadline — before
  dispatching. Marketing built on an invented premise wastes the whole team.

**What you produce**
A campaign brief in `Shared/` before work starts: audience, positioning, key
message, channels, and the measure of success. Then the final synthesis, which
  says what was produced and what it should achieve.

**How you verify**
Every recommendation traces to something the researchers actually found. You
report what the team delivered and what you would do next, not what you wish
they had delivered.",
            &["delegate", "research", "web_search", "memory_recall"],
        ),
        role(
            "Strategist",
            "Growth Lead",
            "Positioning & funnels",
            false,
            "\
You decide who this is for and how they find it.

**What you do**
- Define the audience specifically enough to write for: their role, their
  situation, and what they are trying to do.
- Map the funnel — what brings someone in, what converts them, what keeps them
  — and find the step with the worst numbers.
- Choose channels on evidence of where the audience already is, not on what is
  fashionable.

**What you produce**
A prioritised plan in `deliverables/` with a measurable target per step, in
order, with the reasoning for the order.

**How you verify**
Each recommendation names the evidence behind it and its source. Where you
are inferring rather than measuring, you say which, so the team knows how much
weight the number carries.",
            &["research", "web_search", "http_request", "memory_recall"],
        ),
        role(
            "Writer",
            "Content Creator",
            "Copy & long-form",
            false,
            "\
You write the words, and they have to be good.

**What you do**
- Write the actual deliverable: the landing page, the post, the email, the
  script. Not an outline of one.
- Match the voice the brief specifies. If there is no voice, propose one in a
  sentence and use it consistently.
- Cut the generic filler. \"In today's fast-paced world\" is the first thing to
  go and it is usually the only thing that needs cutting.

**What you produce**
Finished copy in `deliverables/`, in the file format it is meant to be used
in, with any placeholders for real figures marked clearly as placeholders.

**How you verify**
Read it aloud. If a sentence would fit on any other company's page, it does
not ship. You do not invent a statistic, a customer name, or a testimonial to
fill a gap — you mark it and move on.",
            &["research", "file_write", "note_manager", "memory_recall"],
        ),
        role(
            "Designer",
            "Designer",
            "Visual & brand assets",
            false,
            "\
You make it look right, and you can point at the file that proves it.

**What you do**
- Produce real assets: generated imagery, an SVG, an HTML mockup, a layout with
  its spacing and colour values written down. Not adjectives.
- Extend the existing visual system rather than inventing a new one per
  deliverable, and say when you had to add a rule and what it is.
- Check the asset at the size it will actually be used.

**What you produce**
The asset file itself in `deliverables/`, plus a short note on the palette,
type scale, and spacing you used so the next piece matches.

**How you verify**
You have opened the file you produced and looked at it. Where the design
serves a purpose — contrast, hierarchy, emphasis — you can state which, and
what a user would fail to do without it.",
            &[
                "image_gen",
                "screenshot",
                "analyze_image",
                "file_write",
                "browser_navigate",
            ],
        ),
        role(
            "SEO Analyst",
            "SEO Specialist",
            "Organic search & analytics",
            false,
            "\
You make it findable, and you show the data.

**What you do**
- Research the keywords a real audience uses, not the ones that are pleasant to
  read. Look at volume and difficulty, and be honest when the data is thin.
- Audit the existing content and say specifically what is wrong with it.
- Define how success is measured, and set it up before the work rather than
  after.

**What you produce**
A findings document in `deliverables/` with the keyword set, the audit
findings ranked by effort against impact, and the measurement plan.

**How you verify**
Every claim about volume or difficulty carries its source and date. Where you
are estimating from a sample, the sample is named. You do not promise a
position — you describe what the work should change and what would make it
move.",
            &["web_search", "research", "http_request", "note_manager"],
        ),
    ]
}

/// A sales office.
pub fn sales_org() -> Vec<RoleSpec> {
    vec![
        role(
            "VP Sales",
            "VP Sales",
            "Revenue strategy & leadership",
            true,
            "\
You set the revenue approach and the client reads your answer.

**What you do**
- Define the ideal customer profile, the segments, and the territories, and say
  why you have excluded the ones you have excluded.
- Decide the motion — who approaches whom, in what order, with what trigger.
- Ask for the missing brief — ICP, offer, price, cycle length — before
  dispatching. A pipeline plan built on an assumed deal size is fiction.

**What you produce**
A pipeline plan in `Shared/` before work starts, and the final synthesis naming
the accounts, the reasoning, and the expected next step for each.

**How you verify**
Every target traces to a stated assumption about volume, conversion, and cycle
length, and the assumptions are visible rather than buried in a total.",
            &["delegate", "research", "web_search", "memory_recall"],
        ),
        role(
            "SDR",
            "SDR",
            "Prospecting & qualification",
            false,
            "\
You find people worth talking to, and you can say why each one is.

**What you do**
- Build a list of specific accounts and specific people. A company name is not
  a contact; a role at a named company is.
- For each one, write the reason it is worth reaching out now. \"They are a
  fintech with 200 staff\" is a segment; \"they just posted a job for a
  compliance engineer, which means they are buying audit tooling\" is a reason.
- Write the outreach itself, tailored to that one account.

**What you produce**
A prospect table in `deliverables/`: account, contact and role, the trigger,
the angle, and the drafted message. Not a bulk template with names swapped in.

**How you verify**
Every contact traces to something you actually found. You never invent a
person, an email address, a funding round, or a headcount figure — an
unverified detail is left out or marked as needing confirmation.",
            &["research", "web_search", "note_manager", "memory_recall"],
        ),
        role(
            "Account Executive",
            "Account Executive",
            "Demos, objections & closing",
            false,
            "\
You turn interest into a decision, and you prepare for the pushback.

**What you do**
- Build the value narrative in the buyer's terms: their problem, what changes
  for them, and the proof that it does.
- Prepare the demo: the path through the product that shows the outcome, not
  the feature list.
- Anticipate the objections — price, timing, switching cost, trust — and answer
  each one honestly, including saying when you do not have the answer.

**What you produce**
A battlecard in `deliverables/`: the narrative, the demo path, the objection
responses, and the named risk in this deal.

**How you verify**
Every claim in the narrative has evidence behind it. You never quote a number,
a customer result, or a deadline you have not confirmed. If a deal needs
something the offer does not have, you say so rather than talking around it.",
            &["research", "file_write", "note_manager", "web_search"],
        ),
    ]
}

/// A design studio.
pub fn design_org() -> Vec<RoleSpec> {
    vec![
        role(
            "Design Director",
            "Design Director",
            "Vision & design system",
            true,
            "\
You set the visual direction for this studio and the client reads your answer.

**What you do**
- Decide the design system: the type scale, the spacing rhythm, the palette,
  the radii, the motion. Write it as values, not as adjectives.
- Brief the team so each piece extends the system rather than inventing a new
  one.
- Ask for the missing constraints — brand, platform, audience, accessibility
  requirement — before dispatching.

**What you produce**
The design system in `deliverables/`, as a document another designer can apply
without asking you a question, and the final synthesis of what the studio
produced and what is still missing.

**How you verify**
Every value in the system has a reason tied to a user problem. Contrast and
focus are checked, not assumed. If the studio's pieces disagree, you resolve it
in the system rather than shipping both.",
            &[
                "delegate",
                "research",
                "browser_navigate",
                "file_write",
                "memory_recall",
            ],
        ),
        role(
            "UX Researcher",
            "UX Researcher",
            "User flows & usability",
            false,
            "\
You work out what people are actually trying to do before anyone draws a screen.

**What you do**
- Map the real flows, including the ones nobody wants to talk about: the error
  path, the cancel path, the returning user, the user who is out of network.
- Ground each requirement in the product goal, and say which ones are assumed
  rather than established.
- Find the states a design has to handle: empty, loading, partial, error, full.

**What you produce**
A flows document in `deliverables/` covering the primary flow and every
alternate state, with the requirement behind each step.

**How you verify**
Each requirement traces to the goal you were given or to a stated assumption.
Assumptions are labelled as assumptions, because the designer will build
exactly what you specify and cannot tell which parts you are sure about.",
            &["research", "browser_navigate", "file_read", "note_manager"],
        ),
        role(
            "Product Designer",
            "Product Designer",
            "UI & prototyping",
            false,
            "\
You produce the interface itself, as something that can be looked at.

**What you do**
- Build real artifacts: HTML and CSS that renders, or an SVG that opens. Not a
  description of a screen.
- Handle every state the research named — empty, loading, error, full — not
  only the populated one.
- Meet the accessibility requirements: labels on every control, visible focus,
  keyboard reachability, and contrast you have actually measured.

**What you produce**
The working files in `deliverables/`, plus a short note on the interaction
decisions a static file cannot show: transitions, focus order, what happens on
submit.

**How you verify**
You have opened the file in a browser and looked at it, at the size it will be
used. Anything you could not build is named as such with the reason, rather
than left implied by its absence.",
            &[
                "image_gen",
                "screenshot",
                "analyze_image",
                "file_write",
                "prototype",
                "browser_navigate",
            ],
        ),
    ]
}

/// The Rot-Archive: research, synthesis, and verification.
pub fn rot_org() -> Vec<RoleSpec> {
    vec![
        role(
            "Grand Archivist",
            "Grand Archivist",
            "Codex leadership & verification",
            true,
            "\
You lead the Archive and the client reads your answer.

**What you do**
- Decide what has to be found out and assign it.
- Synthesise the specialists' findings into one authoritative answer, keeping
  their disagreements visible rather than averaging them into a bland
  consensus.
- Ask the client for the scope when the brief does not define it.

**What you produce**
The final answer in `deliverables/`, with every claim carrying the source that
supports it, and an explicit list of what remains unresolved.

**How you verify**
Each statement is traceable to a cited source the Archive actually read. If
the specialists conflict, you report the conflict and say which is better
supported. You do not present an inference as a quotation.",
            &[
                "delegate",
                "research",
                "web_search",
                "file_read",
                "memory_recall",
            ],
        ),
        role(
            "Hermetic Alchemist",
            "Alchemist",
            "Transmutation & synthesis",
            false,
            "\
You turn a pile of findings into something usable.

**What you do**
- Read the material and separate the load-bearing findings from the
  incidental ones.
- Produce a formulation: the steps, the inputs, the outputs, and the order
  they must happen in.
- Make it repeatable. If it only works once, it is not a formulation.

**What you produce**
A procedure in `deliverables/` that someone else could follow without asking
you a question, with each step's expected result stated.

**How you verify**
Every step traces back to a cited finding. Where two sources disagree, you say
so and pick the one you are following, and say why. You do not add a step that
no source supports because it seemed reasonable.",
            &["research", "file_write", "note_manager", "memory_recall"],
        ),
        role(
            "Inquisitor",
            "Inquisitor",
            "Verification & containment",
            false,
            "\
You check everything, including the parts your colleagues are confident about.

**What you do**
- Verify each claim against its cited source. Not the source's existence — its
  content at the relevant passage.
- Test the procedure where you can. A step that has never been executed is
  unverified, however sound it looks.
- Contain the dangerous: flag anything unsafe, unsupported, or likely to cause
  harm if followed literally.

**What you produce**
A verification report in `deliverables/`: confirmed claims with their sources,
refuted claims with the contradicting passage, and unverified claims marked as
such.

**How you verify**
You opened every source you are confirming. A claim you could not check is
reported as unverified, which is a useful result; a claim you did not check
being reported as confirmed is the one outcome that makes this office
dangerous.",
            &[
                "code_review",
                "research",
                "file_read",
                "shell_exec",
                "web_search",
            ],
        ),
    ]
}

/// The default team for a template.
pub fn default_org(template: &str) -> Vec<RoleSpec> {
    match template {
        "marketing" => marketing_org(),
        "sales" => sales_org(),
        "design" => design_org(),
        "rot-archive" => rot_org(),
        _ => engineering_org(),
    }
}

// ── prompt composition ──────────────────────────────────────────────────────

/// Build the full system prompt for a role in a specific office.
///
/// The mandate from the template is the body; the office supplies the
/// context that turns it into a working brief — the folder, the teammates, and
/// the policy. Composing them here, rather than at authoring time, is what lets
/// one template serve any office: the same Coder mandate is right whether the
/// office ships Rust or runs a marketing team, but the workspace path and the
/// names on the team are not.
///
/// A mandate already containing the full brief — one written by the lead
/// agent in a previous session and stored on the bot — is left alone, so
/// provisioning twice does not stack two office sections on one prompt.
pub fn compose_system_prompt(office: &OfficeContext, role: &RoleSpec) -> String {
    let mandate = role
        .system_prompt
        .as_deref()
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            format!(
                "You are the {} of the {} office. Your specialty is {}. \
                 Do that work properly, produce the artifact, and report what you \
                 did with the evidence.",
                role.name, office.name, role.specialty
            )
        });

    if mandate.contains("## Your workspace") {
        return mandate;
    }

    let mut s = String::new();
    // Name the role first. A mandate is written about the work, not about who
    // is doing it, so without this the agent never learns its own name — which
    // matters because the roster and the dispatch both refer to it.
    s.push_str(&format!(
        "You are {} — {} on the {} office team. {}\n\n",
        role.name, role.rank, office.name, role.specialty
    ));
    s.push_str(&mandate);
    s.push_str("\n\n# This office\n\n");

    if let Some(goal) = office
        .goal
        .as_deref()
        .map(str::trim)
        .filter(|g| !g.is_empty())
    {
        s.push_str("**Mission:** ");
        s.push_str(goal);
        s.push_str("\n\n");
    }

    s.push_str("## Your workspace\n\n");
    match office
        .workspace_root
        .as_deref()
        .map(str::trim)
        .filter(|r| !r.is_empty())
    {
        Some(root) => {
            s.push_str(&format!(
                "You work in `{root}`. You have full read and write access inside \
                 it and nowhere else on this computer.\n\n\
                 - `deliverables/` — finished work, named so a human can find it\n\
                 - `Shared/` — handoffs and the live status board (`Shared/STATUS.md`)\n\
                 - `notes/` — working notes and research\n\
                 - `OFFICE.md` — the mission, the roster, and the ground rules\n\n\
                 Read `OFFICE.md` first if you have not already. Write your result \
                 into `deliverables/` and leave a line in `Shared/STATUS.md` so \
                 nobody duplicates your work.\n\n",
            ));
        }
        None => {
            s.push_str(
                "Your working folder has not been set up yet. Create it under the \
                 office folder before producing anything, and put finished work in \
                 a `deliverables/` subfolder.\n\n",
            );
        }
    }

    if let Some(policy) = office
        .policy
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        // Quoted rather than inlined as headings: the policy is the operator's
        // document, and it should read as the rules it is.
        s.push_str("## Office policy (binding)\n\n");
        s.push_str("> ");
        s.push_str(&policy.replace("\n", "\n> "));
        s.push_str(
            "\n\nThe same text is in `POLICY.md` in your workspace. It outranks your \
                   own preferences.\n\n",
        );
    }

    if !office.roster.is_empty() {
        s.push_str("## The team\n\n");
        for m in &office.roster {
            if m.name == role.name {
                s.push_str(&format!(
                    "- **{}** ({} — {}) — that is you\n",
                    m.name, m.rank, m.specialty
                ));
            } else if m.is_lead {
                s.push_str(&format!(
                    "- **{}** ({} — {}) — leads this office; route anything that needs a \
                     decision or a client conversation through them\n",
                    m.name, m.rank, m.specialty
                ));
            } else {
                s.push_str(&format!(
                    "- **{}** ({} — {}) — hand work to them rather than doing it \
                     yourself\n",
                    m.name, m.rank, m.specialty
                ));
            }
        }
        s.push('\n');
    }

    let mut assigned: Vec<String> = Vec::new();
    if !role.skills.is_empty() {
        assigned.push(format!("**Skills:** {}", role.skills.join(", ")));
    }
    if !role.mcp_servers.is_empty() {
        assigned.push(format!(
            "**MCP connectors:** {}",
            role.mcp_servers.join(", ")
        ));
    }
    if !role.plugins.is_empty() {
        assigned.push(format!("**Plugin toolkits:** {}", role.plugins.join(", ")));
    }
    if !assigned.is_empty() {
        s.push_str("## Your assigned tools\n\n");
        for line in assigned {
            s.push_str("- ");
            s.push_str(&line);
            s.push('\n');
        }
        s.push_str(
            "\nThese are the tools enabled for this role. Callable function names are \
             the sanitized names in your tool list; a `/` in a skill id becomes `_` \
             plus a short suffix. If a task needs something that is not here, say so \
             rather than pretending it ran.\n\n",
        );
    }

    s.push_str("## How to report\n\n");
    s.push_str(
        "Your report is read by someone who cannot see your screen and does not have \
         your conversation. State what you changed or produced, the files involved, \
         and the command output that proves it. If something failed, say so and paste \
         the failure. If you were blocked, name what you need. Never report work as \
         done that you did not verify.\n\n\
         - If a tool fails, read the error and change the command or report the \
         blocker. Never rerun the same call unchanged; it will fail the same way.\n\
         - Start from your workspace. Do not search the entire filesystem; if \
         something is not in the workspace or your tool list, say so.\n\
         - Before you start, read `Shared/STATUS.md`. Before you finish, update it \
         with what you produced and where it lives.\n",
    );

    s
}

/// Attach the common skills a role did not already have, plus the baseline
/// tools its job cannot be done without.
///
/// The model-authored roster is told to choose skills, and it mostly does. When
/// it does not, the failure is ugly: a "Coder" with `memory_recall` and no
/// `file_write` talks about code it cannot save. The baseline is deliberately
/// keyword-based and conservative — it only ever *adds* allowlisted tools, so a
/// role the model gave a better toolkit keeps it.
pub fn with_common_skills(mut role: RoleSpec) -> RoleSpec {
    let hay = format!("{} {} {}", role.name, role.rank, role.specialty).to_lowercase();
    let has = |role: &RoleSpec, id: &str| role.skills.iter().any(|s| s == id);

    let mut needed: Vec<&str> = COMMON_SKILLS.to_vec();

    let is_lead = role.is_lead
        || hay.contains("ceo")
        || hay.contains("lead")
        || hay.contains("director")
        || hay.contains("chief");
    if is_lead {
        needed.extend(["delegate", "research", "file_read", "file_write"]);
    }
    if [
        "engineer",
        "developer",
        "coder",
        "programmer",
        "architect",
        "implementation",
    ]
    .iter()
    .any(|k| hay.contains(k))
    {
        needed.extend([
            "file_read",
            "file_write",
            "file_tree",
            "code_search",
            "code_edit",
            "shell_exec",
            "git",
            // Engineers need the vendor docs, the API reference, and the
            // issue thread as much as they need the local tree.
            "web_search",
            "browser_navigate",
        ]);
    }
    if [
        "qa",
        "tester",
        "test",
        "quality",
        "verification",
        "verifier",
    ]
    .iter()
    .any(|k| hay.contains(k))
    {
        needed.extend([
            "shell_exec",
            "file_read",
            "code_search",
            "browser_navigate",
            "screenshot",
        ]);
    }
    if ["design", "ux", "ui", "visual", "brand"]
        .iter()
        .any(|k| hay.contains(k))
    {
        needed.extend([
            "image_gen",
            "screenshot",
            "file_write",
            "browser_navigate",
            "visuals",
            "slides",
            "make_slides",
            "fframes",
        ]);
    }
    if ["marketing", "content", "seo", "writer", "copy", "growth"]
        .iter()
        .any(|k| hay.contains(k))
    {
        needed.extend([
            "web_search",
            "research",
            "file_write",
            "note_manager",
            "visuals",
            "slides",
            "make_slides",
            "fframes",
        ]);
    }
    if ["sales", "sdr", "account", "revenue"]
        .iter()
        .any(|k| hay.contains(k))
    {
        needed.extend(["research", "web_search", "file_write", "note_manager"]);
    }
    if ["devops", "infrastructure", "release", "platform", "sre"]
        .iter()
        .any(|k| hay.contains(k))
    {
        needed.extend([
            "shell_exec",
            "docker",
            "git",
            "env_manager",
            "task_runner",
            "web_search",
            "browser_navigate",
        ]);
    }

    for id in needed {
        if ALLOWED_SKILLS.contains(&id) && !has(&role, id) {
            role.skills.push(id.to_string());
        }
    }
    role
}

// ── LLM-authored orgs ───────────────────────────────────────────────────────

/// The prompt that asks the lead agent to propose a team.
pub fn org_prompt(
    office_name: &str,
    mission: Option<&str>,
    brief: &str,
    existing: &[(String, String)],
) -> String {
    let mut s = String::new();
    s.push_str(
        "You are the CEO of an AI office. Decide the team of agents required to \
         deliver the mission, then output it as JSON.\n\n",
    );
    s.push_str(&format!("Office: {office_name}\n"));
    if let Some(m) = mission.map(str::trim).filter(|m| !m.is_empty()) {
        s.push_str(&format!("Office mission: {m}\n"));
    }
    s.push_str(&format!("\nClient brief:\n{brief}\n\n"));
    if existing.is_empty() {
        s.push_str("Current team: (none — this is a brand new office)\n");
    } else {
        s.push_str("Current team (do NOT duplicate these people or roles):\n");
        for (name, rank) in existing {
            s.push_str(&format!("- {name} ({rank})\n"));
        }
    }
    s.push_str(
        "\nEach agent needs working instructions, not a job title. An agent whose \
         prompt is three sentences about its specialty will talk about the work and \
         not do it. For each one, state:\n\
         - what it owns,\n\
         - what artifact it must produce, and in what form,\n\
         - how it proves the work was done (a command, a file, a source),\n\
         - what it hands to the next person.\n\n\
         Write `system_prompt` as 150-400 words of plain prose or light markdown with \
         those four parts under their own headings. Do not use 'You are a helpful \
         assistant'. Do not repeat the same paragraph across roles — if two roles read \
         the same, one of them is wrong.\n\n\
         Output exactly this shape and nothing else:\n\n\
         {\"roles\":[{\"name\":\"<agent name>\",\"rank\":\"<short rank>\",\
         \"specialty\":\"<one line>\",\"system_prompt\":\"<its working instructions>\",\
         \"is_lead\":false,\"skills\":[\"<skill ids>\"]}],\"question\":null}\n\n",
    );
    s.push_str("Rules:\n");
    s.push_str("- Exactly one role has \"is_lead\": true. The lead plans the work, dispatches it, and gives the client the final answer. The lead does not do specialist work.\n");
    s.push_str("- 3-7 roles. Fewer is better than padded.\n");
    s.push_str("- Every skill must come from this list:\n");
    s.push_str(&format!("  {}\n", ALLOWED_SKILLS.join(", ")));
    s.push_str(&format!(
        "- The lead must have `delegate`{}.\n",
        if ALLOWED_SKILLS.contains(&"research") {
            " and `research`"
        } else {
            ""
        }
    ));
    s.push_str("- If the brief is too vague to build a team from, return {\"roles\":[],\"question\":\"<the one question you need answered>\"} instead.\n");
    s
}

/// Parse a proposed org out of a model response.
pub fn parse_org(text: &str) -> Option<ProposedOrg> {
    let json = extract_json_object(text)?;
    let mut org: ProposedOrg = serde_json::from_str(&json).ok()?;

    for r in org.roles.iter_mut() {
        // A model will invent plausible skill ids. Filtering is the boundary.
        r.skills.retain(|s| ALLOWED_SKILLS.contains(&s.as_str()));
        if r.is_lead {
            for needed in ["delegate", "memory_recall"] {
                if !r.skills.iter().any(|s| s == needed) {
                    r.skills.push(needed.to_string());
                }
            }
        }
    }

    if org.roles.is_empty()
        && org
            .question
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
    {
        return None;
    }
    org.roles.truncate(8);
    Some(org)
}

/// The prompt that asks the default model to design a whole office.
///
/// Unlike [`org_prompt`], this one is allowed to proceed on a name alone. The
/// user explicitly asked for an office that creates itself when the form is
/// half-empty, so "too vague, ask a question" is not an acceptable outcome; the
/// model writes the best charter it can and puts the one question it would ask
/// in `question`, alongside a usable team.
/// Static fallback prompt used by tests and by callers that do not have a
/// live registry (the app command uses [`blueprint_prompt_with_catalog`]).
#[allow(dead_code)]
pub fn blueprint_prompt(
    office_name: &str,
    mission: Option<&str>,
    template: &str,
    brief: &str,
) -> String {
    blueprint_prompt_with_catalog(
        office_name,
        mission,
        template,
        brief,
        &CapabilityCatalog::builtin_default(),
    )
}

/// The prompt that asks the default agent to design a whole office using the
/// tools that actually exist on this machine.
///
/// The catalog is the boundary: the model may only choose ids that appear in
/// it, and [`parse_blueprint_with_catalog`] enforces that afterwards. This is
/// what keeps an AI-designed team from being assigned a connector nobody
/// installed or a skill that was renamed.
pub fn blueprint_prompt_with_catalog(
    office_name: &str,
    mission: Option<&str>,
    template: &str,
    brief: &str,
    catalog: &CapabilityCatalog,
) -> String {
    let mut s = String::new();
    s.push_str(
        "You are the founder and CEO of an AI office. The user may have supplied \
         only a name, or a name and a partial mission. Make the office real: infer \
         the missing charter and hire the smallest team that can deliver it, using \
         only the tools that exist on this machine.\n\n",
    );
    s.push_str(&format!("Office name: {}\n", office_name.trim()));
    match mission.map(str::trim).filter(|m| !m.is_empty()) {
        Some(m) => s.push_str(&format!("User-supplied mission: {m}\n")),
        None => s.push_str("User-supplied mission: (not supplied — infer it from the name)\n"),
    }
    s.push_str(&format!(
        "Template hint (a starting point only — ignore it if the name and mission point somewhere else): {template}\n"
    ));
    if !brief.trim().is_empty() {
        s.push_str(&format!(
            "\nExtra context from the user:\n{}\n",
            brief.trim()
        ));
    }

    s.push_str(
        "\nProduce three things.\n\n\
         1. `goal` — 2-4 sentences. What this office must deliver, for whom, and \
         what \"done\" looks like. Concrete and checkable. If the mission was not \
         supplied, infer a credible one from the name and template; do not answer \
         with a question instead of a goal.\n\
         2. `policy` — 5-10 binding rules for how this office works. Cover evidence \
         (what counts as proof), where finished work lands, how handoffs are \
         written, and honesty about failure. Make it specific to this office; no \
         generic values and no filler.\n\
         3. `roles` — 3-6 agents, one of them the lead. Fewer is better than padded.\n\n",
    );

    s.push_str(
        "Each role needs working instructions, not a job title. For each one state:\n\
         - what it owns,\n\
         - what artifact it must produce, and in what form,\n\
         - how it proves the work was done (a command, a file, a source),\n\
         - what it hands to the next person.\n\n\
         Write `system_prompt` as 120-260 words of plain prose or light markdown with \
         those four parts under their own headings. The policy is written once, in \
         `policy`; do not repeat it in every mandate. Do not use 'You are a helpful \
         assistant'. Do not repeat the same paragraph across roles — if two roles read \
         the same, one of them is wrong.\n\n",
    );

    // ── The machine's real capability catalog ───────────────────────────────
    s.push_str("## What this installation can give an agent\n\n");
    s.push_str(
        "Assign only ids from the lists below. An id that is not here does not \
         exist on this machine; inventing one produces an agent that cannot act.\n\n",
    );

    s.push_str("### Built-in skills\n\n");
    if catalog.skills.is_empty() {
        s.push_str("- (none reported; use only the standard file, web, and shell tools)\n");
    } else {
        for kind in ["tool", "workflow"] {
            let group: Vec<&Capability> = catalog
                .skills
                .iter()
                .filter(|c| c.group.eq_ignore_ascii_case(kind))
                .collect();
            if group.is_empty() {
                continue;
            }
            s.push_str(if kind == "tool" {
                "**Tools** (function calls)\n"
            } else {
                "**Workflows** (instructions injected into the agent's prompt)\n"
            });
            for c in group {
                push_capability_line(&mut s, c);
            }
            s.push('\n');
        }
    }

    s.push_str("### MCP connectors (optional, only enabled/configured ones)\n\n");
    if catalog.connectors.is_empty() {
        s.push_str(
            "- (none enabled). Use `[]` for every role's `mcp_servers`; do not invent a connector id.\n\n",
        );
    } else {
        for c in &catalog.connectors {
            push_capability_line(&mut s, c);
        }
        s.push('\n');
    }

    s.push_str("### Installed plugin toolkits (optional)\n\n");
    if catalog.plugins.is_empty() {
        s.push_str("- (none installed). Use `[]` for every role's `plugins`.\n\n");
    } else {
        for c in &catalog.plugins {
            push_capability_line(&mut s, c);
        }
        s.push('\n');
    }

    s.push_str(
        "Output exactly this shape and nothing else:\n\n\
         {\"goal\":\"<the office goal>\",\"policy\":\"<the binding rules>\",\
         \"roles\":[{\"name\":\"<agent name>\",\"rank\":\"<short rank>\",\
         \"specialty\":\"<one line>\",\"system_prompt\":\"<its working instructions>\",\
         \"is_lead\":false,\"skills\":[\"<skill ids>\"],\
         \"mcp_servers\":[\"<connector ids>\"],\"plugins\":[\"<plugin ids>\"]}],\
         \"question\":null}\n\n",
    );

    s.push_str("Rules:\n");
    s.push_str("- Exactly one role has \"is_lead\": true. The lead plans, dispatches, and gives the client the final answer. The lead does not do specialist work.\n");
    s.push_str("- 3-6 roles. Do not invent a role just to fill a template.\n");
    s.push_str("- `skills` must be ids from the built-in skills list. The lead must have `delegate` and `memory_recall`.\n");
    s.push_str("- `mcp_servers` must be ids from the connector list; at most 4 per role. Empty means the agent inherits the installation's globally enabled connectors.\n");
    s.push_str("- `plugins` must be ids from the installed plugin list; at most 2 per role. Empty means none.\n");
    s.push_str("- Give a role a connector or plugin only when its job needs it. A generic \"nice to have\" assignment is a tool the agent will waste turns on.\n");
    s.push_str("- If one detail would materially change the work, put it in \"question\" as one sentence; still return the goal, policy, and team. Never return an empty roster.\n");
    s
}

/// One catalog row, compact enough that a 60-connector list still fits in a
/// normal context window.
fn push_capability_line(out: &mut String, c: &Capability) {
    let description = c
        .description
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let description = if description.chars().count() > 140 {
        format!("{}…", description.chars().take(140).collect::<String>())
    } else {
        description
    };
    out.push_str(&format!("- `{}` — {}", c.id, c.name));
    if !c.group.is_empty() && !c.group.eq_ignore_ascii_case("tool") {
        out.push_str(&format!(" ({})", c.group));
    }
    if !description.is_empty() {
        out.push_str(&format!(": {description}"));
    }
    if !c.extra.is_empty() {
        out.push_str(&format!(" [{}]", c.extra));
    }
    out.push('\n');
}

/// Parse a blueprinted office out of a model response.
///
/// Tolerant on purpose: models wrap JSON in fences, omit optional fields, and
/// occasionally return two leads. A structurally valid roster is worth keeping
/// even when the prose is imperfect — the alternative is discarding a good team
/// because one field was missing.
/// Parse with the static allowlist. The app command uses
/// [`parse_blueprint_with_catalog`]; this exists for tests and for callers with
/// no registry.
#[allow(dead_code)]
pub fn parse_blueprint(text: &str) -> Option<OfficeBlueprint> {
    parse_blueprint_with_catalog(text, None)
}

/// Parse a blueprint and filter every assignment against the live capability
/// catalog. A model that invents `mcp_servers: ["github"]` when GitHub is not
/// configured gets an empty list, not a broken agent.
pub fn parse_blueprint_with_catalog(
    text: &str,
    catalog: Option<&CapabilityCatalog>,
) -> Option<OfficeBlueprint> {
    if let Some(json) = extract_json_object(text) {
        if let Ok(bp) = serde_json::from_str::<OfficeBlueprint>(&json) {
            return normalize_blueprint(bp, catalog);
        }
    }
    // A long roster can hit the output-token cap mid-object. The goal, the
    // policy, and the roles completed before the cut are still usable; throwing
    // them away and hiring the template team is the worse outcome.
    parse_blueprint_truncated(text, catalog)
}

/// Normalise a parsed blueprint: trim, filter assignments, enforce one lead,
/// and reject a document with nothing in it.
fn normalize_blueprint(
    mut bp: OfficeBlueprint,
    catalog: Option<&CapabilityCatalog>,
) -> Option<OfficeBlueprint> {
    let allowed_skills: std::collections::HashSet<&str> = match catalog {
        Some(c) => c.skills.iter().map(|s| s.id.as_str()).collect(),
        None => ALLOWED_SKILLS.iter().copied().collect(),
    };
    let allowed_mcp: std::collections::HashSet<&str> = catalog
        .map(|c| c.connectors.iter().map(|s| s.id.as_str()).collect())
        .unwrap_or_default();
    let allowed_plugins: std::collections::HashSet<&str> = catalog
        .map(|c| c.plugins.iter().map(|s| s.id.as_str()).collect())
        .unwrap_or_default();

    for r in bp.roles.iter_mut() {
        r.name = r.name.trim().to_string();
        r.rank = r.rank.trim().to_string();
        r.specialty = r.specialty.trim().to_string();
        // A model will invent plausible ids. Filtering is the boundary.
        r.skills.retain(|s| allowed_skills.contains(s.as_str()));
        r.mcp_servers.retain(|s| allowed_mcp.contains(s.as_str()));
        r.plugins.retain(|s| allowed_plugins.contains(s.as_str()));
        r.mcp_servers.truncate(4);
        r.plugins.truncate(2);
    }
    bp.roles.retain(|r| !r.name.is_empty());
    // Two agents with the same name are indistinguishable in the roster, the
    // dispatch, and the transcript. The first one wins.
    let mut seen_names = std::collections::HashSet::new();
    bp.roles
        .retain(|r| seen_names.insert(r.name.to_lowercase()));

    // One lead, exactly. If the model returned none, the first role is the
    // natural lead; if it returned several, the first wins and the rest become
    // specialists, because two people giving the client an answer is worse
    // than one.
    if !bp.roles.is_empty() {
        let lead = bp.roles.iter().position(|r| r.is_lead).unwrap_or(0);
        for (i, r) in bp.roles.iter_mut().enumerate() {
            r.is_lead = i == lead;
            r.rank = if r.rank.is_empty() {
                if r.is_lead {
                    "Lead".to_string()
                } else {
                    "Specialist".to_string()
                }
            } else {
                r.rank.clone()
            };
            r.specialty = if r.specialty.is_empty() {
                "Generalist".to_string()
            } else {
                r.specialty.clone()
            };
        }
        for needed in ["delegate", "memory_recall"] {
            if !bp.roles[lead].skills.iter().any(|s| s == needed) {
                bp.roles[lead].skills.push(needed.to_string());
            }
        }
    }
    bp.roles.truncate(7);
    bp.goal = bp.goal.trim().to_string();
    bp.policy = bp.policy.trim().to_string();

    if bp.roles.is_empty() && bp.goal.is_empty() && bp.policy.is_empty() {
        return None;
    }
    Some(bp)
}

/// Read a JSON string field without requiring the object around it to close.
///
/// Used only for the truncation path. Returns `None` for an unterminated string,
/// because half a sentence is not a goal.
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
                'b' => '\u{0008}',
                'f' => '\u{000c}',
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

/// Collect every complete role object from a possibly truncated `roles` array.
fn role_objects_from_truncated(text: &str) -> Vec<RoleSpec> {
    let Some(key) = text.find("\"roles\"") else {
        return Vec::new();
    };
    let Some(open) = text[key..].find('[') else {
        return Vec::new();
    };
    let rest = &text[key + open + 1..];

    let mut roles = Vec::new();
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
                        if let Ok(role) = serde_json::from_str::<RoleSpec>(&rest[start..=i]) {
                            roles.push(role);
                        }
                    }
                }
            }
            ']' if depth == 0 => break,
            _ => {}
        }
    }
    roles
}

fn parse_blueprint_truncated(
    text: &str,
    catalog: Option<&CapabilityCatalog>,
) -> Option<OfficeBlueprint> {
    let bp = OfficeBlueprint {
        goal: json_string_field(text, "goal").unwrap_or_default(),
        policy: json_string_field(text, "policy").unwrap_or_default(),
        roles: role_objects_from_truncated(text),
        question: json_string_field(text, "question"),
    };
    normalize_blueprint(bp, catalog)
}

/// Whether a proposed team is specific enough to provision.
///
/// A roster of titles with one-line personas is worse than the template: it
/// looks custom while giving every agent the same vague instructions. The
/// command falls back to the template unless the team carries real mandates.
pub fn blueprint_roles_are_usable(roles: &[RoleSpec]) -> bool {
    if roles.len() < 2 {
        return false;
    }
    let leads = roles.iter().filter(|r| r.is_lead).count();
    let substantive = roles
        .iter()
        .filter(|r| {
            r.system_prompt
                .as_deref()
                .map(mandate_is_substantive)
                .unwrap_or(false)
        })
        .count();
    leads == 1 && substantive >= roles.len().div_ceil(2)
}

/// The first balanced `{…}` object in `text`, ignoring braces inside strings.
///
/// Tolerant of a ```json fence and of the model writing prose around the
/// object, because both are what models actually do. The scan is string- and
/// escape-aware so a brace inside a JSON string value does not end the object
/// early — a mistake that made a perfectly good roster unparseable.
fn extract_json_object(text: &str) -> Option<String> {
    let mut body = text.trim();
    if body.starts_with("```") {
        let after = body.find('\n').map(|i| &body[i + 1..]).unwrap_or(body);
        body = after.split("```").next().unwrap_or(after);
    }

    let start = body.find('{')?;
    let bytes: Vec<char> = body[start..].chars().collect();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (i, c) in bytes.iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if *c == '\\' {
                escaped = true;
            } else if *c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(bytes[..=i].iter().collect());
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether a mandate is worth storing on a bot.
///
/// Replaces the old `len() > 40` check, which a two-sentence persona passes.
/// A usable mandate names a responsibility, an artifact, and a way of proving
/// the work — so the floor is a word count plus the structure, and the
/// structure is what actually matters.
pub fn mandate_is_substantive(prompt: &str) -> bool {
    let text = prompt.trim();
    if text.split_whitespace().count() < 40 {
        return false;
    }
    // A mandate that only says "you are a …" is a persona, not instructions.
    // Structure can be bold labels, markdown headings, or the older
    // "What you do" prose — the point is that the four parts are separated.
    let lower = text.to_lowercase();
    let structured = lower.contains("you do")
        || lower.contains("what you")
        || lower.contains("**")
        || lower.contains("\n## ")
        || lower.starts_with("## ");
    if !structured {
        return false;
    }
    // And it has to say how the work is evidenced.
    lower.contains("verify")
        || lower.contains("evidence")
        || lower.contains("paste")
        || lower.contains("source")
        || lower.contains("output")
        || lower.contains("test")
        || lower.contains("proof")
        || lower.contains("reproduce")
        || lower.contains("trace")
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEMPLATES: &[&str] = &[
        "it-office",
        "custom",
        "marketing",
        "sales",
        "design",
        "rot-archive",
    ];

    #[test]
    fn every_template_teams_a_substantive_office() {
        for template in TEMPLATES {
            let org = default_org(template);
            assert!(org.len() >= 3, "{template}: only {} roles", org.len());

            let leads: Vec<&RoleSpec> = org.iter().filter(|r| r.is_lead).collect();
            assert_eq!(leads.len(), 1, "{template}: {} leads", leads.len());

            for r in &org {
                let mandate = r.system_prompt.as_deref().unwrap_or_default();
                assert!(
                    mandate_is_substantive(mandate),
                    "{template}/{}: mandate is not substantive ({} words)",
                    r.name,
                    mandate.split_whitespace().count()
                );
                assert!(!r.skills.is_empty(), "{template}/{}: no skills", r.name);
            }
        }
    }

    #[test]
    fn no_two_roles_share_a_mandate() {
        // Padded orgs where every agent reads the same are worse than a small
        // one: the client cannot tell who did what.
        for template in TEMPLATES {
            let org = default_org(template);
            for (i, a) in org.iter().enumerate() {
                for b in org.iter().skip(i + 1) {
                    assert_ne!(
                        a.system_prompt, b.system_prompt,
                        "{template}: {} and {} share a mandate",
                        a.name, b.name
                    );
                }
            }
        }
    }

    #[test]
    fn every_allowlisted_skill_exists_in_the_registry() {
        // The allowlist is what an office can hand an agent, so a stale entry
        // is a tool that can never be given and a wrong one is a lie in the
        // prompt.
        let registry = ravenbot_skills::SkillRegistry::new_builtin();
        for id in ALLOWED_SKILLS {
            assert!(
                registry.get(id).is_some(),
                "ALLOWED_SKILLS lists {id}, which is not in the registry"
            );
        }
    }

    #[test]
    fn every_tool_name_is_provider_safe_and_unique() {
        // OpenAI-shaped gateways reject `tools[].function.name` unless it
        // matches ^[a-zA-Z0-9_-]+$. Awesome/plugin ids contain `/` and `.`, and
        // one of them in the tools array used to fail the whole request with a
        // 400 before the model saw it.
        let registry = ravenbot_skills::SkillRegistry::new_builtin();
        let mut seen = std::collections::HashSet::new();
        for skill in registry.list() {
            let name = skill.tool_name();
            assert!(
                !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
                "skill {} produced unsafe callable name {name}",
                skill.id()
            );
            assert!(
                seen.insert(name.clone()),
                "two skills share the callable name {name}"
            );
        }
    }

    #[test]
    fn common_skills_are_in_the_allowlist() {
        for s in COMMON_SKILLS {
            assert!(ALLOWED_SKILLS.contains(s), "{s} missing from allowlist");
        }
    }

    #[test]
    fn every_template_only_grants_allowlisted_skills() {
        for template in TEMPLATES {
            for r in default_org(template) {
                for s in &r.skills {
                    assert!(
                        ALLOWED_SKILLS.contains(&s.as_str()),
                        "{template}/{} has unknown skill {s}",
                        r.name
                    );
                }
            }
        }
    }

    #[test]
    fn a_lead_can_delegate() {
        for template in TEMPLATES {
            let lead = default_org(template)
                .into_iter()
                .find(|r| r.is_lead)
                .expect("a lead");
            assert!(
                lead.skills.iter().any(|s| s == "delegate"),
                "{template}: {} cannot delegate",
                lead.name
            );
        }
    }

    #[test]
    fn the_brief_names_the_role_assigned_tools() {
        let office = OfficeContext::new("Acme");
        let mut role = default_org("it-office")
            .into_iter()
            .find(|r| r.name == "Coder")
            .unwrap();
        role.mcp_servers = vec!["github".into()];
        role.plugins = vec!["notion".into()];
        let prompt = compose_system_prompt(&office, &role);
        assert!(prompt.contains("**MCP connectors:** github"));
        assert!(prompt.contains("**Plugin toolkits:** notion"));
        assert!(prompt.contains("## Your assigned tools"));
    }

    #[test]
    fn composing_gives_the_agent_its_workspace_and_team() {
        let office = OfficeContext::new("Acme")
            .with_goal(Some("Ship v1 on Friday".into()))
            .with_policy(Some(template_policy("it-office").into()))
            .with_workspace(Some("/home/u/RAVENBOT/projects/acme".into()))
            .with_roster(vec![
                Teammate::new("CEO", "CEO", "Orchestration", true),
                Teammate::new("Coder", "Developer", "Implementation", false),
            ]);
        let coder = default_org("it-office")
            .into_iter()
            .find(|r| r.name == "Coder")
            .unwrap();
        let prompt = compose_system_prompt(&office, &coder);

        // The agent learns its own identity, not just its job: the roster and
        // every dispatch from the lead refer to it by name.
        assert!(prompt.contains("You are Coder — Developer on the Acme office team"));
        // The mandate body survives composition.
        assert!(prompt.contains("Read the surrounding code"), "mandate lost");
        assert!(
            prompt.contains("/home/u/RAVENBOT/projects/acme"),
            "no workspace path"
        );
        assert!(prompt.contains("deliverables/"), "no layout");
        assert!(prompt.contains("Ship v1 on Friday"), "no mission");
        assert!(
            prompt.contains("Office policy (binding)"),
            "no policy section"
        );
        assert!(prompt.contains("leads this office"), "no lead marked");
        assert!(
            prompt.contains("**Coder** (Developer — Implementation) — that is you"),
            "the agent is not told which entry is itself"
        );
    }

    #[test]
    fn composing_twice_does_not_stack_the_office_section() {
        let office = OfficeContext::new("Acme").with_workspace(Some("/tmp/acme".into()));
        let coder = default_org("it-office")
            .into_iter()
            .find(|r| r.name == "Coder")
            .unwrap();
        let once = compose_system_prompt(&office, &coder);
        let twice = compose_system_prompt(&office, &coder);
        assert_eq!(once, twice);
        assert_eq!(once.matches("## Your workspace").count(), 1);
    }

    #[test]
    fn a_role_with_no_mandate_still_gets_a_usable_brief() {
        let office = OfficeContext::new("Acme");
        let bare = RoleSpec {
            name: "Intern".into(),
            rank: "Intern".into(),
            specialty: "Fetching coffee".into(),
            system_prompt: None,
            is_lead: false,
            skills: vec![],
            mcp_servers: vec![],
            plugins: vec![],
            avatar_style: None,
        };
        let prompt = compose_system_prompt(&office, &bare);
        assert!(prompt.contains("You are the Intern of the Acme office"));
        assert!(prompt.contains("How to report"), "no reporting section");
    }

    #[test]
    fn common_skills_are_added_without_duplicating() {
        let role = with_common_skills(role("X", "X", "X", false, "do the thing well", &["todo"]));
        assert_eq!(role.skills.iter().filter(|s| *s == "todo").count(), 1);
        assert!(role.skills.contains(&"memory_recall".to_string()));
        assert!(role.skills.contains(&"ask_user".to_string()));
    }

    #[test]
    fn every_template_has_a_policy_with_checkable_rules() {
        for template in TEMPLATES {
            let p = template_policy(template);
            assert!(p.len() > 200, "{template}: policy is a stub");
            assert!(
                p.contains("deliverables/"),
                "{template}: no file convention"
            );
            // A policy with no negatives is a values statement, not a rule set.
            assert!(
                p.contains("never") || p.contains("not ") || p.contains("Do not"),
                "{template}: policy has no prohibitions"
            );
        }
    }

    #[test]
    fn the_it_office_policy_bans_the_specific_failure_modes() {
        let p = template_policy("it-office");
        assert!(p.contains("Read before you change"));
        assert!(p.contains("paste"), "no evidence requirement");
        assert!(p.contains("placeholder"), "placeholder ban missing");
    }

    #[test]
    fn parses_a_proposed_org_and_filters_bad_skills() {
        let text = r#"Here you go:
```json
{"roles":[
  {"name":"CEO","rank":"CEO","specialty":"Lead","system_prompt":"Plan the work and give the client the answer with evidence from the team.","is_lead":true,"skills":["delegate","research"]},
  {"name":"Coder","rank":"Dev","specialty":"Build","system_prompt":"Read the code, make the change, run the build, and paste the real output as proof.","is_lead":false,"skills":["code_edit","bogus_skill"]}
],"question":null}
```"#;
        let org = parse_org(text).expect("should parse");
        assert_eq!(org.roles.len(), 2);
        let coder = org.roles.iter().find(|r| r.name == "Coder").unwrap();
        assert!(coder.skills.contains(&"code_edit".to_string()));
        assert!(!coder.skills.contains(&"bogus_skill".to_string()));
        let ceo = org.roles.iter().find(|r| r.name == "CEO").unwrap();
        assert!(ceo.skills.contains(&"delegate".to_string()));
        assert!(ceo.skills.contains(&"memory_recall".to_string()));
    }

    #[test]
    fn a_brace_inside_a_string_does_not_end_the_object_early() {
        let text = r#"{"roles":[{"name":"A","rank":"R","specialty":"S","system_prompt":"Use the { placeholder } and a } brace.","is_lead":true,"skills":[]}],"question":null}"#;
        let org = parse_org(text).expect("should parse a brace inside a string");
        assert_eq!(org.roles[0].name, "A");
        assert!(org.roles[0].system_prompt.clone().unwrap().contains('}'));
    }

    #[test]
    fn an_escaped_quote_inside_a_string_is_handled() {
        let text = r#"{"roles":[{"name":"A","rank":"R","specialty":"S","system_prompt":"Say \"hi\" and { stop }","is_lead":false,"skills":[]}]}"#;
        assert_eq!(parse_org(text).unwrap().roles[0].name, "A");
    }

    #[test]
    fn parses_a_clarifying_question() {
        let org = parse_org(r#"{"roles":[],"question":"What is the target audience?"}"#).unwrap();
        assert!(org.roles.is_empty());
        assert_eq!(org.question.unwrap(), "What is the target audience?");
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_org("no json here").is_none());
        assert!(parse_org(r#"{"roles":[]}"#).is_none());
        assert!(parse_org(r#"{"unclosed": "#).is_none());
    }

    #[test]
    fn a_roster_is_capped() {
        let mut roles = Vec::new();
        for i in 0..20 {
            roles.push(format!(
                "{{\"name\":\"A{i}\",\"rank\":\"R\",\"specialty\":\"S\",\"system_prompt\":\"x\",\"is_lead\":false,\"skills\":[]}}"
            ));
        }
        let text = format!("{{\"roles\":[{}]}}", roles.join(","));
        assert_eq!(parse_org(&text).unwrap().roles.len(), 8);
    }

    #[test]
    fn the_org_prompt_demands_an_artifact_and_evidence() {
        let p = org_prompt("Acme", Some("Ship v1"), "We need a website", &[]);
        assert!(
            p.contains("what artifact it must produce"),
            "no artifact requirement"
        );
        assert!(p.contains("how it proves"), "no evidence requirement");
        assert!(p.contains("150-400 words"), "no length guidance");
        assert!(
            p.contains("You are a helpful assistant"),
            "no anti-generic guard"
        );
        // The allowlist must actually be in the prompt the model reads.
        for s in ["note_manager", "env_manager", "shell_exec"] {
            assert!(p.contains(s), "{s} missing from the prompt");
        }
    }

    #[test]
    fn the_org_prompt_lists_existing_teammates() {
        let p = org_prompt("Acme", None, "brief", &[("CEO".into(), "CEO".into())]);
        assert!(p.contains("do NOT duplicate"));
        assert!(p.contains("- CEO (CEO)"));
    }

    #[test]
    fn the_substantive_check_rejects_a_persona() {
        // The old gate was `len() > 40`, which all three of these pass.
        assert!(!mandate_is_substantive("You are a helpful assistant."));
        assert!(!mandate_is_substantive("You are the Coder. Write code."));
        // Long enough, but still a persona with no instruction and no evidence.
        assert!(!mandate_is_substantive(
            &"You are a careful engineer. ".repeat(6)
        ));

        // A real template mandate passes, which is the point of the check.
        for template in TEMPLATES {
            for r in default_org(template) {
                assert!(
                    mandate_is_substantive(r.system_prompt.as_deref().unwrap()),
                    "{template}/{}",
                    r.name
                );
            }
        }

        // A model-authored mandate with markdown headings and no "What you do"
        // phrase is still a mandate; the live blueprint uses this shape.
        let markdown = "## Ownership\nOwn the engagement and the client relationship. \
            ## Artifact\nProduce the plan and the final report, both linked from the \
            office index. ## Proof\nEvery claim traces to a file in the office, and the \
            report is checked against the acceptance criteria before delivery. \
            ## Handoff\nGive the reviewer the draft and the list of open risks, with the \
            exact artifact paths and the date of the next review.";
        assert!(mandate_is_substantive(markdown));
    }

    #[test]
    fn a_blueprint_can_be_made_from_a_name_alone() {
        let prompt = blueprint_prompt("Nightwatch", None, "custom", "");
        assert!(prompt.contains("Nightwatch"));
        assert!(prompt.contains("infer it from the name"));
        assert!(prompt.contains("Never return an empty roster"));
        assert!(prompt.contains("\"goal\""));
        assert!(prompt.contains("\"policy\""));
        assert!(
            prompt.contains("shell_exec"),
            "allowlist must reach the model"
        );
    }

    #[test]
    fn a_blueprint_parses_charter_and_team_together() {
        let text = r#"
Here is the office.
```json
{
  "goal": "Ship a reviewed security assessment for one authorized web app.",
  "policy": "Evidence over adjectives. Every finding cites a request and response.",
  "roles": [
    {"name":"Lead","rank":"Director","specialty":"Delivery","system_prompt":"You own the outcome for this office and the client reads your answer. What you produce: the brief before work starts, and the final report after. You decide who does what and dispatch it; a specialist cannot see this conversation, so every task must stand on its own. How you verify: every claim in your answer traces to a specialist's output or a file in the office, and you paste the command or quote the source that proves it. What you hand to the next person: a clear statement of what is done, what is not, and why.","is_lead":true,"skills":["delegate","research"]},
    {"name":"Tester","rank":"QA","specialty":"Verification","system_prompt":"You find out whether the work actually works, and you prove it either way. What you produce: tests that fail for the right reason, and a results summary in the shared folder. You cover the edges: empty input, the boundary, the value that should be rejected, and two things happening at once. How you verify: you run the suite yourself and paste the real output. A test you did not run is labelled unrun, and a failure is reported with its assertion message rather than softened. What you hand to the next person: the exact reproduction for every failure, so nobody has to guess.","is_lead":false,"skills":["shell_exec","file_read"]}
  ],
  "question": "Which domain is authorized?"
}
```
"#;
        let bp = parse_blueprint(text).expect("must parse");
        assert!(bp.goal.contains("reviewed security assessment"));
        assert!(bp.policy.contains("Evidence over adjectives"));
        assert_eq!(bp.roles.len(), 2);
        assert_eq!(bp.roles[0].name, "Lead");
        assert!(bp.roles[0].skills.contains(&"memory_recall".to_string()));
        assert_eq!(bp.question.as_deref(), Some("Which domain is authorized?"));
        assert!(blueprint_roles_are_usable(&bp.roles));
    }

    #[test]
    fn a_blueprint_with_several_leads_keeps_exactly_one() {
        let text = r#"{"goal":"g","policy":"p","roles":[
            {"name":"A","rank":"CEO","specialty":"x","system_prompt":"What you produce: x. How you verify: y.","is_lead":true,"skills":[]},
            {"name":"B","rank":"CTO","specialty":"y","system_prompt":"What you produce: x. How you verify: y.","is_lead":true,"skills":[]}
        ]}"#;
        let bp = parse_blueprint(text).unwrap();
        assert_eq!(bp.roles.iter().filter(|r| r.is_lead).count(), 1);
        assert!(bp.roles[0].is_lead);
        assert!(!bp.roles[1].is_lead);
    }

    #[test]
    fn a_blueprint_with_only_a_charter_is_still_kept() {
        // The team can come from the template; throwing away a usable goal
        // because the roster failed would be the wrong trade.
        let bp = parse_blueprint(r#"{"goal":"Do the thing","policy":"Be honest","roles":[]}"#)
            .expect("charter-only blueprint");
        assert_eq!(bp.goal, "Do the thing");
        assert!(bp.roles.is_empty());
        assert!(!blueprint_roles_are_usable(&bp.roles));
    }

    fn cap(id: &str, group: &str) -> Capability {
        Capability {
            id: id.to_string(),
            name: id.to_string(),
            group: group.to_string(),
            description: format!("{id} description"),
            extra: String::new(),
        }
    }

    #[test]
    fn the_prompt_lists_the_live_capability_catalog() {
        let catalog = CapabilityCatalog {
            skills: vec![cap("file_write", "tool"), cap("code_review", "workflow")],
            connectors: vec![cap("github", "Development & Coding")],
            plugins: vec![cap("notion", "plugin")],
        };
        let p = blueprint_prompt_with_catalog("Acme", Some("Ship it"), "custom", "", &catalog);
        assert!(p.contains("`file_write`"), "tool missing from prompt");
        assert!(p.contains("`code_review`"), "workflow missing from prompt");
        assert!(p.contains("`github`"), "connector missing from prompt");
        assert!(p.contains("`notion`"), "plugin missing from prompt");
        assert!(p.contains("\"mcp_servers\""), "connector field in shape");
        assert!(p.contains("\"plugins\""), "plugin field in shape");
    }

    #[test]
    fn catalog_assignments_are_filtered_to_real_ids() {
        let catalog = CapabilityCatalog {
            skills: vec![cap("delegate", "tool"), cap("file_write", "tool")],
            connectors: vec![cap("github", "Development & Coding")],
            plugins: vec![cap("notion", "plugin")],
        };
        let text = r#"{"goal":"g","policy":"p","roles":[
            {"name":"Lead","rank":"CEO","specialty":"x","system_prompt":"Own the outcome. What you produce: the report. How you verify: evidence.","is_lead":true,"skills":["delegate","invented"],"mcp_servers":["github","invented"],"plugins":["notion","invented"]}
        ]}"#;
        let bp = parse_blueprint_with_catalog(text, Some(&catalog)).expect("parse");
        let role = &bp.roles[0];
        assert!(role.skills.iter().any(|s| s == "delegate"));
        assert!(!role.skills.iter().any(|s| s == "invented"));
        assert_eq!(role.mcp_servers, vec!["github"]);
        assert_eq!(role.plugins, vec!["notion"]);
        // The lead still gets the mandatory memory tool even though the model
        // did not list it.
        assert!(role.skills.iter().any(|s| s == "memory_recall"));
    }

    #[test]
    fn with_no_catalog_connector_assignments_are_cleared() {
        // Legacy callers have no registry to validate against, so they must not
        // silently create agents with unverified connector ids.
        let text = r#"{"goal":"g","policy":"p","roles":[
            {"name":"Lead","rank":"CEO","specialty":"x","system_prompt":"Own the outcome. What you produce: the report. How you verify: evidence.","is_lead":true,"skills":["delegate"],"mcp_servers":["github"],"plugins":["notion"]}
        ]}"#;
        let bp = parse_blueprint(text).expect("parse");
        assert!(bp.roles[0].mcp_servers.is_empty());
        assert!(bp.roles[0].plugins.is_empty());
    }

    #[test]
    fn a_truncated_blueprint_keeps_the_completed_roles() {
        // The live model wrote a strong charter and four roles, then hit the
        // output-token cap in the middle of the fourth. Falling back to the
        // template would have thrown all of it away.
        let text = r#"{"goal":"Assess authorized web apps and report verified findings.","policy":"Evidence over adjectives.","roles":[
            {"name":"Lead","rank":"Principal","specialty":"Delivery","system_prompt":"Own the outcome.","is_lead":true,"skills":["delegate"]},
            {"name":"Tester","rank":"QA","specialty":"Verification","system_prompt":"Test it.","is_lead":false,"skills":["shell_exec"]},
            {"name":"Reviewer","rank":"Reviewer","specialty":"Evidence","system_prompt":"This one is cut off mid"#;
        let bp = parse_blueprint(text).expect("truncated but usable");
        assert_eq!(
            bp.goal,
            "Assess authorized web apps and report verified findings."
        );
        assert_eq!(bp.policy, "Evidence over adjectives.");
        assert_eq!(bp.roles.len(), 2, "only complete role objects survive");
        assert!(bp.roles[0].is_lead);
        assert!(!bp.roles[1].is_lead);
    }

    #[test]
    fn a_persona_roster_is_not_usable() {
        let mut roles = default_org("it-office");
        for r in &mut roles {
            r.system_prompt = Some("You are a helpful assistant.".to_string());
        }
        assert!(!blueprint_roles_are_usable(&roles));

        let real = default_org("it-office");
        assert!(blueprint_roles_are_usable(&real));
    }

    #[test]
    fn a_coder_without_tools_gets_the_ones_the_job_needs() {
        let role = RoleSpec {
            name: "Coder".into(),
            rank: "Developer".into(),
            specialty: "Implementation".into(),
            system_prompt: None,
            is_lead: false,
            skills: vec!["memory_recall".into()],
            mcp_servers: vec![],
            plugins: vec![],
            avatar_style: None,
        };
        let role = with_common_skills(role);
        for id in ["file_read", "file_write", "code_edit", "shell_exec", "git"] {
            assert!(role.skills.iter().any(|s| s == id), "coder missing {id}");
        }
        // No duplicate of a skill the model already chose.
        assert_eq!(
            role.skills.iter().filter(|s| *s == "memory_recall").count(),
            1
        );
    }

    #[test]
    fn a_lead_can_always_delegate() {
        let role = RoleSpec {
            name: "Nova".into(),
            rank: "Delivery Lead".into(),
            specialty: "Outcomes".into(),
            system_prompt: None,
            is_lead: true,
            skills: vec![],
            mcp_servers: vec![],
            plugins: vec![],
            avatar_style: None,
        };
        let role = with_common_skills(role);
        assert!(role.skills.iter().any(|s| s == "delegate"));
        assert!(role.skills.iter().any(|s| s == "research"));
    }
}
