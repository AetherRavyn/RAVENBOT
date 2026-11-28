//! API keys requested through the Vault.
//!
//! The metadata row is a normal `vault_items` row with `kind = 'api_key'` and a
//! unique `key_name`. The value lives in the same sealed `secret` column as a
//! login, and a second copy is written to an owner-only file under
//! `keys/api-keys` so an agent can use it without the value entering its
//! context. List results and answers never contain the value.

use std::path::Path;

use chrono::Utc;
use ravenbot_core::api_keys as files;
use ravenbot_core::{QuestionRequest, QuestionStatus};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::queries::{QuestionQueries, VaultQueries};

/// Serialize file + row changes inside one process.
///
/// Parallel tool calls can request the same name at once. Two `store` calls
/// that both see "no row" would otherwise race: one insert wins, the loser
/// removes the winner's file during rollback, and the remaining row points at
/// a missing key. The desktop runs one fleet owner, so an in-process lock is
/// the whole boundary.
static KEY_STORE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Metadata for one stored API key. The value is deliberately absent.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ApiKeyMeta {
    pub id: String,
    /// The machine-safe name used for the file and the tool calls.
    pub name: String,
    /// The display label shown in the Vault and on the request card.
    pub label: String,
    pub website: String,
    pub created_at: String,
    pub updated_at: String,
}

/// The result of storing a key, safe to return to the model.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StoredApiKey {
    pub name: String,
    pub label: String,
    pub path: String,
    pub replaced: bool,
    pub answered: bool,
}

/// Every stored API key, newest first. Never includes the value.
pub async fn list(pool: &SqlitePool) -> Result<Vec<ApiKeyMeta>, String> {
    type Row = (String, String, String, String, String, String);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, key_name, name, website, created_at, updated_at
         FROM vault_items
         WHERE kind = 'api_key' AND key_name IS NOT NULL
         ORDER BY updated_at DESC",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("Could not read API keys: {error}"))?;
    Ok(rows
        .into_iter()
        .map(
            |(id, name, label, website, created_at, updated_at)| ApiKeyMeta {
                id,
                name,
                label,
                website,
                created_at,
                updated_at,
            },
        )
        .collect())
}

/// Look up one API key by its machine-safe name.
pub async fn find(pool: &SqlitePool, name: &str) -> Result<Option<ApiKeyMeta>, String> {
    let name = files::validate_api_key_name(name)?;
    type Row = (String, String, String, String, String, String);
    let row: Option<Row> = sqlx::query_as(
        "SELECT id, key_name, name, website, created_at, updated_at
         FROM vault_items
         WHERE kind = 'api_key' AND key_name = ?",
    )
    .bind(name)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("Could not read the API key: {error}"))?;
    Ok(row.map(
        |(id, name, label, website, created_at, updated_at)| ApiKeyMeta {
            id,
            name,
            label,
            website,
            created_at,
            updated_at,
        },
    ))
}

/// Store or replace one key and write its owner-only file.
pub async fn store(
    pool: &SqlitePool,
    name: &str,
    label: &str,
    website: &str,
    value: &str,
) -> Result<StoredApiKey, String> {
    store_in(pool, &files::api_keys_dir(), name, label, website, value).await
}

