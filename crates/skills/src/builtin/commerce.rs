//! Create reviewable spend requests.
//!
//! This is deliberately the *only* commerce tool an agent gets. It writes a
//! pending row that a person approves or denies in Settings → Commerce. There
//! is no purchase tool, payment client, card field or network access here; the
//! human opens the merchant's own checkout page in a browser.

use async_trait::async_trait;
use chrono::Utc;
use ravenbot_core::commerce::{self, SpendPolicy, SpendRequestInput};
use ravenbot_core::Permission;
use ravenbot_db::queries::{AppSettingsQueries, NewSpendRequest, SpendRequestQueries};

use crate::traits::{Skill, SkillContext, SkillError, SkillResult, SkillRisk};

/// The one agent-facing commerce tool.
pub struct SpendRequestSkill;

impl SpendRequestSkill {
    pub fn new() -> Self {
        Self
    }
}

fn required_str<'a>(arguments: &'a serde_json::Value, key: &str) -> Result<&'a str, SkillError> {
    arguments
        .get(key)
        .and_then(|value| value.as_str())
        .ok_or_else(|| SkillError::InvalidArguments(format!("Missing or invalid `{key}`.")))
}

fn required_amount(arguments: &serde_json::Value) -> Result<u64, SkillError> {
    arguments
        .get("amount_minor")
        .and_then(|value| value.as_u64())
        .ok_or_else(|| {
            SkillError::InvalidArguments(
                "Missing or invalid `amount_minor`; use a whole number of minor units.".to_string(),
            )
        })
}

#[async_trait]
impl Skill for SpendRequestSkill {
    fn id(&self) -> &str {
        "spend_request"
    }

    fn name(&self) -> &str {
        "Spend Request"
    }

    fn description(&self) -> &str {
        "Create a pending spend request for owner approval. Never purchases."
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
                "title": {
                    "type": "string",
                    "description": "Short one-line description of the purchase (1–200 characters)"
                },
                "merchant": {
                    "type": "string",
                    "description": "Merchant display name (1–200 characters)"
                },
                "url": {
                    "type": "string",
                    "description": "HTTPS merchant checkout URL; its host must be on the owner's allowlist"
                },
                "amount_minor": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Amount in minor units (cents for USD/EUR), not a decimal"
                },
                "currency": {
                    "type": "string",
                    "description": "Three-letter currency code matching the workspace policy, such as USD"
                },
                "reason": {
                    "type": "string",
                    "description": "Why this purchase is needed (1–2,000 characters)"
                }
            },
            "required": ["title", "merchant", "url", "amount_minor", "currency", "reason"]
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
        let input = SpendRequestInput::validate(
            required_str(&arguments, "title")?,
            required_str(&arguments, "merchant")?,
            required_str(&arguments, "url")?,
            required_amount(&arguments)?,
            required_str(&arguments, "currency")?,
            required_str(&arguments, "reason")?,
        )
        .map_err(SkillError::InvalidArguments)?;

        let setting = |key: &'static str| {
            let pool = pool.clone();
            async move {
                AppSettingsQueries::get(&pool, key)
                    .await
                    .map(|value| value.unwrap_or_default().trim().to_string())
            }
        };

        let enabled = setting(commerce::ENABLED_SETTING_KEY)
            .await
            .map_err(|error| {
                SkillError::Execution(format!("Could not read the commerce setting: {error}"))
            })?;
        if enabled != "true" {
            return Err(SkillError::Execution(
                "Commerce is disabled. An owner must enable it in Settings → Commerce.".to_string(),
            ));
        }
        let currency = setting(commerce::CURRENCY_SETTING_KEY)
            .await
            .map_err(|error| {
                SkillError::Execution(format!("Could not read the commerce currency: {error}"))
            })?;
        let per_request = setting(commerce::PER_REQUEST_LIMIT_SETTING_KEY)
            .await
            .map_err(|error| {
                SkillError::Execution(format!("Could not read the commerce limit: {error}"))
            })?;
        let daily = setting(commerce::DAILY_LIMIT_SETTING_KEY)
            .await
            .map_err(|error| {
                SkillError::Execution(format!("Could not read the commerce limit: {error}"))
            })?;
        let allowlist = setting(commerce::ALLOWLIST_SETTING_KEY)
            .await
            .map_err(|error| {
                SkillError::Execution(format!(
                    "Could not read the commerce merchant allowlist: {error}"
                ))
            })?;
        let policy = SpendPolicy::from_stored(true, &currency, &per_request, &daily, &allowlist)
            .map_err(|error| {
                SkillError::Execution(format!(
                    "The saved commerce policy is invalid. Fix it in Settings → Commerce: {error}"
                ))
            })?;

        let today = Utc::now().date_naive().to_string();
        let spent_today = SpendRequestQueries::spent_on(pool, &today)
            .await
            .map_err(|error| {
                SkillError::Execution(format!("Could not read today's spend: {error}"))
            })?;
        policy
            .check(&input, spent_today)
            .map_err(SkillError::InvalidArguments)?;

        let thread_id = context.thread_id.to_string();
        let run_id = context.run_id.to_string();
        let bot_id = context.bot_id.to_string();
        let row = SpendRequestQueries::create(
            pool,
            &NewSpendRequest {
                bot_id: &bot_id,
                thread_id: Some(&thread_id),
                run_id: Some(&run_id),
                title: &input.title,
                merchant: &input.merchant,
                merchant_url: &input.url,
                amount_minor: input.amount_minor as i64,
                currency: &input.currency,
                reason: &input.reason,
            },
        )
        .await
        .map_err(|error| {
            SkillError::Execution(format!("Could not save the spend request: {error}"))
        })?;

        Ok(SkillResult::success(serde_json::json!({
            "request_id": row.id,
            "status": "pending",
            "approved": false,
            "purchased": false,
            "amount_minor": row.amount_minor,
            "currency": row.currency,
            "merchant_host": input.host(),
            "message": "Spend request saved. Nothing was purchased. The workspace owner must approve it in Settings → Commerce.",
        })))
    }
}

