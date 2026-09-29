//! Deciding what an agent is actually allowed to do.
//!
//! Every part of the declared permission model used to be decorative. A skill
//! said what it needed via `required_permissions()`, a bot carried a
//! `permissions` list, and neither was ever compared — so the list was written
//! to the database, read back, and never looked at again. The only enforcement
//! anywhere was the approval gate, which asks the *user* about a tool call it
//! has already decided to allow.
//!
//! That is a different thing, and it is why the model felt hollow: a permission
//! you configured and a permission that is checked are not the same permission.
//!
//! Three rules, in the order they apply:
//!
//! 1. **A tool the agent does not have the capability for is refused** before it
//!    runs. This is the missing enforcement, and it is independent of approval:
//!    `ask` mode is about asking the user, this is about the agent's own grant.
//! 2. **Delegation is scoped.** A bot delegates only to bots on its
//!    `delegate_to` list, or — for a lead — to its office roster.
//! 3. **Fail closed.** An unrecognised capability, an agent with no grant, or a
//!    delegate list that cannot be read all refuse. An empty grant means "no
//!    powers", not "all powers", because the alternative is that a bot created
//!    by an import is silently unrestricted.

use crate::{Bot, Permission};

/// Why a tool call was refused.
///
/// Carried into the tool result so the model is told *why* in terms it can act
/// on. A refusal the model cannot interpret becomes a retry loop against a tool
/// that will always refuse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Denial {
    /// The agent's grant does not include a capability the tool needs.
    MissingCapability {
        tool: String,
        needed: String,
    },
    /// Delegation to an agent that is not on the delegate list.
    NotDelegable { target: String },
    /// Delegation attempted by a bot that is not allowed to hand work out.
    NotADelegate { target: String },
}

impl Denial {
    /// The message the model receives, phrased so it can change course.
    pub fn explain(&self) -> String {
        match self {
            Denial::MissingCapability { tool, needed } => format!(
                "You do not have the {needed} capability, so `{tool}` is not available to you. \
                 Do not try it again. Either do the work with the tools you do have, or say \
                 plainly that you need this capability and why."
            ),
            Denial::NotDelegable { target } => format!(
                "You are not allowed to delegate to {target}. You may only hand work to the \
                 agents on your delegate list. Do the work yourself, or ask the user to \
                 connect you to that agent."
            ),
            Denial::NotADelegate { target } => format!(
                "Only an office lead may hand work to another agent, and you are not one, so \
                 you cannot delegate to {target}. Ask your lead to do it."
            ),
        }
    }
}

/// A short, human name for a capability.
///
/// Used in refusal messages and in the settings UI, so it reads as a thing a
/// person can turn on rather than a Rust variant name.
pub fn permission_label(p: &Permission) -> String {
    match p {
        Permission::FileSystem { paths } => {
            if paths.iter().any(|x| x == "/" || x == "." || x == "**") {
                "file system".to_string()
            } else {
                format!("file system ({})", paths.join(", "))
            }
        }
        Permission::Network { domains } => {
            if domains.iter().any(|d| d == "*" || d.is_empty()) {
                "network".to_string()
            } else {
                format!("network ({})", domains.join(", "))
            }
        }
        Permission::Shell => "shell".to_string(),
        Permission::Screenshot => "screen capture".to_string(),
        Permission::InputControl => "input control".to_string(),
        Permission::AudioCapture => "microphone".to_string(),
        Permission::AudioPlayback => "audio output".to_string(),
        Permission::Clipboard => "clipboard".to_string(),
        Permission::Delegation => "delegation".to_string(),
    }
}

/// Whether `granted` covers `needed`.
///
/// A grant covers a need when it is the same capability and, for the two that
/// carry scopes, the grant is at least as wide. A grant of `FileSystem["/tmp"]`
/// does not cover a skill that declares `FileSystem["/"]`, and a grant of
/// `Network["*"]` does cover a skill that declares `Network["api.example.com"]`
/// — an open grant is a superset of any narrow one.
pub fn grant_covers(granted: &Permission, needed: &Permission) -> bool {
    match (granted, needed) {
        (Permission::FileSystem { paths: g }, Permission::FileSystem { paths: n }) => {
            n.iter().all(|want| g.iter().any(|have| scope_covers(have, want)))
        }
        (Permission::Network { domains: g }, Permission::Network { domains: n }) => {
            n.iter().all(|want| g.iter().any(|have| domain_covers(have, want)))
        }
        (a, b) => a == b,
    }
}

