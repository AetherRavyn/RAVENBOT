//! Create reviewable outbound email drafts.
//!
//! This is deliberately the *only* email tool an agent gets. It writes a row
//! the person can read and edit in Settings → Mail. There is no send tool, no
//! SMTP client, and no network access in this module; sending is an owner-only
//! desktop command that a human invokes after reviewing the draft.

use async_trait::async_trait;
use ravenbot_core::{email, Permission};

use crate::traits::{Skill, SkillContext, SkillError, SkillResult, SkillRisk};

/// The one agent-facing email tool. The id has no `send` counterpart.
pub struct EmailDraftSkill;

impl EmailDraftSkill {
    pub fn new() -> Self {
        Self
    }
}

fn required_arg<'a>(arguments: &'a serde_json::Value, key: &str) -> Result<&'a str, SkillError> {
    arguments
        .get(key)
        .and_then(|value| value.as_str())
        .ok_or_else(|| SkillError::InvalidArguments(format!("Missing or invalid `{key}`.")))
}

#[async_trait]
impl Skill for EmailDraftSkill {
    fn id(&self) -> &str {
        "email_draft"
    }

    fn name(&self) -> &str {
        "Email Draft"
    }

    fn description(&self) -> &str {
        "Create a reviewable email draft. Never sends."
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn required_permissions(&self) -> Vec<Permission> {
        Vec::new()
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "to": {
                    "type": "string",
                    "description": "Recipients, comma or semicolon separated (1–20 addresses)"
                },
                "cc": {
                    "type": "string",
                    "description": "Optional Cc recipients, comma or semicolon separated (0–20 addresses)"
                },
                "subject": {
                    "type": "string",
                    "description": "Plain-text subject, 1–200 characters"
                },
                "body": {
                    "type": "string",
                    "description": "Plain-text body, 1–50,000 characters. No attachments or HTML."
                }
            },
            "required": ["to", "subject", "body"]
        })
    }

    fn risk(&self) -> SkillRisk {
        SkillRisk::Low
    }

    async fn execute(
        &self,
        context: &SkillContext,
        arguments: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        let pool = context.db_pool()?;

        let to = email::parse_recipient_list(required_arg(&arguments, "to")?, "To", true)
            .map_err(SkillError::InvalidArguments)?;
        let cc = email::parse_recipient_list(
            arguments
                .get("cc")
                .and_then(|value| value.as_str())
                .unwrap_or(""),
            "Cc",
            false,
        )
        .map_err(SkillError::InvalidArguments)?;
        let subject = email::validate_subject(required_arg(&arguments, "subject")?)
            .map_err(SkillError::InvalidArguments)?;
        let body = email::validate_body(required_arg(&arguments, "body")?)
            .map_err(SkillError::InvalidArguments)?;

        // Parse the saved allowlist on every call. A value saved by an older
        // build that no longer parses fails closed instead of silently
        // allowing everyone.
        let allowlist =
            ravenbot_db::queries::AppSettingsQueries::get(pool, email::ALLOWLIST_SETTING_KEY)
                .await
                .map_err(|error| {
                    SkillError::Execution(format!("Could not read the email allowlist: {error}"))
                })?
                .unwrap_or_default();
        let allowlist = email::validate_allowlist(&allowlist).map_err(|error| {
            SkillError::Execution(format!(
                "The saved email allowlist is invalid. Fix it in Settings → Mail: {error}"
            ))
        })?;
        email::ensure_allowlisted(&to, &allowlist).map_err(SkillError::InvalidArguments)?;
        email::ensure_allowlisted(&cc, &allowlist).map_err(SkillError::InvalidArguments)?;

        let thread_id = context.thread_id.to_string();
        let run_id = context.run_id.to_string();
        let bot_id = context.bot_id.to_string();
        let draft = ravenbot_db::queries::EmailDraftQueries::create(
            pool,
            &ravenbot_db::queries::NewEmailDraft {
                bot_id: &bot_id,
                thread_id: Some(&thread_id),
                run_id: Some(&run_id),
                to_addresses: &to,
                cc_addresses: &cc,
                subject: &subject,
                body: &body,
            },
        )
        .await
        .map_err(|error| SkillError::Execution(format!("Could not save the draft: {error}")))?;

        Ok(SkillResult::success(serde_json::json!({
            "draft_id": draft.id,
            "status": "draft",
            "recipients": to,
            "cc": cc,
            "sent": false,
            "message": "Draft saved. Nothing was sent. Review it in Settings → Mail.",
        })))
    }
}

