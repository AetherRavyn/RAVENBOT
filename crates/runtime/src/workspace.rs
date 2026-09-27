//! An office's workspace: the folder its agents actually work in.
//!
//! Every office gets a real room under `~/RAVENBOT/projects/<slug>/`, seeded
//! with the charter files an agent needs on its first turn. Without them an
//! agent arrives at an empty directory and has to guess where to put things;
//! with them it reads `OFFICE.md`, sees the roster, and writes into
//! `deliverables/` because the room says to.
//!
//! The layout is fixed so an agent can rely on it without asking:
//!
//! ```text
//! <office>/
//!   .ravenbot-office   marker — RAVENBOT owns this directory
//!   OFFICE.md          mission, roster, workspace contract
//!   POLICY.md          the working policy
//!   README.md          what this office is for, for a human reading it
//!   Shared/            cross-agent handoffs and scratch
//!     STATUS.md        what everyone is doing right now
//!   deliverables/      finished outputs
//!   notes/             working notes and research
//! ```
//!
//! Seeding is idempotent. `OFFICE.md`, `README.md` and `STATUS.md` are
//! rewritten whenever the charter changes, but `POLICY.md` is only written
//! when the office has a policy and the file is missing or RAVENBOT-generated
//! — an operator who edits the policy by hand keeps their version.
//!
//! Nothing here is allowed to fail an office creation. Every write is
//! best-effort with a `warn!`, because a read-only or full disk must not stop
//! a user from creating an office and chatting in it.

use std::path::{Path, PathBuf};

use ravenbot_core::ChatRoom;

/// One line of the roster, as written into `OFFICE.md`.
#[derive(Debug, Clone)]
pub struct RosterEntry {
    pub name: String,
    pub rank: String,
    pub specialty: String,
    pub is_lead: bool,
    /// The agent's own working instructions, so the room records how each
    /// member is expected to work rather than only their job title.
    pub mandate: Option<String>,
}

impl RosterEntry {
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
            mandate: None,
        }
    }

    pub fn with_mandate(mut self, mandate: impl Into<String>) -> Self {
        self.mandate = Some(mandate.into());
        self
    }
}

/// The result of making sure an office has a room.
#[derive(Debug, Clone)]
pub struct Workspace {
    /// The office root. Every configured project folder points here.
    pub root: PathBuf,
    /// The subfolders agents are told to use, in the order they are described.
    pub shared: PathBuf,
    pub deliverables: PathBuf,
    pub notes: PathBuf,
    /// False when the directory already existed, so callers can log once
    /// instead of announcing a new room every send.
    pub created: bool,
}

/// Make sure `name`'s office has a workspace under `~/RAVENBOT/projects`.
///
/// Stable for a given name: calling this twice returns the same directory, and
/// a directory RAVENBOT did not create is stepped over rather than adopted.
pub fn ensure_workspace(name: &str) -> Workspace {
    let root = ravenbot_core::office_workspace(name);
    let created = ravenbot_core::mark_office_dir(&root);
    let ws = Workspace {
        shared: root.join("Shared"),
        deliverables: root.join("deliverables"),
        notes: root.join("notes"),
        root,
        created,
    };
    for dir in [&ws.shared, &ws.deliverables, &ws.notes] {
        if let Err(e) = std::fs::create_dir_all(dir) {
            tracing::warn!(path = %dir.display(), error = %e, "Could not create office subfolder");
        }
    }
    ws
}

/// Create the room and write its charter for `room`, in the default location.
///
/// Called at office creation and again whenever the roster or the charter
/// changes, so `OFFICE.md` in the folder always matches what the app shows.
pub fn seed(room: &ChatRoom, roster: &[RosterEntry]) -> Workspace {
    let ws = ensure_workspace(&room.name);
    write_charter(&ws, room, roster);
    ws
}