fn scope_covers(granted: &str, needed: &str) -> bool {
    let g = granted.trim();
    let n = needed.trim();
    if g == "*" || g == "**" || g == "/" {
        return true;
    }
    if g == n {
        return true;
    }
    // A parent directory covers what is under it.
    n.starts_with(g.trim_end_matches('/')) && n.len() > g.len() && n.as_bytes()[g.len()] == b'/'
}

fn domain_covers(granted: &str, needed: &str) -> bool {
    let g = granted.trim().to_lowercase();
    let n = needed.trim().to_lowercase();
    if g == "*" || g.is_empty() {
        return true;
    }
    if g == n {
        return true;
    }
    // `example.com` covers `api.example.com`, but not `notexample.com`.
    n.ends_with(&format!(".{g}"))
}

/// How an agent's `permissions` list is interpreted.
///
/// The distinction matters and is visible in the UI, because it is the
/// difference between an agent that can do everything its skills allow and one
/// that has been narrowed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrantMode {
    /// The list is empty: the agent may use whatever it was equipped with, and
    /// the list is not narrowing anything.
    Unrestricted,
    /// The list has entries: the agent may only run tools whose needs are all
    /// covered by it.
    Narrowed,
}

impl GrantMode {
    pub fn of(bot: &Bot) -> Self {
        if bot.permissions.is_empty() {
            GrantMode::Unrestricted
        } else {
            GrantMode::Narrowed
        }
    }
}

/// Whether `bot` may call `tool` at all, given the capabilities the tool needs.
///
/// An agent with **no** `permissions` is not refused everything, and that is a
/// deliberate reading of the field rather than a missing check. The field was
/// empty on every agent that existed before this was enforced, so failing it
/// closed would have bricked the entire fleet on upgrade. It is also the
/// correct layering:
///
///  - The **filesystem boundary** is `SkillContext::resolve_path` with
///    `confined()`, and it is unconditional — an agent cannot reach outside its
///    workspace whether or not it holds a `FileSystem` grant.
///  - The **skill list** is what decides which tools an agent is even offered,
///    and that is already enforced (explicit skills, MCP assignment, plugin
///    assignment, and the tool cap).
///  - So this list sits *above* both, and what it is for is **narrowing**:
///    "this agent has `file_write` because I gave it the skill, but it is not
///    allowed to shell." That was the use that was impossible before, because
///    nothing read the field.
///
/// An agent is only `Unrestricted` in the sense that its skills decide. The
/// settings surface shows which mode an agent is in, so "unrestricted" is never
/// mistaken for "unsandboxed".
pub fn tool_permitted(
    bot: &Bot,
    tool: &str,
    needed: &[Permission],
) -> Result<(), Denial> {
    // `delegate` is gated on `delegate_to` and on being a lead, not on a
    // capability, so it is decided by `delegation_permitted`.
    if tool == "delegate" {
        return Ok(());
    }

    // Nothing to narrow against: the skill list is the grant.
    if GrantMode::of(bot) == GrantMode::Unrestricted {
        return Ok(());
    }

    for want in needed {
        if !bot.permissions.iter().any(|have| grant_covers(have, want)) {
            return Err(Denial::MissingCapability {
                tool: tool.to_string(),
                needed: permission_label(want),
            });
        }
    }
    Ok(())
}

