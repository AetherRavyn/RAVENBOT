//! Team packages — install a whole bot team from one Markdown file.
//!
//! Format: optional YAML frontmatter between `---` fences followed by an
//! ordinary Markdown playbook (kept for humans). The frontmatter is the
//! machine-readable contract:
//!
//! ```markdown
//! ---
//! name: Acme Growth Team
//! description: A small team for demand generation.
//! bots:
//!   - name: Chief of Staff
//!     title: Chief of Staff
//!     description: Coordinates the team.
//!     prompt: |
//!       You are the chief of staff. Route work and report clearly.
//!     rank: lead
//! office:
//!   name: Growth Office
//!   template: marketing
//!   goal: Ship the Q3 launch.
//! routines:
//!   - name: Morning briefing
//!     bot: Chief of Staff
//!     schedule: "0 9 * * 1-5"
//!     instruction: Summarize yesterday and plan today.
//! ---
//! # Playbook
//! ...human-readable instructions...
//! ```
//!
//! Nothing carries credentials, conversations, permissions, or memory — an
//! import only creates bots, an optional office, and paused routines.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TeamParseError {
    #[error("no YAML frontmatter found — a team file must start with `---`")]
    MissingFrontmatter,
    #[error("invalid frontmatter YAML: {0}")]
    Yaml(String),
    #[error("team must define at least one bot")]
    NoBots,
    #[error("bot #{0} is missing a name")]
    BotMissingName(usize),
    #[error("routine #{0} references unknown bot '{1}'")]
    RoutineUnknownBot(usize, String),
}

/// One bot in a team package.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TeamBot {
    pub name: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
    /// Rank within the office ("lead", "specialist", …)
    #[serde(default)]
    pub rank: Option<String>,
    #[serde(default)]
    pub specialty: Option<String>,
    /// Optional model binding, e.g. "anthropic/claude-3.5-sonnet" or "ollama".
    #[serde(default)]
    pub model: Option<String>,
    /// Avatar style for the generated avatar.
    #[serde(default)]
    pub avatar_style: Option<String>,
}

/// Optional office (chatroom) the team belongs to.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TeamOffice {
    pub name: String,
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub goal: Option<String>,
    #[serde(default)]
    pub policy: Option<String>,
}

/// A routine to create for the team (created paused, never auto-running).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TeamRoutine {
    pub name: String,
    /// Name of the bot (as listed in `bots`) that owns this routine.
    pub bot: String,
    /// Cron expression. If omitted, the routine is created disabled.
    #[serde(default)]
    pub schedule: Option<String>,
    #[serde(default)]
    pub instruction: String,
}

/// The parsed, validated team package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamPackage {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub bots: Vec<TeamBot>,
    #[serde(default)]
    pub office: Option<TeamOffice>,
    #[serde(default)]
    pub routines: Vec<TeamRoutine>,
    /// The human-readable Markdown body after the frontmatter.
    #[serde(default)]
    pub playbook: String,
    /// Whether the file used YAML frontmatter (vs bare JSON).
    pub had_frontmatter: bool,
}

impl TeamPackage {
    /// Parse a team Markdown file. Accepts YAML frontmatter (preferred) or a
    /// bare YAML/JSON document with the same fields.
    pub fn parse(input: &str) -> Result<Self, TeamParseError> {
        let trimmed = input.trim_start();

        let (front, body, had_frontmatter) = if let Some(rest) = trimmed.strip_prefix("---") {
            // Find the closing fence on its own line.
            let rest = rest.strip_prefix('\n').unwrap_or(rest);
            let close = rest
                .find("\n---")
                .ok_or(TeamParseError::MissingFrontmatter)?;
            let front = &rest[..close];
            let after = &rest[close + 4..];
            let body = after.strip_prefix('\n').unwrap_or(after).to_string();
            (front.to_string(), body, true)
        } else {
            // No frontmatter: try the whole document as YAML (covers bare JSON too).
            (trimmed.to_string(), String::new(), false)
        };

        let raw: RawTeam =
            serde_yaml::from_str(&front).map_err(|e| TeamParseError::Yaml(e.to_string()))?;

        if raw.bots.is_empty() {
            return Err(TeamParseError::NoBots);
        }
        for (i, bot) in raw.bots.iter().enumerate() {
            if bot.name.trim().is_empty() {
                return Err(TeamParseError::BotMissingName(i + 1));
            }
        }
        let names: Vec<String> = raw.bots.iter().map(|b| b.name.to_lowercase()).collect();
        for (i, routine) in raw.routines.iter().enumerate() {
            if !names.contains(&routine.bot.to_lowercase()) {
                return Err(TeamParseError::RoutineUnknownBot(i + 1, routine.bot.clone()));
            }
        }

        Ok(TeamPackage {
            name: if raw.name.trim().is_empty() {
                "Imported Team".to_string()
            } else {
                raw.name
            },
            description: raw.description,
            bots: raw.bots,
            office: raw.office,
            routines: raw.routines,
            playbook: body,
            had_frontmatter,
        })
    }

