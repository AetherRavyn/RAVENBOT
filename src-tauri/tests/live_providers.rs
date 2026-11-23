//! Opt-in live provider checks.
//!
//! These tests are ignored by default: they need real credentials and, for the
//! attachment test, a real public file URL. Nothing here writes a chat message.
//! Run one with:
//!
//! ```sh
//! RAVENBOT_LIVE_TELEGRAM_TOKEN=... cargo test -p ravenbot --test live_providers \
//!   telegram_bot_token_is_valid -- --ignored --nocapture
//! ```
//!
//! Every test skips cleanly when its credential is absent, so the whole file
//! can be run with whichever providers are configured.

fn env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn skip(name: &str) {
    eprintln!("skipping: {name} is not set");
}

#[tokio::test]
#[ignore = "requires RAVENBOT_LIVE_TELEGRAM_TOKEN"]
async fn telegram_bot_token_is_valid() {
    let Some(token) = env("RAVENBOT_LIVE_TELEGRAM_TOKEN") else {
        return skip("RAVENBOT_LIVE_TELEGRAM_TOKEN");
    };
    let body: serde_json::Value =
        reqwest::get(format!("https://api.telegram.org/bot{token}/getMe"))
            .await
            .expect("telegram request")
            .json()
            .await
            .expect("telegram json");
    assert_eq!(body.get("ok").and_then(|value| value.as_bool()), Some(true));
}

#[tokio::test]
#[ignore = "requires RAVENBOT_LIVE_SLACK_BOT_TOKEN"]
async fn slack_bot_token_is_valid() {
    let Some(token) = env("RAVENBOT_LIVE_SLACK_BOT_TOKEN") else {
        return skip("RAVENBOT_LIVE_SLACK_BOT_TOKEN");
    };
    let body: serde_json::Value = reqwest::Client::new()
        .post("https://slack.com/api/auth.test")
        .bearer_auth(token)
        .send()
        .await
        .expect("slack request")
        .json()
        .await
        .expect("slack json");
    assert_eq!(body.get("ok").and_then(|value| value.as_bool()), Some(true));
}

#[tokio::test]
#[ignore = "requires RAVENBOT_LIVE_DISCORD_TOKEN"]
async fn discord_bot_token_is_valid() {
    let Some(token) = env("RAVENBOT_LIVE_DISCORD_TOKEN") else {
        return skip("RAVENBOT_LIVE_DISCORD_TOKEN");
    };
    let response = reqwest::Client::new()
        .get("https://discord.com/api/v10/users/@me")
        .header("Authorization", format!("Bot {token}"))
        .send()
        .await
        .expect("discord request");
    assert!(response.status().is_success(), "discord rejected the token");
}

#[tokio::test]
#[ignore = "requires RAVENBOT_LIVE_WHATSAPP_TOKEN and RAVENBOT_LIVE_WHATSAPP_PHONE_ID"]
async fn whatsapp_phone_number_is_valid() {
    let (Some(token), Some(phone_id)) = (
        env("RAVENBOT_LIVE_WHATSAPP_TOKEN"),
        env("RAVENBOT_LIVE_WHATSAPP_PHONE_ID"),
    ) else {
        return skip("RAVENBOT_LIVE_WHATSAPP_TOKEN / RAVENBOT_LIVE_WHATSAPP_PHONE_ID");
    };
    let response = reqwest::Client::new()
        .get(format!(
            "https://graph.facebook.com/v21.0/{phone_id}?fields=id,display_phone_number"
        ))
        .bearer_auth(token)
        .send()
        .await
        .expect("whatsapp request");
    assert!(
        response.status().is_success(),
        "whatsapp rejected the phone number/token"
    );
}

#[tokio::test]
#[ignore = "requires RAVENBOT_LIVE_TEAMS_APP_ID and RAVENBOT_LIVE_TEAMS_APP_PASSWORD"]
async fn teams_app_credentials_are_valid() {
    let (Some(app_id), Some(password)) = (
        env("RAVENBOT_LIVE_TEAMS_APP_ID"),
        env("RAVENBOT_LIVE_TEAMS_APP_PASSWORD"),
    ) else {
        return skip("RAVENBOT_LIVE_TEAMS_APP_ID / RAVENBOT_LIVE_TEAMS_APP_PASSWORD");
    };
    let response = reqwest::Client::new()
        .post("https://login.microsoftonline.com/botframework.com/oauth2/v2.0/token")
        .form(&[
            ("grant_type", "client_credentials"),
            ("client_id", app_id.as_str()),
            ("client_secret", password.as_str()),
            ("scope", "https://api.botframework.com/.default"),
        ])
        .send()
        .await
        .expect("teams token request");
    let body: serde_json::Value = response.json().await.expect("teams json");
    assert!(
        body.get("access_token")
            .and_then(|value| value.as_str())
            .is_some(),
        "teams rejected the app credentials"
    );
}

#[tokio::test]
#[ignore = "requires RAVENBOT_LIVE_ATTACHMENT_URL"]
async fn a_live_attachment_url_is_downloadable() {
    let Some(url) = env("RAVENBOT_LIVE_ATTACHMENT_URL") else {
        return skip("RAVENBOT_LIVE_ATTACHMENT_URL");
    };
    let attachment = ravenbot_lib::chat_attachments::download(
        &reqwest::Client::new(),
        &url,
        None,
        "live-check",
        None,
        None,
    )
    .await
    .expect("attachment download");
    assert!(std::path::Path::new(&attachment.path).is_file());
    assert!(attachment.size > 0);
    let _ = std::fs::remove_file(&attachment.path);
}