impl Default for EmailDraftSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ravenbot_db::queries::{AppSettingsQueries, EmailDraftQueries};
    use uuid::Uuid;

    async fn db() -> sqlx::SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        ravenbot_db::migrations::run(&pool)
            .await
            .expect("migrations");
        pool
    }

    fn ctx(pool: &sqlx::SqlitePool) -> SkillContext {
        SkillContext::with_default_tier(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4())
            .with_db(pool.clone())
    }

    fn args() -> serde_json::Value {
        serde_json::json!({
            "to": "Person@Example.com, second@example.com",
            "cc": "cc@example.com",
            "subject": "Quarterly numbers",
            "body": "Hi,\n\nHere is the summary.\tThanks."
        })
    }

    #[tokio::test]
    async fn creates_a_draft_with_the_run_context_and_sends_nothing() {
        let pool = db().await;
        let context = ctx(&pool);
        let result = EmailDraftSkill::new()
            .execute(&context, args())
            .await
            .expect("execute");
        assert!(result.success);
        assert_eq!(
            result.output.get("sent").and_then(|value| value.as_bool()),
            Some(false)
        );
        assert_eq!(
            result.output.get("status").and_then(|value| value.as_str()),
            Some("draft")
        );
        assert_eq!(
            result
                .output
                .get("message")
                .and_then(|value| value.as_str()),
            Some("Draft saved. Nothing was sent. Review it in Settings → Mail.")
        );

        let id = result
            .output
            .get("draft_id")
            .and_then(|value| value.as_str())
            .expect("draft id");
        let draft = EmailDraftQueries::get(&pool, id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(draft.status, "draft");
        assert_eq!(draft.bot_id, context.bot_id.to_string());
        assert_eq!(
            draft.thread_id.as_deref(),
            Some(context.thread_id.to_string().as_str())
        );
        assert_eq!(
            draft.run_id.as_deref(),
            Some(context.run_id.to_string().as_str())
        );
        assert_eq!(
            draft.to_addresses,
            vec!["Person@Example.com", "second@example.com"]
        );
        assert_eq!(draft.cc_addresses, vec!["cc@example.com"]);
        assert_eq!(draft.subject, "Quarterly numbers");
        assert!(draft.sent_at.is_none());
    }

    #[tokio::test]
    async fn refuses_without_an_attached_database() {
        let context =
            SkillContext::with_default_tier(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let error = EmailDraftSkill::new()
            .execute(&context, args())
            .await
            .expect_err("must fail without a database");
        let message = error.to_string();
        assert!(message.contains("RAVENBOT database"), "{message}");
        assert!(message.contains("did not attach"), "{message}");
    }

    #[tokio::test]
    async fn validates_recipients_subject_body_and_limits() {
        let pool = db().await;
        let context = ctx(&pool);
        let skill = EmailDraftSkill::new();

        let mut bad = args();
        bad["to"] = serde_json::json!("not-an-address");
        assert!(skill.execute(&context, bad).await.is_err());

        let mut bad = args();
        bad["to"] = serde_json::json!((0..21)
            .map(|index| format!("person{index}@example.com"))
            .collect::<Vec<_>>()
            .join(","));
        assert!(skill.execute(&context, bad).await.is_err());

        let mut bad = args();
        bad["subject"] = serde_json::json!("");
        assert!(skill.execute(&context, bad).await.is_err());

        let mut bad = args();
        bad["subject"] = serde_json::json!("s".repeat(201));
        assert!(skill.execute(&context, bad).await.is_err());

        let mut bad = args();
        bad["body"] = serde_json::json!("");
        assert!(skill.execute(&context, bad).await.is_err());

        let mut bad = args();
        bad["body"] = serde_json::json!("b".repeat(50_001));
        assert!(skill.execute(&context, bad).await.is_err());

        let mut bad = args();
        bad["body"] = serde_json::json!("bad\0body");
        assert!(skill.execute(&context, bad).await.is_err());

        // Newlines and tabs are allowed.
        let good = skill.execute(&context, args()).await.expect("execute");
        assert!(good.success);
        assert_eq!(
            EmailDraftQueries::list(&pool, 100)
                .await
                .expect("list")
                .len(),
            1,
            "only the valid draft is persisted"
        );
    }

    #[tokio::test]
    async fn allowlist_rejects_outsiders_and_accepts_domains_and_subdomains() {
        let pool = db().await;
        let context = ctx(&pool);
        let skill = EmailDraftSkill::new();
        AppSettingsQueries::set(
            &pool,
            email::ALLOWLIST_SETTING_KEY,
            "example.com, boss@other.test",
        )
        .await
        .expect("allowlist");

        let result = skill.execute(&context, args()).await.expect("execute");
        assert!(result.success);

        let mut blocked = args();
        blocked["to"] = serde_json::json!("person@notexample.com");
        let error = skill
            .execute(&context, blocked)
            .await
            .expect_err("must reject the outsider");
        assert!(
            error.to_string().contains("person@notexample.com"),
            "{error}"
        );
        assert!(error.to_string().contains("allowlist"), "{error}");

        let mut subdomain = args();
        subdomain["to"] = serde_json::json!("person@team.example.com");
        subdomain["cc"] = serde_json::json!("Boss@Other.Test");
        assert!(skill.execute(&context, subdomain).await.is_ok());
    }

    #[tokio::test]
    async fn never_contacts_smtp_even_when_one_is_configured() {
        let pool = db().await;
        let context = ctx(&pool);
        // Point a configured SMTP account at a listener. The tool has no
        // transport and must not open a connection.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let port = listener.local_addr().expect("addr").port();
        for (key, value) in [
            ("agent_email_host", "127.0.0.1".to_string()),
            ("agent_email_port", port.to_string()),
            ("agent_email_security", "none".to_string()),
            ("agent_email_from", "agent@example.test".to_string()),
        ] {
            AppSettingsQueries::set(&pool, key, &value)
                .await
                .expect("setting");
        }

        EmailDraftSkill::new()
            .execute(&context, args())
            .await
            .expect("execute");
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(250), listener.accept())
                .await
                .is_err(),
            "email_draft opened an SMTP connection"
        );
    }
}