    /// A short human summary for the review screen.
    pub fn summary(&self) -> String {
        let mut parts = vec![format!("{} bot(s)", self.bots.len())];
        if self.office.is_some() {
            parts.push("1 office".to_string());
        }
        if !self.routines.is_empty() {
            parts.push(format!("{} routine(s) (paused)", self.routines.len()));
        }
        format!("{} — {}", self.name, parts.join(", "))
    }
}

/// Raw deserialization target (fields optional so a partial file still parses).
#[derive(Debug, Deserialize)]
struct RawTeam {
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    bots: Vec<TeamBot>,
    #[serde(default)]
    office: Option<TeamOffice>,
    #[serde(default)]
    routines: Vec<TeamRoutine>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"---
name: Acme Growth Team
description: A small demand-gen team.
bots:
  - name: Chief of Staff
    title: Chief of Staff
    description: Coordinates the team.
    prompt: |
      You are the chief of staff.
    rank: lead
  - name: Growth Marketer
    title: Growth Marketer
    description: Runs campaigns.
    model: openrouter/anthropic/claude-3.5-sonnet
office:
  name: Growth Office
  template: marketing
  goal: Ship the Q3 launch.
routines:
  - name: Morning briefing
    bot: Chief of Staff
    schedule: "0 9 * * 1-5"
    instruction: Summarize yesterday.
---
# Playbook

Do the work.
"#;

    #[test]
    fn parses_frontmatter_bots_office_and_routines() {
        let team = TeamPackage::parse(SAMPLE).expect("parse");
        assert!(team.had_frontmatter);
        assert_eq!(team.name, "Acme Growth Team");
        assert_eq!(team.bots.len(), 2);
        assert_eq!(team.bots[0].rank.as_deref(), Some("lead"));
        assert!(team.bots[0].prompt.as_deref().unwrap().contains("chief of staff"));
        assert_eq!(team.office.as_ref().unwrap().template.as_deref(), Some("marketing"));
        assert_eq!(team.routines.len(), 1);
        assert!(team.playbook.contains("Do the work."));
    }

    #[test]
    fn rejects_missing_frontmatter() {
        let err = TeamPackage::parse("just some text").unwrap_err();
        // A bare doc is attempted as YAML and fails, or is rejected explicitly.
        assert!(matches!(
            err,
            TeamParseError::Yaml(_) | TeamParseError::MissingFrontmatter
        ));
    }

    #[test]
    fn rejects_team_with_no_bots() {
        let err = TeamPackage::parse("---\nname: Empty\n---\n").unwrap_err();
        assert!(matches!(err, TeamParseError::NoBots));
    }

    #[test]
    fn rejects_routine_for_unknown_bot() {
        let md = r#"---
bots:
  - name: Alice
routines:
  - name: x
    bot: Bob
    instruction: hi
---
"#;
        let err = TeamPackage::parse(md).unwrap_err();
        assert!(matches!(err, TeamParseError::RoutineUnknownBot(_, _)));
    }

    #[test]
    fn accepts_bare_yaml_without_fences() {
        let md = "name: Solo\nbots:\n  - name: Helper\n";
        let team = TeamPackage::parse(md).expect("parse bare yaml");
        assert!(!team.had_frontmatter);
        assert_eq!(team.bots.len(), 1);
    }

    #[test]
    fn summary_counts_components() {
        let team = TeamPackage::parse(SAMPLE).unwrap();
        let s = team.summary();
        assert!(s.contains("2 bot(s)"));
        assert!(s.contains("1 office"));
        assert!(s.contains("1 routine(s)"));
    }
}
