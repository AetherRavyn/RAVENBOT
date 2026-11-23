//! Environment handoff between the account launcher and the account process.
//!
//! This lives in an integration-test binary so it owns its process environment
//! and cannot race the library's other tests.

use std::path::PathBuf;

use ravenbot_lib::accounts;

fn set_env(key: &str, value: &str) {
    std::env::set_var(key, value);
}

fn clear_env() {
    for key in [
        "RAVENBOT_HOME",
        "RAVENBOT_BASE_HOME",
        "RAVENBOT_DB",
        "RAVENBOT_ACCOUNT_ID",
        "RAVENBOT_ACCOUNT_SESSION",
        "RAVENBOT_KEYCHAIN_NAMESPACE",
        "RAVENBOT_WORKSPACE_ID",
        "RAVENBOT_WEBHOOK_PORT",
        "RAVENBOT_CHAT_WEBHOOK_PORT",
        "RAVENBOT_REMOTE_PORT",
        "RAVENBOT_SERVICE_NAME",
        "RAVENBOT_SERVICE_LABEL",
    ] {
        std::env::remove_var(key);
    }
}

#[tokio::test]
async fn the_owner_and_an_isolated_account_get_distinct_roots() {
    let base = std::env::temp_dir().join(format!("ravenbot-accounts-env-{}", uuid::Uuid::new_v4()));
    clear_env();
    set_env("RAVENBOT_BASE_HOME", &base.to_string_lossy());
    assert!(matches!(
        accounts::detect_startup().await.expect("launcher mode"),
        accounts::Startup::Launcher
    ));

    let owner = accounts::create_account(&base, "owner", "correct horse battery")
        .await
        .expect("owner");
    let owner_session = accounts::authenticate(&base, "owner", "correct horse battery")
        .await
        .expect("owner session");
    assert!(accounts::validate_session(&base, &owner_session.token)
        .await
        .expect("validate owner session")
        .is_some());
    set_env("RAVENBOT_ACCOUNT_ID", &owner.id);
    set_env("RAVENBOT_ACCOUNT_SESSION", &owner_session.token);
    let startup = accounts::detect_startup().await.expect("owner startup");
    match startup {
        accounts::Startup::Full(account) => {
            assert!(account.workspace.is_owner);
            assert_eq!(account.workspace.data_root, base.to_string_lossy());
        }
        accounts::Startup::Launcher => panic!("expected a full account"),
    }
    assert_eq!(
        std::env::var("RAVENBOT_HOME").expect("home"),
        base.to_string_lossy()
    );
    assert!(std::env::var("RAVENBOT_KEYCHAIN_NAMESPACE").is_err());
    assert!(std::env::var("RAVENBOT_ACCOUNT_SESSION").is_err());

    let second = accounts::create_account(&base, "second", "another good password")
        .await
        .expect("second");
    let second_session = accounts::authenticate(&base, "second", "another good password")
        .await
        .expect("second session");
    std::env::remove_var("RAVENBOT_WORKSPACE_ID");
    set_env("RAVENBOT_ACCOUNT_ID", &second.id);
    set_env("RAVENBOT_ACCOUNT_SESSION", &second_session.token);
    let startup = accounts::detect_startup().await.expect("second startup");
    match startup {
        accounts::Startup::Full(account) => {
            assert!(!account.workspace.is_owner);
            assert!(PathBuf::from(&account.workspace.data_root).starts_with(base.join("accounts")));
        }
        accounts::Startup::Launcher => panic!("expected a full account"),
    }
    assert_eq!(
        std::env::var("RAVENBOT_HOME").expect("home"),
        second.data_root
    );
    assert_eq!(
        std::env::var("RAVENBOT_KEYCHAIN_NAMESPACE").expect("namespace"),
        second.id
    );
    assert_eq!(
        std::env::var("RAVENBOT_WEBHOOK_PORT").expect("webhook port"),
        "8810"
    );
    assert_eq!(
        std::env::var("RAVENBOT_CHAT_WEBHOOK_PORT").expect("chat port"),
        "8811"
    );
    assert_eq!(
        std::env::var("RAVENBOT_REMOTE_PORT").expect("remote port"),
        "8812"
    );
    assert_eq!(
        std::env::var("RAVENBOT_SERVICE_NAME").expect("service name"),
        format!("ravenbot-{}", &second.id[..8])
    );

    accounts::grant_workspace(&base, "owner", "second", "operator")
        .await
        .expect("grant");
    set_env("RAVENBOT_ACCOUNT_ID", &second.id);
    set_env("RAVENBOT_ACCOUNT_SESSION", &second_session.token);
    set_env("RAVENBOT_WORKSPACE_ID", &owner.id);
    let startup = accounts::detect_startup().await.expect("shared startup");
    match startup {
        accounts::Startup::Full(account) => {
            assert_eq!(account.actor.id, second.id);
            assert_eq!(account.workspace.id, owner.id);
            assert_eq!(account.role, "operator");
        }
        accounts::Startup::Launcher => panic!("expected a shared workspace"),
    }
    assert_eq!(
        std::env::var("RAVENBOT_HOME").expect("home"),
        base.to_string_lossy()
    );
    assert!(std::env::var("RAVENBOT_KEYCHAIN_NAMESPACE").is_err());
    clear_env();
    let _ = std::fs::remove_dir_all(&base);
}