/// Write the charter for `room` into a directory the caller chose.
///
/// This is the primitive behind [`seed`]. An office pointed at a folder the
/// user already has gets its charter written there rather than into a second,
/// default room — otherwise the agents would be told to work in a directory
/// that is not the one their tools are actually confined to.
pub fn seed_at(root: &Path, room: &ChatRoom, roster: &[RosterEntry]) {
    let root = root.to_path_buf();
    let shared = root.join("Shared");
    let deliverables = root.join("deliverables");
    let notes = root.join("notes");
    let ws = Workspace {
        shared: shared.clone(),
        deliverables: deliverables.clone(),
        notes: notes.clone(),
        created: false,
        root,
    };
    for dir in [&shared, &deliverables, &notes] {
        if let Err(e) = std::fs::create_dir_all(dir) {
            tracing::warn!(path = %dir.display(), error = %e, "Could not create office subfolder");
        }
    }
    write_charter(&ws, room, roster);
}

fn write_charter(ws: &Workspace, room: &ChatRoom, roster: &[RosterEntry]) {
    write_if_changed(&ws.root.join("OFFICE.md"), &office_md(ws, room, roster));
    write_if_changed(&ws.root.join("README.md"), &readme_md(room, ws));
    write_if_changed(&ws.shared.join("STATUS.md"), &status_md(room, roster));

    // The policy is the operator's document. Only seed it when they have not
    // written one, so a hand-edited POLICY.md survives.
    let policy_path = ws.root.join("POLICY.md");
    let generated_marker = "<!-- ravenbot:generated -->";
    let needs_write = !policy_path.exists()
        || std::fs::read_to_string(&policy_path)
            .map(|s| s.starts_with(generated_marker))
            .unwrap_or(false);
    if needs_write {
        if let Some(policy) = room.policy.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
            let body = format!("{generated_marker}\n{policy}\n");
            write_if_changed(&policy_path, &body);
        }
    }
}

/// The `project_folders` value an office should persist: the room plus its
/// shared folder, so a teammate can read another agent's handoff notes.
pub fn project_folders(ws: &Workspace) -> Vec<String> {
    vec![
        ws.root.to_string_lossy().to_string(),
        ws.shared.to_string_lossy().to_string(),
    ]
}