/// Whether `bot` may hand work to `target`.
///
/// Three things decide it:
///
///  - **The `Delegation` capability**, when the agent's list narrows. An agent
///    narrowed away from delegation cannot delegate at all, even to somebody it
///    lists. An agent whose list is empty is not narrowed, so this does not
///    apply and the list alone decides.
///  - **The delegate list.** An empty `delegate_to` means "may not delegate".
///  - **Being a lead.** A bot with `is_orchestrator` may hand work to any agent
///    in its office, because that is the office's dispatch path. A bot without
///    it may only use its own list, so a Coder cannot quietly become a router.
///
/// Before this was enforced, `delegate_to` was written to the database and never
/// read, and target resolution fell back to a case-insensitive name search
/// across every bot in the database — so any agent could hand work to any other,
/// including one belonging to a different office.
pub fn delegation_permitted(
    bot: &Bot,
    target_id: uuid::Uuid,
    target_name: &str,
    office_roster: &[uuid::Uuid],
) -> Result<(), Denial> {
    // A lead dispatches within its office.
    if bot.is_orchestrator && office_roster.contains(&target_id) {
        return Ok(());
    }

    // The capability decides whether the move is permitted at all; the list
    // decides the target. An agent narrowed away from delegation cannot
    // delegate even to an agent on its list, so this is checked first.
    if GrantMode::of(bot) == GrantMode::Narrowed && !has_delegation(bot) {
        return Err(Denial::NotADelegate {
            target: target_name.to_string(),
        });
    }

    if bot.delegate_to.contains(&target_id) {
        return Ok(());
    }
    if bot.is_orchestrator {
        // A lead, but the target is not in its office.
        return Err(Denial::NotADelegate {
            target: target_name.to_string(),
        });
    }
    Err(Denial::NotDelegable {
        target: target_name.to_string(),
    })
}