impl Default for SpendRequestSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ravenbot_db::queries::NewSpendRequest;
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

    async fn set_policy(pool: &sqlx::SqlitePool, per_request: &str, daily: &str, allowlist: &str) {
        AppSettingsQueries::set(pool, commerce::ENABLED_SETTING_KEY, "true")
            .await
            .expect("enabled");
        AppSettingsQueries::set(pool, commerce::CURRENCY_SETTING_KEY, "USD")
            .await
            .expect("currency");
        AppSettingsQueries::set(pool, commerce::PER_REQUEST_LIMIT_SETTING_KEY, per_request)
            .await
            .expect("per-request");
        AppSettingsQueries::set(pool, commerce::DAILY_LIMIT_SETTING_KEY, daily)
            .await
            .expect("daily");
        AppSettingsQueries::set(pool, commerce::ALLOWLIST_SETTING_KEY, allowlist)
            .await
            .expect("allowlist");
    }

    fn args() -> serde_json::Value {
        serde_json::json!({
            "title": "Replacement SSD",
            "merchant": "Example Store",
            "url": "https://shop.example.com/checkout/ssd",
            "amount_minor": 12500,
            "currency": "usd",
            "reason": "The test machine's drive failed."
        })
    }

    #[tokio::test]
    async fn creates_a_pending_request_and_purchases_nothing() {
        let pool = db().await;
        set_policy(&pool, "20000", "50000", "example.com").await;
        let context = ctx(&pool);
        let result = SpendRequestSkill::new()
            .execute(&context, args())
            .await
            .expect("execute");
        assert!(result.success);
        assert_eq!(
            result.output.get("status").and_then(|value| value.as_str()),
            Some("pending")
        );
        assert_eq!(
            result
                .output
                .get("approved")
                .and_then(|value| value.as_bool()),
            Some(false)
        );
        assert_eq!(
            result
                .output
                .get("purchased")
                .and_then(|value| value.as_bool()),
            Some(false)
        );
        assert_eq!(
            result
                .output
                .get("message")
                .and_then(|value| value.as_str()),
            Some(
                "Spend request saved. Nothing was purchased. The workspace owner must approve it in Settings → Commerce."
            )
        );

        let id = result
            .output
            .get("request_id")
            .and_then(|value| value.as_str())
            .expect("request id");
        let row = SpendRequestQueries::get(&pool, id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(row.status, "pending");
        assert!(row.decided_at.is_none());
        assert!(row.purchased_at.is_none());
        assert_eq!(row.amount_minor, 12_500);
        assert_eq!(row.currency, "USD");
    }

    #[tokio::test]
    async fn fails_closed_when_commerce_is_disabled() {
        let pool = db().await;
        let error = SpendRequestSkill::new()
            .execute(&ctx(&pool), args())
            .await
            .expect_err("disabled");
        assert!(error.to_string().contains("Commerce is disabled"));
        assert!(SpendRequestQueries::list(&pool, 10)
            .await
            .expect("list")
            .is_empty());
    }

    #[tokio::test]
    async fn rejects_requests_over_the_per_request_limit() {
        let pool = db().await;
        set_policy(&pool, "1000", "5000", "example.com").await;
        let error = SpendRequestSkill::new()
            .execute(&ctx(&pool), args())
            .await
            .expect_err("over limit");
        assert!(error.to_string().contains("per-request limit"));
        assert!(SpendRequestQueries::list(&pool, 10)
            .await
            .expect("list")
            .is_empty());
    }

    #[tokio::test]
    async fn rejects_a_host_outside_the_allowlist() {
        let pool = db().await;
        set_policy(&pool, "20000", "50000", "other.example").await;
        let error = SpendRequestSkill::new()
            .execute(&ctx(&pool), args())
            .await
            .expect_err("host");
        assert!(error.to_string().contains("not in the merchant allowlist"));
    }

    #[tokio::test]
    async fn counts_today_s_approved_spend_before_creating() {
        let pool = db().await;
        set_policy(&pool, "20000", "25000", "example.com").await;
        let context = ctx(&pool);
        let bot_id = context.bot_id.to_string();
        let created = SpendRequestQueries::create(
            &pool,
            &NewSpendRequest {
                bot_id: &bot_id,
                thread_id: None,
                run_id: None,
                title: "Earlier purchase",
                merchant: "Example Store",
                merchant_url: "https://shop.example.com/checkout/one",
                amount_minor: 20_000,
                currency: "USD",
                reason: "Already approved today.",
            },
        )
        .await
        .expect("create");
        SpendRequestQueries::approve(&pool, &created.id, "owner", 25_000)
            .await
            .expect("approve");

        let error = SpendRequestSkill::new()
            .execute(&context, args())
            .await
            .expect_err("daily limit");
        assert!(error.to_string().contains("daily limit"));
    }

    #[tokio::test]
    async fn rejects_a_non_integer_amount_or_a_plain_http_url() {
        let pool = db().await;
        set_policy(&pool, "20000", "50000", "example.com").await;
        let context = ctx(&pool);
        let mut float = args();
        float["amount_minor"] = serde_json::json!(12.5);
        assert!(SpendRequestSkill::new()
            .execute(&context, float)
            .await
            .is_err());

        let mut insecure = args();
        insecure["url"] = serde_json::json!("http://shop.example.com/checkout");
        assert!(SpendRequestSkill::new()
            .execute(&context, insecure)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn refuses_without_an_attached_database() {
        let context =
            SkillContext::with_default_tier(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let error = SpendRequestSkill::new()
            .execute(&context, args())
            .await
            .expect_err("no db");
        assert!(error.to_string().contains("database"));
    }

    #[test]
    fn has_no_purchase_counterpart_and_no_permissions() {
        let skill = SpendRequestSkill::new();
        assert_eq!(skill.id(), "spend_request");
        assert!(skill.required_permissions().is_empty());
        assert_eq!(skill.risk().as_str(), "low");
        assert!(skill.input_schema()["required"]
            .as_array()
            .expect("required")
            .contains(&serde_json::json!("amount_minor")));
    }
}