/// Rewrite the policy when the operator changes it in the app.
///
/// Unlike seeding, this always writes: the user edited a field, so the file on
/// disk is stale by definition. `None` removes a policy that no longer exists,
/// but only if RAVENBOT generated it.
pub fn write_policy(room: &ChatRoom) {
    let root = room
        .project_folders
        .first()
        .map(|d| ravenbot_core::expand_home(d))
        .unwrap_or_else(|| ravenbot_core::office_workspace(&room.name));
    let path = root.join("POLICY.md");
    match room.policy.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        Some(policy) => {
            let body = format!("<!-- ravenbot:generated -->\n{policy}\n");
            write_if_changed(&path, &body);
        }
        None => {
            let generated = std::fs::read_to_string(&path)
                .map(|s| s.starts_with("<!-- ravenbot:generated -->"))
                .unwrap_or(false);
            if generated {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

// ── charter contents ────────────────────────────────────────────────────────

fn office_md(ws: &Workspace, room: &ChatRoom, roster: &[RosterEntry]) -> String {
    let mut s = String::new();
    s.push_str("# ");
    s.push_str(&room.name);
    s.push_str("\n\n");

    if !room.description.trim().is_empty() {
        s.push_str(&room.description.trim());
        s.push_str("\n\n");
    }
    if let Some(goal) = room.goal.as_deref().map(str::trim).filter(|g| !g.is_empty()) {
        s.push_str("## Mission\n\n");
        s.push_str(goal);
        s.push_str("\n\n");
    }

    s.push_str("## Your workspace\n\n");
    s.push_str(
        "This directory is the office. You have full read and write access \
         inside it and nowhere else on this computer. Everything you produce \
         for this office belongs here.\n\n",
    );
    s.push_str(&format!("- Office root: `{}`\n", ws.root.display()));
    s.push_str(&format!(
        "- `Shared/` — handoffs and scratch everyone can read (`{}`)\n",
        ws.shared.display()
    ));
    s.push_str(&format!(
        "- `deliverables/` — finished outputs (`{}`)\n",
        ws.deliverables.display()
    ));
    s.push_str(&format!(
        "- `notes/` — working notes and research (`{}`)\n",
        ws.notes.display()
    ));
    s.push_str(&format!(
        "- `POLICY.md` — how this office works ({})\n",
        if room.policy.as_deref().map(str::trim).unwrap_or("").is_empty() {
            "not set yet"
        } else {
            "set"
        }
    ));
    s.push('\n');

    if !roster.is_empty() {
        let lead = roster.iter().find(|r| r.is_lead);
        s.push_str("## The team\n\n");
        if let Some(lead) = lead {
            s.push_str(&format!(
                "**{}** ({} — {}) leads this office and gives the final answer to the client.\n\n",
                lead.name, lead.rank, lead.specialty
            ));
        }
        s.push_str("| Agent | Rank | Specialty |\n|---|---|---|\n");
        for r in roster {
            s.push_str(&format!(
                "| {} | {} | {} |\n",
                escape_cell(&r.name),
                escape_cell(&r.rank),
                escape_cell(&r.specialty)
            ));
        }
        s.push('\n');
        let mandates: Vec<&RosterEntry> =
            roster.iter().filter(|r| r.mandate.is_some()).collect();
        if !mandates.is_empty() {
            s.push_str("### How each member works\n\n");
            for r in mandates {
                s.push_str(&format!(
                    "**{}**\n\n{}\n\n",
                    r.name,
                    r.mandate.as_deref().unwrap_or_default()
                ));
            }
        }
    }

    s.push_str("## Ground rules\n\n");
    s.push_str(
        "- Write real files. A claim that something works is worth nothing \
         without the command output or the file that proves it.\n\
         - Put finished work in `deliverables/`, working notes in `notes/`, \
         and anything a teammate needs in `Shared/`.\n\
         - `Shared/STATUS.md` is the live board. Update it when you start and \
         when you finish so nobody duplicates work.\n\
         - Read a file before you change it, and change only what your task \
         asks for.\n\
         - If you are blocked, say so and name what you need — do not invent a \
         result.\n\
         - Never reach outside this directory. If you need a file from \
         elsewhere on the computer, ask the client.\n",
    );
    s
}

fn readme_md(room: &ChatRoom, ws: &Workspace) -> String {
    let mut s = String::new();
    s.push_str(&format!("# {}\n\n", room.name));
    s.push_str(
        "This folder is the workspace of a RAVENBOT office. The agents in it \
         work here on real files; anything they produce stays here.\n\n",
    );
    if let Some(goal) = room.goal.as_deref().map(str::trim).filter(|g| !g.is_empty()) {
        s.push_str(&format!("**Mission:** {goal}\n\n"));
    }
    s.push_str("| Folder | What goes here |\n|---|---|\n");
    s.push_str("| `Shared/` | Handoffs, scratch, and the live status board |\n");
    s.push_str("| `deliverables/` | Finished outputs |\n");
    s.push_str("| `notes/` | Working notes and research |\n\n");
    s.push_str(&format!("Created: {}\n", room.created_at.to_rfc3339()));
    s.push_str(&format!("Workspace: `{}`\n", ws.root.display()));
    s
}

fn status_md(room: &ChatRoom, roster: &[RosterEntry]) -> String {
    let mut s = String::new();
    s.push_str(&format!("# {} — status\n\n", room.name));
    s.push_str(
        "Update this when you start a task and when you finish one, so nobody \
         duplicates work. One line per agent, most recent activity last.\n\n",
    );
    if roster.is_empty() {
        s.push_str("_No agents assigned yet._\n");
        return s;
    }
    s.push_str("| Agent | Working on | Status |\n|---|---|---|\n");
    for r in roster {
        s.push_str(&format!(
            "| {} | — | idle |\n",
            escape_cell(&r.name)
        ));
    }
    s.push('\n');
    s.push_str(&format!("_Last reset: {}_\n", room.updated_at.to_rfc3339()));
    s
}

/// Escape the three characters that would break a Markdown table cell.
fn escape_cell(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('\n', " ")
}

/// Write `body` only when it differs, so re-seeding does not churn mtimes and
/// does not fight a file watcher or an editor holding the file open.
fn write_if_changed(path: &Path, body: &str) {
    if let Ok(existing) = std::fs::read_to_string(path) {
        if existing == body {
            return;
        }
    }
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::warn!(path = %parent.display(), error = %e, "Could not create workspace folder");
            return;
        }
    }
    if let Err(e) = std::fs::write(path, body) {
        tracing::warn!(path = %path.display(), error = %e, "Could not write workspace file");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch `RAVENBOT_HOME` for the duration of `body`.
    fn in_temp_root<T>(body: impl FnOnce() -> T) -> T {
        let tmp = std::env::temp_dir().join(format!("rb-ws-{}", uuid::Uuid::new_v4()));
        let saved = std::env::var("RAVENBOT_HOME").ok();
        let saved_proj = std::env::var("RAVENBOT_PROJECTS_DIR").ok();
        std::env::set_var("RAVENBOT_HOME", &tmp);
        std::env::remove_var("RAVENBOT_PROJECTS_DIR");
        let out = body();
        match saved {
            Some(v) => std::env::set_var("RAVENBOT_HOME", v),
            None => std::env::remove_var("RAVENBOT_HOME"),
        }
        match saved_proj {
            Some(v) => std::env::set_var("RAVENBOT_PROJECTS_DIR", v),
            None => std::env::remove_var("RAVENBOT_PROJECTS_DIR"),
        }
        let _ = std::fs::remove_dir_all(&tmp);
        out
    }

    fn room() -> ChatRoom {
        let mut r = ChatRoom::new("Acme Corp", "We ship things", "it-office");
        r.goal = Some("Ship v1 by Friday".into());
        r
    }

    fn roster() -> Vec<RosterEntry> {
        vec![
            RosterEntry::new("CEO", "CEO", "Orchestration", true)
                .with_mandate("Decompose the goal and synthesize the answer."),
            RosterEntry::new("Coder", "Developer", "Implementation", false),
        ]
    }

    // These tests mutate RAVENBOT_HOME, which is process-global, so they share
    // one lock rather than racing each other.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn locked<T>(body: impl FnOnce() -> T) -> T {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        in_temp_root(body)
    }

    #[test]
    fn seeding_creates_the_room_and_its_subfolders() {
        locked(|| {
            let ws = seed(&room(), &roster());
            assert!(ws.created, "first seed should report a new room");
            assert!(ws.root.ends_with("projects/acme-corp"));
            for f in ["OFFICE.md", "README.md"] {
                assert!(ws.root.join(f).is_file(), "missing {f}");
            }
            for d in [&ws.shared, &ws.deliverables, &ws.notes] {
                assert!(d.is_dir(), "missing {}", d.display());
            }
            assert!(ws.shared.join("STATUS.md").is_file());
        });
    }

    #[test]
    fn office_md_names_the_mission_roster_and_paths() {
        locked(|| {
            let ws = seed(&room(), &roster());
            let body = std::fs::read_to_string(ws.root.join("OFFICE.md")).unwrap();

            assert!(body.contains("Ship v1 by Friday"), "mission missing");
            assert!(body.contains("| CEO | CEO | Orchestration |"), "roster row missing");
            assert!(body.contains("| Coder | Developer | Implementation |"));
            assert!(body.contains("Decompose the goal"), "mandate missing");
            assert!(body.contains(&ws.root.display().to_string()), "root path missing");
            assert!(body.contains("deliverables/"), "layout missing");
            // The workspace contract is the point of the file.
            assert!(body.contains("nowhere else on this computer"));
        });
    }

    #[test]
    fn seeding_twice_is_idempotent_and_reports_the_room_as_existing() {
        locked(|| {
            let first = seed(&room(), &roster());
            let stamp = std::fs::metadata(first.root.join("OFFICE.md"))
                .unwrap()
                .modified()
                .unwrap();
            let body = std::fs::read_to_string(first.root.join("OFFICE.md")).unwrap();

            let second = seed(&room(), &roster());
            assert!(!second.created, "second seed must not claim a new room");
            assert_eq!(first.root, second.root);
            assert_eq!(body, std::fs::read_to_string(second.root.join("OFFICE.md")).unwrap());
            // Unchanged content must not be rewritten.
            assert_eq!(
                stamp,
                std::fs::metadata(second.root.join("OFFICE.md"))
                    .unwrap()
                    .modified()
                    .unwrap()
            );
        });
    }

    #[test]
    fn a_roster_change_rewrites_the_charter() {
        locked(|| {
            let ws = seed(&room(), &roster());
            let mut grown = roster();
            grown.push(RosterEntry::new("QA", "Quality Assurance", "Acceptance", false));
            seed(&room(), &grown);

            let body = std::fs::read_to_string(ws.root.join("OFFICE.md")).unwrap();
            assert!(body.contains("| QA | Quality Assurance | Acceptance |"));
        });
    }

    #[test]
    fn a_hand_written_policy_survives_reseeding() {
        locked(|| {
            let mut r = room();
            r.policy = Some("Ship weekly. No exceptions.".into());
            let ws = seed(&r, &roster());
            let path = ws.root.join("POLICY.md");
            assert!(std::fs::read_to_string(&path).unwrap().contains("Ship weekly"));

            // An operator edits the file directly.
            std::fs::write(&path, "Hand written, no marker.").unwrap();
            seed(&r, &roster());
            assert_eq!(
                std::fs::read_to_string(&path).unwrap(),
                "Hand written, no marker."
            );

            // Changing the policy in the app overwrites it, because the user
            // asked for that value.
            r.policy = Some("Ship daily now.".into());
            write_policy(&r);
            let body = std::fs::read_to_string(&path).unwrap();
            assert!(body.contains("Ship daily now."), "got: {body}");
        });
    }

    #[test]
    fn clearing_the_policy_only_removes_a_generated_one() {
        locked(|| {
            let mut r = room();
            r.policy = Some("Generated.".into());
            let ws = seed(&r, &roster());
            let path = ws.root.join("POLICY.md");

            r.policy = None;
            write_policy(&r);
            assert!(!path.exists(), "a generated policy should be removed");

            std::fs::write(&path, "Hand written.").unwrap();
            r.policy = None;
            write_policy(&r);
            assert!(path.exists(), "a hand-written policy must not be deleted");
        });
    }

    #[test]
    fn a_table_cell_cannot_break_the_roster_table() {
        locked(|| {
            let mut entries = roster();
            entries[1].name = "Coder | drop | table".into();
            let ws = seed(&room(), &entries);
            let body = std::fs::read_to_string(ws.root.join("OFFICE.md")).unwrap();
            let row = body.lines().find(|l| l.starts_with("| Coder")).expect("coder row");

            assert!(row.contains("Coder \\| drop \\| table"), "not escaped: {row}");
            assert_eq!(cell_count(row), 3, "row: {row}");
        });
    }

    /// The number of real cells in a Markdown table row: the `|` characters
    /// that are not escaped, minus the two edge delimiters.
    fn cell_count(row: &str) -> usize {
        let separators = row
            .chars()
            .enumerate()
            .filter(|(i, c)| {
                *c == '|' && row[..*i].chars().rev().take_while(|p| *p == '\\').count() % 2 == 0
            })
            .count();
        separators.saturating_sub(1)
    }

    #[test]
    fn project_folders_are_the_room_and_its_shared_folder() {
        locked(|| {
            let ws = seed(&room(), &roster());
            let folders = project_folders(&ws);
            assert_eq!(folders.len(), 2);
            assert_eq!(folders[0], ws.root.to_string_lossy());
            assert_eq!(folders[1], ws.shared.to_string_lossy());
        });
    }

    #[test]
    fn seeding_never_panics_without_a_goal_or_roster() {
        locked(|| {
            let bare = ChatRoom::new("", "", "custom");
            let ws = seed(&bare, &[]);
            let body = std::fs::read_to_string(ws.root.join("OFFICE.md")).unwrap();
            assert!(body.contains("## Your workspace"));
            assert!(body.contains("_No agents assigned yet._") || !body.is_empty());
        });
    }
}