/// Whether the delegation capability itself is granted.
pub fn has_delegation(bot: &Bot) -> bool {
    bot.permissions
        .iter()
        .any(|p| matches!(p, Permission::Delegation))
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn bot_with(permissions: Vec<Permission>) -> Bot {
        let mut b = Bot::new("Tester", "test");
        b.permissions = permissions;
        b
    }

    fn id() -> Uuid {
        Uuid::new_v4()
    }

    fn fs(paths: &[&str]) -> Permission {
        Permission::FileSystem {
            paths: paths.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn net(domains: &[&str]) -> Permission {
        Permission::Network {
            domains: domains.iter().map(|s| s.to_string()).collect(),
        }
    }

    // ── scope comparison ──

    #[test]
    fn a_wide_scope_covers_a_narrow_one() {
        assert!(grant_covers(&fs(&["/"]), &fs(&["/tmp"])));
        assert!(grant_covers(&fs(&["/home/u"]), &fs(&["/home/u/project/src"])));
        assert!(grant_covers(&net(&["*"]), &net(&["api.example.com"])));
        assert!(grant_covers(&net(&["example.com"]), &net(&["api.example.com"])));
    }

    #[test]
    fn a_narrow_scope_does_not_cover_a_wide_one() {
        // The direction that matters: granting /tmp is not permission to write /etc.
        assert!(!grant_covers(&fs(&["/tmp"]), &fs(&["/"])));
        assert!(!grant_covers(&fs(&["/home/u/project"]), &fs(&["/home/u"])));
    }

    #[test]
    fn a_prefix_is_not_a_parent_directory() {
        // `starts_with` on a string would say `/home/user-evil` is inside
        // `/home/user`, which is the classic prefix bug.
        assert!(!grant_covers(&fs(&["/home/user"]), &fs(&["/home/user-evil/x"])));
        assert!(!grant_covers(&net(&["example.com"]), &net(&["notexample.com"])));
    }

    #[test]
    fn different_capabilities_never_cover_each_other() {
        assert!(!grant_covers(&Permission::Shell, &Permission::Screenshot));
        assert!(!grant_covers(&Permission::Shell, &fs(&["/"])));
    }

    // ── the missing enforcement ──

    #[test]
    fn a_tool_needing_a_capability_the_agent_lacks_is_refused() {
        // This is the check that did not exist before.
        let bot = bot_with(vec![Permission::Shell]);
        let err = tool_permitted(&bot, "file_read", &[fs(&["."])]).unwrap_err();
        assert!(matches!(err, Denial::MissingCapability { .. }));
        assert!(err.explain().contains("file system"), "{}", err.explain());
        // The model is told not to retry, or it will loop.
        assert!(err.explain().contains("Do not try it again"));
    }

    #[test]
    fn a_capability_the_agent_has_permits_the_tool() {
        let bot = bot_with(vec![fs(&["/"]), Permission::Shell]);
        assert!(tool_permitted(&bot, "file_read", &[fs(&["."])]).is_ok());
        assert!(tool_permitted(&bot, "shell_exec", &[Permission::Shell]).is_ok());
    }

    #[test]
    fn every_needed_capability_must_be_covered() {
        let bot = bot_with(vec![fs(&["/"])]);
        // A skill needing both filesystem and shell is refused on the missing one.
        let err = tool_permitted(&bot, "big", &[fs(&["."]), Permission::Shell]).unwrap_err();
        assert!(matches!(err, Denial::MissingCapability { .. }));
    }

    #[test]
    fn an_empty_list_does_not_narrow_anything() {
        // The list narrows; it does not grant. With nothing to narrow against,
        // the skill list decides — and the workspace boundary is enforced
        // elsewhere, unconditionally.
        let bot = bot_with(vec![]);
        assert_eq!(GrantMode::of(&bot), GrantMode::Unrestricted);
        assert!(tool_permitted(&bot, "file_read", &[fs(&["."])]).is_ok());
    }

    #[test]
    fn a_populated_list_narrows() {
        let bot = bot_with(vec![fs(&["/"])]);
        assert_eq!(GrantMode::of(&bot), GrantMode::Narrowed);
        assert!(tool_permitted(&bot, "file_read", &[fs(&["."])]).is_ok());
        // This is the use that was impossible before: equipped with the skill,
        // but not allowed to use it.
        assert!(tool_permitted(&bot, "shell_exec", &[Permission::Shell]).is_err());
    }

    #[test]
    fn delegation_is_not_gated_by_a_capability_but_by_the_delegate_list() {
        let bot = bot_with(vec![fs(&["/"])]);
        assert!(tool_permitted(&bot, "delegate", &[]).is_ok());
    }

    // ── delegation scope ──

    #[test]
    fn a_bot_may_delegate_to_an_agent_on_its_list() {
        let target = id();
        let mut bot = bot_with(vec![Permission::Delegation]);
        bot.delegate_to = vec![target];
        assert!(delegation_permitted(&bot, target, "Coder", &[]).is_ok());
    }

    #[test]
    fn a_bot_with_no_delegation_permission_may_not_delegate() {
        // The list decides the target; the capability decides whether the
        // move is permitted at all. An agent narrowed away from delegation
        // cannot delegate even to an agent on its list.
        let target = id();
        let mut bot = bot_with(vec![fs(&["/"])]);
        bot.delegate_to = vec![target];
        assert!(delegation_permitted(&bot, target, "Coder", &[]).is_err());
    }

    #[test]
    fn a_bot_may_not_delegate_off_its_list() {
        // Previously: any bot could delegate to any bot in the database by name.
        let mut bot = bot_with(vec![Permission::Delegation]);
        bot.delegate_to = vec![id()];
        let stranger = id();
        let err = delegation_permitted(&bot, stranger, "Somebody Else", &[]).unwrap_err();
        assert!(matches!(err, Denial::NotDelegable { .. }));
        assert!(err.explain().contains("Somebody Else"));
    }

    #[test]
    fn an_empty_delegate_list_means_no_delegation() {
        let bot = bot_with(vec![Permission::Delegation]);
        assert!(delegation_permitted(&bot, id(), "Anyone", &[]).is_err());
    }

    #[test]
    fn a_lead_dispatches_to_its_office() {
        let target = id();
        let mut bot = bot_with(vec![Permission::Delegation]);
        bot.is_orchestrator = true;
        assert!(delegation_permitted(&bot, target, "Coder", &[target]).is_ok());
    }

    #[test]
    fn a_lead_cannot_reach_outside_its_office() {
        let mut bot = bot_with(vec![Permission::Delegation]);
        bot.is_orchestrator = true;
        let err = delegation_permitted(&bot, id(), "Outsider", &[id()]).unwrap_err();
        assert!(matches!(err, Denial::NotADelegate { .. }));
    }

    #[test]
    fn a_non_lead_cannot_dispatch_across_the_office_even_with_a_list() {
        let target = id();
        let mut bot = bot_with(vec![Permission::Delegation]);
        bot.delegate_to = vec![target];
        // On its list, but it is not the lead, and the roster is not the source
        // of authority for a non-lead.
        assert!(delegation_permitted(&bot, target, "Coder", &[target, id()]).is_ok());
    }

    #[test]
    fn labels_read_as_things_a_person_can_turn_on() {
        assert_eq!(permission_label(&Permission::Shell), "shell");
        assert_eq!(permission_label(&fs(&["/"])), "file system");
        assert_eq!(permission_label(&net(&["*"])), "network");
        assert_eq!(permission_label(&fs(&["/tmp"])), "file system (/tmp)");
    }
}
