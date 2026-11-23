//! OS-keychain storage for provider credentials.
//!
//! The database keeps a *marker* for a provider whose key lives in the OS
//! keychain. A key that could not be written there stays in the database as a
//! fallback, so a headless Linux box without a Secret Service is still usable
//! and the caller can tell which backend it got from the stored value.

use keyring::Entry;

pub const KEYCHAIN_MARKER: &str = "__RAVENBOT_KEYCHAIN__";
const SERVICE: &str = "com.ravenbot.app";

fn disabled() -> bool {
    std::env::var("RAVENBOT_DISABLE_KEYCHAIN")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn namespace() -> String {
    std::env::var("RAVENBOT_KEYCHAIN_NAMESPACE")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_default()
}

fn namespaced(account: &str, namespace: &str) -> String {
    if namespace.is_empty() {
        account.to_string()
    } else {
        format!("{namespace}:{account}")
    }
}

fn entry(account: &str) -> Result<Entry, keyring::Error> {
    Entry::new(SERVICE, account)
}

async fn set_secret(account: String, value: String) -> bool {
    if disabled() {
        return false;
    }
    let account = namespaced(&account, &namespace());
    tokio::task::spawn_blocking(move || match entry(&account) {
        Ok(entry) => entry.set_password(&value).is_ok(),
        Err(_) => false,
    })
    .await
    .unwrap_or(false)
}

async fn get_secret(account: String) -> Option<String> {
    if disabled() {
        return None;
    }
    let account = namespaced(&account, &namespace());
    tokio::task::spawn_blocking(move || match entry(&account) {
        Ok(entry) => entry.get_password().ok(),
        Err(_) => None,
    })
    .await
    .ok()
    .flatten()
}

async fn delete_secret(account: String) {
    if disabled() {
        return;
    }
    let account = namespaced(&account, &namespace());
    let _ = tokio::task::spawn_blocking(move || {
        if let Ok(entry) = entry(&account) {
            let _ = entry.delete_credential();
        }
    })
    .await;
}

/// Store a provider key in the OS keychain. Returns false when the platform
/// store is unavailable; the caller then persists the value in SQLite.
pub async fn set_provider_key(provider: &str, value: &str) -> bool {
    set_secret(format!("provider:{provider}"), value.to_string()).await
}

/// MCP credentials are per server and per key; the account name keeps both.
pub async fn set_mcp_env(server_id: &str, key: &str, value: &str) -> bool {
    set_secret(format!("mcp:{server_id}:{key}"), value.to_string()).await
}

pub async fn get_mcp_env(server_id: &str, key: &str) -> Option<String> {
    get_secret(format!("mcp:{server_id}:{key}")).await
}

pub async fn delete_mcp_env(server_id: &str, key: &str) {
    delete_secret(format!("mcp:{server_id}:{key}")).await;
}

pub async fn set_plugin_credential(plugin_id: &str, value: &str) -> bool {
    set_secret(format!("plugin:{plugin_id}"), value.to_string()).await
}

pub async fn get_plugin_credential(plugin_id: &str) -> Option<String> {
    get_secret(format!("plugin:{plugin_id}")).await
}

pub async fn delete_plugin_credential(plugin_id: &str) {
    delete_secret(format!("plugin:{plugin_id}")).await;
}

pub async fn set_memory_embedding_key(value: &str) -> bool {
    set_secret("memory-embedding-key".to_string(), value.to_string()).await
}

pub async fn get_memory_embedding_key() -> Option<String> {
    get_secret("memory-embedding-key".to_string()).await
}

pub async fn delete_memory_embedding_key() {
    delete_secret("memory-embedding-key".to_string()).await;
}

pub async fn set_attention_email_password(value: &str) -> bool {
    set_secret("attention-email-password".to_string(), value.to_string()).await
}

pub async fn get_attention_email_password() -> Option<String> {
    get_secret("attention-email-password".to_string()).await
}

pub async fn delete_attention_email_password() {
    delete_secret("attention-email-password".to_string()).await;
}

pub async fn set_attention_webhook(value: &str) -> bool {
    set_secret("attention-webhook".to_string(), value.to_string()).await
}

pub async fn get_attention_webhook() -> Option<String> {
    get_secret("attention-webhook".to_string()).await
}

pub async fn delete_attention_webhook() {
    delete_secret("attention-webhook".to_string()).await;
}

pub async fn set_vault_secret(item_id: &str, value: &str) -> bool {
    set_secret(format!("vault:{item_id}"), value.to_string()).await
}

pub async fn get_vault_secret(item_id: &str) -> Option<String> {
    get_secret(format!("vault:{item_id}")).await
}

pub async fn delete_vault_secret(item_id: &str) {
    delete_secret(format!("vault:{item_id}")).await;
}

pub async fn set_webhook_secret(routine_id: &str, value: &str) -> bool {
    set_secret(format!("webhook:{routine_id}"), value.to_string()).await
}

pub async fn get_webhook_secret(routine_id: &str) -> Option<String> {
    get_secret(format!("webhook:{routine_id}")).await
}

pub async fn delete_webhook_secret(routine_id: &str) {
    delete_secret(format!("webhook:{routine_id}")).await;
}

pub async fn set_mcp_headers(server_id: &str, value: &str) -> bool {
    set_secret(format!("mcp-server:{server_id}:headers"), value.to_string()).await
}

pub async fn get_mcp_headers(server_id: &str) -> Option<String> {
    get_secret(format!("mcp-server:{server_id}:headers")).await
}

pub async fn delete_mcp_headers(server_id: &str) {
    delete_secret(format!("mcp-server:{server_id}:headers")).await;
}

pub async fn set_custom_provider_headers(provider_id: &str, value: &str) -> bool {
    set_secret(
        format!("custom-provider:{provider_id}:headers"),
        value.to_string(),
    )
    .await
}

pub async fn get_custom_provider_headers(provider_id: &str) -> Option<String> {
    get_secret(format!("custom-provider:{provider_id}:headers")).await
}

pub async fn delete_custom_provider_headers(provider_id: &str) {
    delete_secret(format!("custom-provider:{provider_id}:headers")).await;
}

pub async fn set_signing_seed(value: &str) -> bool {
    set_secret("bundle:signing-seed".to_string(), value.to_string()).await
}

pub async fn get_signing_seed() -> Option<String> {
    get_secret("bundle:signing-seed".to_string()).await
}

/// Read a provider key from the OS keychain.
pub async fn get_provider_key(provider: &str) -> Option<String> {
    get_secret(format!("provider:{provider}")).await
}

/// Delete a provider key from the OS keychain. A missing entry is success.
pub async fn delete_provider_key(provider: &str) {
    delete_secret(format!("provider:{provider}")).await;
}

pub async fn set_telegram_token(value: &str) -> bool {
    set_secret("telegram:bot".to_string(), value.to_string()).await
}

pub async fn get_telegram_token() -> Option<String> {
    get_secret("telegram:bot".to_string()).await
}

pub async fn delete_telegram_token() {
    delete_secret("telegram:bot".to_string()).await;
}

pub async fn set_slack_app_token(value: &str) -> bool {
    set_secret("slack:app".to_string(), value.to_string()).await
}

pub async fn get_slack_app_token() -> Option<String> {
    get_secret("slack:app".to_string()).await
}

pub async fn delete_slack_app_token() {
    delete_secret("slack:app".to_string()).await;
}

pub async fn set_slack_bot_token(value: &str) -> bool {
    set_secret("slack:bot".to_string(), value.to_string()).await
}

pub async fn get_slack_bot_token() -> Option<String> {
    get_secret("slack:bot".to_string()).await
}

pub async fn delete_slack_bot_token() {
    delete_secret("slack:bot".to_string()).await;
}

pub async fn set_discord_token(value: &str) -> bool {
    set_secret("discord:bot".to_string(), value.to_string()).await
}

pub async fn get_discord_token() -> Option<String> {
    get_secret("discord:bot".to_string()).await
}

pub async fn delete_discord_token() {
    delete_secret("discord:bot".to_string()).await;
}

pub async fn set_whatsapp_token(value: &str) -> bool {
    set_secret("whatsapp:token".to_string(), value.to_string()).await
}

pub async fn get_whatsapp_token() -> Option<String> {
    get_secret("whatsapp:token".to_string()).await
}

pub async fn delete_whatsapp_token() {
    delete_secret("whatsapp:token".to_string()).await;
}

pub async fn set_whatsapp_app_secret(value: &str) -> bool {
    set_secret("whatsapp:app_secret".to_string(), value.to_string()).await
}

pub async fn get_whatsapp_app_secret() -> Option<String> {
    get_secret("whatsapp:app_secret".to_string()).await
}

pub async fn delete_whatsapp_app_secret() {
    delete_secret("whatsapp:app_secret".to_string()).await;
}

pub async fn set_whatsapp_verify_token(value: &str) -> bool {
    set_secret("whatsapp:verify_token".to_string(), value.to_string()).await
}

pub async fn get_whatsapp_verify_token() -> Option<String> {
    get_secret("whatsapp:verify_token".to_string()).await
}

pub async fn delete_whatsapp_verify_token() {
    delete_secret("whatsapp:verify_token".to_string()).await;
}

pub async fn set_teams_app_password(value: &str) -> bool {
    set_secret("teams:app_password".to_string(), value.to_string()).await
}

pub async fn get_teams_app_password() -> Option<String> {
    get_secret("teams:app_password".to_string()).await
}

pub async fn delete_teams_app_password() {
    delete_secret("teams:app_password".to_string()).await;
}

#[cfg(test)]
mod account_namespace_tests {
    use super::namespaced;

    #[test]
    fn owner_keys_keep_the_legacy_name_and_accounts_are_prefixed() {
        assert_eq!(namespaced("provider:openai", ""), "provider:openai");
        assert_eq!(
            namespaced("provider:openai", "account-1"),
            "account-1:provider:openai"
        );
    }
}