/// Store or replace one key under an explicit key directory.
///
/// The file is written first, then the metadata row; if the row fails, the
/// previous file is restored (or the new file removed) so the two copies do
/// not disagree about which value is current.
pub async fn store_in(
    pool: &SqlitePool,
    keys_dir: &Path,
    name: &str,
    label: &str,
    website: &str,
    value: &str,
) -> Result<StoredApiKey, String> {
    let _guard = KEY_STORE_LOCK.lock().await;
    let name = files::validate_api_key_name(name)?.to_string();
    let value = files::validate_api_key_value(value)?.to_string();
    // Resolve the path before writing: a non-UTF-8 location cannot be handed
    // to an agent, and discovering that after the row and file exist would
    // leave a stored key with a failed request.
    let path = keys_dir.join(&name);
    let path_string = path
        .to_str()
        .ok_or_else(|| "The API-key path is not valid UTF-8.".to_string())?
        .to_string();
    let label = {
        let label = label.trim();
        if label.is_empty() {
            name.clone()
        } else if label.chars().count() > 120 {
            return Err("The API key label is longer than 120 characters.".to_string());
        } else {
            label.to_string()
        }
    };
    let website = website.trim();
    if website.chars().count() > 300 {
        return Err("The API key website is longer than 300 characters.".to_string());
    }
    let website = website.to_string();

    let existing: Option<(String,)> =
        sqlx::query_as("SELECT id FROM vault_items WHERE kind = 'api_key' AND key_name = ?")
            .bind(&name)
            .fetch_optional(pool)
            .await
            .map_err(|error| format!("Could not look up the API key: {error}"))?;
    let replaced = existing.is_some();
    let id = existing
        .as_ref()
        .map(|(id,)| id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let previous = if replaced {
        VaultQueries::get_secret(pool, &id)
            .await
            .map_err(|error| format!("Could not read the previous API key: {error}"))?
            .and_then(|raw| parse_key_secret(&raw))
    } else {
        None
    };

    files::write_api_key_in(keys_dir, &name, &value)?;
    let secret = serde_json::json!({ "key": value }).to_string();
    let stored = crate::secrets::set_vault_secret(&id, &secret).await;
    let secret_column = if stored {
        crate::secrets::KEYCHAIN_MARKER
    } else {
        secret.as_str()
    };
    let now = Utc::now().to_rfc3339();
    let result = sqlx::query(
        "INSERT INTO vault_items
           (id, kind, name, key_name, username, website, last4, secret, created_at, updated_at)
         VALUES (?, 'api_key', ?, ?, '', ?, NULL, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
           name = excluded.name,
           key_name = excluded.key_name,
           website = excluded.website,
           secret = excluded.secret,
           updated_at = excluded.updated_at",
    )
    .bind(&id)
    .bind(&label)
    .bind(&name)
    .bind(&website)
    .bind(secret_column)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await;

    if let Err(error) = result {
        // Put the file back in step with the row that still exists.
        match previous {
            Some(previous) => {
                let _ = files::write_api_key_in(keys_dir, &name, &previous);
            }
            None => {
                let _ = files::remove_api_key_file_in(keys_dir, &name);
            }
        }
        return Err(format!("Could not save the API key: {error}"));
    }

    Ok(StoredApiKey {
        name,
        label,
        path: path_string,
        replaced,
        answered: false,
    })
}

/// Remove one key from the Vault and its owner-only file.
pub async fn remove(pool: &SqlitePool, name: &str) -> Result<bool, String> {
    remove_in(pool, &files::api_keys_dir(), name).await
}

/// Remove one key under an explicit key directory.
pub async fn remove_in(pool: &SqlitePool, keys_dir: &Path, name: &str) -> Result<bool, String> {
    let _guard = KEY_STORE_LOCK.lock().await;
    let name = files::validate_api_key_name(name)?;
    if find(pool, name).await?.is_none() {
        return Ok(false);
    }
    // Delete the file first: if that fails, the row still describes a usable
    // key. A row without a file would look present but fail in the shell.
    files::remove_api_key_file_in(keys_dir, name)?;
    let result = sqlx::query("DELETE FROM vault_items WHERE kind = 'api_key' AND key_name = ?")
        .bind(name)
        .execute(pool)
        .await
        .map_err(|error| format!("Could not remove the API key: {error}"))?;
    Ok(result.rows_affected() > 0)
}

/// Delete a Vault item by id, including an API key's owner-only file.
///
/// The Vault panel does not know which rows have a second copy on disk. This
/// is the one deletion path that does: an `api_key` row goes through
/// [`remove`], every other kind through the ordinary Vault delete.
pub async fn remove_vault_item(pool: &SqlitePool, id: &str) -> Result<(), String> {
    remove_vault_item_in(pool, &files::api_keys_dir(), id).await
}

/// Delete a Vault item under an explicit key directory.
pub async fn remove_vault_item_in(
    pool: &SqlitePool,
    keys_dir: &Path,
    id: &str,
) -> Result<(), String> {
    let row: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT kind, key_name FROM vault_items WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(|error| format!("Could not read the Vault item: {error}"))?;
    if let Some((kind, Some(name))) = row {
        if kind == "api_key" {
            remove_in(pool, keys_dir, &name).await?;
            return Ok(());
        }
    }
    VaultQueries::delete(pool, id)
        .await
        .map_err(|error| format!("Could not delete the Vault item: {error}"))
}

/// Read a key value for a local caller that is not the model.
///
/// Used by tests and by future rotation UI. The tool path never calls this.
pub async fn secret_for(pool: &SqlitePool, name: &str) -> Result<Option<String>, String> {
    let Some(key) = find(pool, name).await? else {
        return Ok(None);
    };
    let raw = VaultQueries::get_secret(pool, &key.id)
        .await
        .map_err(|error| format!("Could not read the API key: {error}"))?;
    Ok(raw.and_then(|raw| parse_key_secret(&raw)))
}

/// Complete an API-key request with a value entered in the desktop card.
///
/// This is the only path that accepts a value from a person. It checks that the
/// request is still pending and is really an API-key request, stores the key,
/// and writes an answer containing only the name and path. The value is never
/// written to the question row or returned.
pub async fn answer_question_with_key(
    pool: &SqlitePool,
    question_id: Uuid,
    value: &str,
) -> Result<StoredApiKey, String> {
    answer_question_with_key_in(pool, &files::api_keys_dir(), question_id, value).await
}

/// Complete an API-key request under an explicit key directory.
pub async fn answer_question_with_key_in(
    pool: &SqlitePool,
    keys_dir: &Path,
    question_id: Uuid,
    value: &str,
) -> Result<StoredApiKey, String> {
    let question = QuestionQueries::get(pool, question_id)
        .await
        .map_err(|error| format!("Could not read the request: {error}"))?
        .ok_or_else(|| "That request no longer exists.".to_string())?;
    if question.status != QuestionStatus::Pending {
        return Err("That request is no longer pending.".to_string());
    }
    if !question.is_api_key_request() {
        return Err("That question is not an API-key request.".to_string());
    }
    let name = question
        .api_key_name()
        .ok_or_else(|| "The API-key request has no valid name.".to_string())?;
    let label = question
        .payload
        .as_ref()
        .and_then(|payload| payload.title.as_deref())
        .unwrap_or(name);
    let website = question
        .payload
        .as_ref()
        .and_then(|payload| payload.site.as_deref())
        .unwrap_or("");
    let mut stored = store_in(pool, keys_dir, name, label, website, value).await?;
    let answer = serde_json::json!({
        "action_id": "saved",
        "values": {
            "name": stored.name.clone(),
            "path": stored.path.clone(),
        }
    });
    let answered = QuestionQueries::answer(pool, question_id, &answer.to_string())
        .await
        .map_err(|error| {
            format!("The key was stored, but the answer could not be recorded: {error}")
        })?;
    stored.answered = answered;
    Ok(stored)
}

/// Whether a pending question must be answered on the desktop.
pub fn requires_desktop_answer(question: &QuestionRequest) -> bool {
    question.is_api_key_request()
}

fn parse_key_secret(raw: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(raw)
        .ok()?
        .get("key")?
        .as_str()
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ravenbot_core::{BlockField, BlockOption, InteractionBlock, QuestionKind};
    use sqlx::sqlite::SqlitePoolOptions;
    use std::sync::OnceLock;

    static SECRET_ROOT: OnceLock<std::path::PathBuf> = OnceLock::new();

    fn init_secret_root() -> std::path::PathBuf {
        SECRET_ROOT
            .get_or_init(|| {
                let root = std::env::temp_dir().join(format!("rb-api-secrets-{}", Uuid::new_v4()));
                crate::secrets::force_fallback_for_tests();
                crate::secrets::use_test_vault_root(root.clone());
                root
            })
            .clone()
    }

    async fn db() -> SqlitePool {
        init_secret_root();
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        crate::migrations::run(&pool).await.expect("migrations");
        pool
    }

    fn keys_dir(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("rb-api-{tag}-{}", Uuid::new_v4()))
    }

    async fn question(pool: &SqlitePool) -> QuestionRequest {
        let bot = ravenbot_core::Bot::new("Key holder", "assistant");
        crate::queries::BotQueries::insert(pool, &bot)
            .await
            .expect("bot");
        let thread = ravenbot_core::Thread::new(bot.id, "Keys");
        crate::queries::ThreadQueries::create(pool, &thread)
            .await
            .expect("thread");
        let card = InteractionBlock {
            item_kind: Some("api_key".into()),
            name: Some("openai".into()),
            title: Some("OpenAI API key".into()),
            placeholder: Some("sk-…".into()),
            site: Some("https://platform.openai.com".into()),
            ..Default::default()
        };
        let request = QuestionRequest::pending_block(
            bot.id,
            thread.id,
            Uuid::new_v4(),
            "OpenAI",
            "Enter an OpenAI key",
            QuestionKind::Vault,
            card,
        )
        .expect("question block");
        QuestionQueries::create(pool, &request)
            .await
            .expect("question");
        request
    }

    #[tokio::test]
    async fn a_key_round_trips_without_returning_the_value() {
        let pool = db().await;
        let dir = keys_dir("roundtrip");
        let stored = store_in(
            &pool,
            &dir,
            "openai",
            "OpenAI API key",
            "https://openai.com",
            "sk-secret",
        )
        .await
        .expect("store");
        assert_eq!(stored.name, "openai");
        assert_eq!(stored.label, "OpenAI API key");
        assert!(!stored.replaced);
        assert_eq!(
            std::fs::read_to_string(&stored.path).expect("file"),
            "sk-secret"
        );

        let listed = list(&pool).await.expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "openai");
        assert!(!serde_json::to_string(&listed)
            .unwrap()
            .contains("sk-secret"));

        assert_eq!(
            secret_for(&pool, "openai").await.expect("secret"),
            Some("sk-secret".to_string())
        );

        let replaced = store_in(&pool, &dir, "openai", "OpenAI API key", "", "sk-new")
            .await
            .expect("replace");
        assert!(replaced.replaced);
        assert_eq!(
            std::fs::read_to_string(&replaced.path).expect("file"),
            "sk-new"
        );
        assert_eq!(list(&pool).await.unwrap().len(), 1);

        assert!(remove_in(&pool, &dir, "openai").await.expect("remove"));
        assert!(list(&pool).await.unwrap().is_empty());
        assert!(!std::path::Path::new(&stored.path).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn an_api_key_question_stores_only_metadata_in_the_answer() {
        let pool = db().await;
        let request = question(&pool).await;
        let dir = keys_dir("answer");
        let stored = answer_question_with_key_in(&pool, &dir, request.id, "sk-answer-secret")
            .await
            .expect("answer");
        assert!(stored.answered);
        assert_eq!(stored.name, "openai");
        assert_eq!(
            secret_for(&pool, "openai").await.expect("secret"),
            Some("sk-answer-secret".to_string())
        );

        let answered = QuestionQueries::get(&pool, request.id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(answered.status, QuestionStatus::Answered);
        let answer = answered.answer.expect("answer");
        assert!(answer.contains("\"path\""));
        assert!(!answer.contains("sk-answer-secret"));
        assert!(!answer.contains("secret"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_closed_or_wrong_question_cannot_store_a_key() {
        let pool = db().await;
        let request = question(&pool).await;
        let dir = keys_dir("closed");
        assert!(QuestionQueries::answer(&pool, request.id, "{}")
            .await
            .expect("close"));
        let error = answer_question_with_key_in(&pool, &dir, request.id, "sk-secret")
            .await
            .expect_err("closed");
        assert!(error.contains("no longer pending"));
        assert!(list(&pool).await.unwrap().is_empty());

        // A non-api-key vault request must not be completed by this path.
        let login = InteractionBlock {
            item_kind: Some("login".into()),
            site: Some("https://github.com".into()),
            ..Default::default()
        };
        let request = QuestionRequest::pending_block(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            "Login",
            "Sign in",
            QuestionKind::Vault,
            login,
        )
        .expect("login");
        let error = answer_question_with_key_in(&pool, &dir, request.id, "sk-secret")
            .await
            .expect_err("not an api key");
        assert!(error.contains("no longer exists") || error.contains("not an API-key"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn invalid_names_and_values_are_refused_before_any_write() {
        let pool = db().await;
        let dir = keys_dir("invalid");
        for name in ["OpenAI", "../openai", "openai key", "con"] {
            let error = store_in(&pool, &dir, name, "Label", "", "sk-secret")
                .await
                .expect_err(name);
            assert!(!error.is_empty());
        }
        let error = store_in(&pool, &dir, "openai", "Label", "", "sk secret")
            .await
            .expect_err("whitespace");
        assert!(error.contains("whitespace"));
        assert!(list(&pool).await.unwrap().is_empty());
        assert!(!dir.exists());
    }

    #[tokio::test]
    async fn vault_panel_deletion_removes_the_api_key_file_too() {
        let pool = db().await;
        let dir = keys_dir("panel-delete");
        let stored = store_in(&pool, &dir, "openai", "OpenAI", "", "sk-panel-secret")
            .await
            .expect("store");
        let key = find(&pool, "openai").await.unwrap().expect("key");
        remove_vault_item_in(&pool, &dir, &key.id)
            .await
            .expect("delete");
        assert!(find(&pool, "openai").await.unwrap().is_none());
        assert!(!std::path::Path::new(&stored.path).exists());

        // A login has no file; the ordinary Vault delete still works.
        crate::queries::VaultQueries::upsert(
            &pool,
            "login-1",
            "login",
            "GitHub",
            "ada",
            "https://github.com",
            None,
            r#"{"password":"pw"}"#,
        )
        .await
        .expect("login");
        remove_vault_item_in(&pool, &dir, "login-1")
            .await
            .expect("delete login");
        assert!(crate::queries::VaultQueries::list(&pool)
            .await
            .unwrap()
            .is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn list_and_find_never_carry_the_secret() {
        let pool = db().await;
        let dir = keys_dir("metadata");
        store_in(&pool, &dir, "openai", "OpenAI", "", "sk-meta-secret")
            .await
            .expect("store");
        let listed = list(&pool).await.unwrap();
        let json = serde_json::to_string(&listed).unwrap();
        assert!(!json.contains("sk-meta-secret"));
        let found = find(&pool, "openai").await.unwrap().expect("found");
        let json = serde_json::to_string(&found).unwrap();
        assert!(!json.contains("sk-meta-secret"));
        assert!(!json.contains("\"secret\""));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn an_api_key_block_cannot_smuggle_unrelated_controls() {
        let invalid = InteractionBlock {
            item_kind: Some("api_key".into()),
            name: Some("openai".into()),
            options: vec![BlockOption {
                id: "yes".into(),
                label: "Yes".into(),
                meta: None,
                selected: false,
            }],
            fields: vec![BlockField {
                id: "value".into(),
                label: "Value".into(),
                kind: "password".into(),
                required: true,
                options: Vec::new(),
                placeholder: None,
                value: None,
            }],
            ..Default::default()
        };
        assert!(invalid
            .validate(QuestionKind::Vault)
            .unwrap_err()
            .contains("cannot carry"));
    }
}
