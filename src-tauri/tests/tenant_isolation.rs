//! Executable zero-cross-tenant-leak audit for local accounts and workspaces.
//!
//! Every test owns a unique temporary root, so it never touches the real
//! `RAVENBOT_HOME` or the shared environment variables the launcher uses. The
//! cases exercise the production functions, not a test-only shortcut:
//!
//! * the account registry and its grant resolver (`ravenbot_lib::accounts`);
//! * the per-workspace SQLite databases and the queries that read them;
//! * remote device tokens and thread-share capabilities stored in those DBs.
//!
//! The invariant each test proves is stated in its name and its doc comment.
//! The written threat model and the invariant → evidence table live in
//! `docs/TENANT-ISOLATION.md`.

use std::path::{Path, PathBuf};

use chrono::{Duration, Utc};
use ravenbot_core::{Bot, Message, Thread};
use ravenbot_db::queries::{
    BotQueries, MessageQueries, RemoteAccessQueries, ThreadQueries, ThreadShareQueries,
};
use ravenbot_db::Database;
use ravenbot_governance::{AuditLogger, AuditQuery};
use ravenbot_lib::accounts::{self, Account};
use uuid::Uuid;

const ALICE_PASSWORD: &str = "alice correct horse battery";
const BOB_PASSWORD: &str = "bob correct horse battery";
const CAROL_PASSWORD: &str = "carol correct horse battery";

/// A unique root that removes itself, even when an assertion fails late.
struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "ravenbot-tenant-isolation-{label}-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&path).expect("create the temporary account root");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Alice owns the base root, Bob and Carol own separate roots below it.
async fn three_accounts(base: &Path) -> (Account, Account, Account) {
    let alice = accounts::create_account(base, "alice", ALICE_PASSWORD)
        .await
        .expect("create alice");
    let bob = accounts::create_account(base, "bob", BOB_PASSWORD)
        .await
        .expect("create bob");
    let carol = accounts::create_account(base, "carol", CAROL_PASSWORD)
        .await
        .expect("create carol");
    (alice, bob, carol)
}

async fn workspace_db(account: &Account) -> Database {
    let path = PathBuf::from(&account.data_root).join("ravenbot.db");
    Database::new(path)
        .await
        .expect("open the workspace database")
}

/// Insert a bot, a thread and one user message through the real query layer.
async fn seed_thread(db: &Database, title: &str) -> (Bot, Thread, Message) {
    let bot = Bot::new(format!("{title} agent"), "tenant isolation fixture");
    BotQueries::insert(db.pool(), &bot)
        .await
        .expect("insert bot");
    let thread = Thread::new(bot.id, title);
    ThreadQueries::create(db.pool(), &thread)
        .await
        .expect("insert thread");
    let message = Message::user(thread.id, format!("{title} private message"));
    MessageQueries::insert(db.pool(), &message)
        .await
        .expect("insert message");
    (bot, thread, message)
}

/// Invariant: a grant opens exactly one workspace. It never widens the
/// grantee's own membership list, and it never appears in another workspace's
/// member list.
#[tokio::test]
async fn grants_resolve_only_for_the_workspace_they_target() {
    let root = TempRoot::new("grants");
    let base = root.path();
    let (alice, bob, carol) = three_accounts(base).await;

    accounts::grant_workspace(base, "alice", "carol", "viewer")
        .await
        .expect("grant carol viewer access to alice's workspace");

    let (workspace, role) = accounts::workspace_for_actor(base, &carol.id, &alice.id)
        .await
        .expect("resolve alice's workspace")
        .expect("carol has a viewer grant on alice's workspace");
    assert_eq!(workspace.id, alice.id);
    assert_eq!(role, "viewer");

    assert!(
        accounts::workspace_for_actor(base, &carol.id, &bob.id)
            .await
            .expect("resolve bob's workspace")
            .is_none(),
        "a grant on alice's workspace must not imply access to bob's workspace"
    );

    let choices = accounts::list_workspaces_for_account(base, &carol.id)
        .await
        .expect("list carol's workspaces");
    assert_eq!(choices.len(), 2);
    assert!(choices
        .iter()
        .any(|choice| choice.id == carol.id && choice.role == "owner" && choice.owner));
    assert!(choices
        .iter()
        .any(|choice| choice.id == alice.id && choice.role == "viewer" && !choice.owner));
    assert!(
        !choices.iter().any(|choice| choice.id == bob.id),
        "carol must not see bob's workspace in her workspace list"
    );

    let alice_members = accounts::workspace_members(base, "alice")
        .await
        .expect("alice members");
    assert!(alice_members
        .iter()
        .any(|member| member.username == "alice" && member.owner));
    assert!(alice_members
        .iter()
        .any(|member| member.username == "carol" && member.role == "viewer"));
    assert!(
        !alice_members.iter().any(|member| member.username == "bob"),
        "bob must not appear as a member of alice's workspace"
    );

    let bob_members = accounts::workspace_members(base, "bob")
        .await
        .expect("bob members");
    assert_eq!(bob_members.len(), 1);
    assert_eq!(bob_members[0].username, "bob");
    assert!(
        !bob_members.iter().any(|member| member.username == "carol"),
        "a viewer grant on alice's workspace must not make carol a member of bob's"
    );
}

/// Invariant: revocation is read live by the access gate. A revoked member
/// loses the workspace on the next resolve, while their identity session and
/// their own workspace stay valid.
#[tokio::test]
async fn revoking_a_grant_removes_access_immediately_and_leaves_the_session_valid() {
    let root = TempRoot::new("revocation");
    let base = root.path();
    let (alice, bob, carol) = three_accounts(base).await;

    let session = accounts::authenticate(base, "carol", CAROL_PASSWORD)
        .await
        .expect("sign carol in");
    accounts::grant_workspace(base, "alice", "carol", "viewer")
        .await
        .expect("grant carol viewer access to alice's workspace");
    assert_eq!(
        accounts::workspace_for_actor(base, &carol.id, &alice.id)
            .await
            .expect("resolve before revoke")
            .expect("carol has access before revoke")
            .1,
        "viewer"
    );

    assert!(
        accounts::revoke_workspace(base, "alice", "carol")
            .await
            .expect("revoke carol's grant"),
        "the grant must exist before it can be revoked"
    );

    // `open_workspace`, the remembered-session startup path and the 30-second
    // watch loop all call `workspace_for_actor`; none of them cache a role, so
    // this is the same gate that refuses to open the workspace.
    assert!(
        accounts::workspace_for_actor(base, &carol.id, &alice.id)
            .await
            .expect("resolve after revoke")
            .is_none(),
        "a revoked grant must not resolve, immediately and without a restart"
    );
    assert!(
        accounts::workspace_for_actor(base, &carol.id, &bob.id)
            .await
            .expect("resolve bob's workspace")
            .is_none(),
        "revoking a grant on one workspace must not create access to another"
    );

    let (own, own_role) = accounts::workspace_for_actor(base, &carol.id, &carol.id)
        .await
        .expect("resolve carol's own workspace")
        .expect("carol still owns her own workspace");
    assert_eq!(own.id, carol.id);
    assert_eq!(own_role, "owner");
    let actor = accounts::validate_session(base, &session.token)
        .await
        .expect("validate carol's session")
        .expect("revoking a workspace grant must not revoke the identity session");
    assert_eq!(actor.id, carol.id);

    let choices = accounts::list_workspaces_for_account(base, &carol.id)
        .await
        .expect("list workspaces after revoke");
    assert!(!choices.iter().any(|choice| choice.id == alice.id));
    assert!(choices.iter().any(|choice| choice.id == carol.id));

    assert!(
        !accounts::revoke_workspace(base, "alice", "carol")
            .await
            .expect("revoke an already-revoked grant"),
        "revoking a missing grant is a no-op, not a new grant"
    );
}

/// Invariant: a session token proves *who* the actor is. It is not a workspace
/// capability, so a valid token for Carol still cannot resolve Bob's workspace.
#[tokio::test]
async fn a_valid_session_identifies_an_account_but_grants_no_workspace() {
    let root = TempRoot::new("session-identity");
    let base = root.path();
    let (alice, bob, carol) = three_accounts(base).await;

    let session = accounts::authenticate(base, "carol", CAROL_PASSWORD)
        .await
        .expect("sign carol in");
    let resolved = accounts::validate_session(base, &session.token)
        .await
        .expect("validate the session")
        .expect("the freshly minted session is valid");
    assert_eq!(resolved.id, carol.id);
    assert_ne!(resolved.id, bob.id);

    // There is no token-to-workspace lookup to bypass: the only workspace gate
    // takes the resolved actor id and consults grants for that actor.
    assert!(
        accounts::workspace_for_actor(base, &resolved.id, &bob.id)
            .await
            .expect("resolve bob's workspace with carol's identity")
            .is_none(),
        "a valid session for carol must not open bob's workspace"
    );
    assert!(
        accounts::workspace_for_actor(base, &resolved.id, &alice.id)
            .await
            .expect("resolve alice's workspace with carol's identity")
            .is_none(),
        "carol has no grant on alice's workspace"
    );

    let unknown = "0".repeat(64);
    assert!(
        accounts::validate_session(base, &unknown)
            .await
            .expect("validate an unknown token")
            .is_none(),
        "an invented token must not resolve to any account"
    );
}

/// Invariant: each account root contains its own database. Rows written in one
/// workspace cannot be read by id or by listing through another workspace's
/// queries, and the audit log is separate too.
#[tokio::test]
async fn each_account_root_holds_an_independent_database() {
    let root = TempRoot::new("data-roots");
    let base = root.path();
    let (alice, bob, carol) = three_accounts(base).await;

    assert_eq!(Path::new(&alice.data_root), base);
    assert_ne!(alice.data_root, bob.data_root);
    assert_ne!(alice.data_root, carol.data_root);
    assert_ne!(bob.data_root, carol.data_root);
    assert!(Path::new(&bob.data_root).starts_with(base.join("accounts")));
    assert!(Path::new(&carol.data_root).starts_with(base.join("accounts")));

    let a_db = workspace_db(&alice).await;
    let b_db = workspace_db(&bob).await;
    let (a_bot, a_thread, a_message) = seed_thread(&a_db, "alice private thread").await;
    let (b_bot, b_thread, b_message) = seed_thread(&b_db, "bob private thread").await;
    assert_ne!(a_bot.id, b_bot.id);

    let a_audit = AuditLogger::new(a_db.pool().clone());
    a_audit
        .log_model_call(
            a_bot.id,
            None,
            Some(a_thread.id),
            "isolation-provider",
            "isolation-model",
            11,
            7,
            0.01,
        )
        .await
        .expect("write an audit row in alice's database");

    // Alice sees every row she wrote.
    let a_threads = ThreadQueries::list_by_bot(a_db.pool(), a_bot.id)
        .await
        .expect("list alice's threads");
    assert_eq!(a_threads.len(), 1);
    assert_eq!(a_threads[0].id, a_thread.id);
    let a_messages = MessageQueries::list_by_thread(a_db.pool(), a_thread.id)
        .await
        .expect("list alice's messages");
    assert_eq!(a_messages.len(), 1);
    assert_eq!(a_messages[0].id, a_message.id);
    let a_page = a_audit
        .query(&AuditQuery {
            bot_id: Some(a_bot.id.to_string()),
            ..Default::default()
        })
        .await
        .expect("query alice's audit log");
    assert_eq!(a_page.total, 1);
    assert_eq!(a_page.entries.len(), 1);

    // Bob's database has Bob's rows only. Alice's ids do not resolve.
    assert!(
        BotQueries::get(b_db.pool(), a_bot.id)
            .await
            .expect("look up alice's bot in bob's database")
            .is_none(),
        "alice's bot id must not resolve in bob's database"
    );
    assert!(
        ThreadQueries::get(b_db.pool(), a_thread.id)
            .await
            .expect("look up alice's thread in bob's database")
            .is_none(),
        "alice's thread id must not resolve in bob's database"
    );
    assert!(ThreadQueries::list_by_bot(b_db.pool(), a_bot.id)
        .await
        .expect("list alice's threads from bob's database")
        .is_empty());
    assert!(
        MessageQueries::list_by_thread(b_db.pool(), a_thread.id)
            .await
            .expect("list alice's messages from bob's database")
            .is_empty(),
        "alice's thread must expose no messages through bob's database"
    );
    let b_page = AuditLogger::new(b_db.pool().clone())
        .query(&AuditQuery::default())
        .await
        .expect("query bob's audit log");
    assert_eq!(b_page.total, 0);
    assert!(b_page.entries.is_empty());

    // The same holds in the other direction: bob's rows are visible only to
    // bob, and alice's audit view contains no row from bob's database.
    assert!(ThreadQueries::get(a_db.pool(), b_thread.id)
        .await
        .expect("look up bob's thread in alice's database")
        .is_none());
    assert!(MessageQueries::list_by_thread(a_db.pool(), b_thread.id)
        .await
        .expect("list bob's messages from alice's database")
        .is_empty());
    assert_eq!(
        MessageQueries::list_by_thread(b_db.pool(), b_thread.id)
            .await
            .expect("list bob's messages in bob's database")
            .first()
            .map(|message| message.id),
        Some(b_message.id)
    );
    assert_eq!(
        AuditLogger::new(a_db.pool().clone())
            .query(&AuditQuery::default())
            .await
            .expect("query alice's full audit log")
            .total,
        1
    );
}

/// Invariant: a paired remote device is a bearer credential in exactly one
/// workspace database. The raw token is never stored, and the same token does
/// not resolve in another workspace.
#[tokio::test]
async fn remote_device_tokens_are_workspace_scoped() {
    let root = TempRoot::new("remote-device");
    let base = root.path();
    let (alice, bob, _carol) = three_accounts(base).await;
    let a_db = workspace_db(&alice).await;
    let b_db = workspace_db(&bob).await;

    let a_token = "alice-phone-device-token";
    let a_hash = ravenbot_remote::hash_secret(a_token);
    let a_device =
        RemoteAccessQueries::create_device(a_db.pool(), "Alice phone", "viewer", &a_hash)
            .await
            .expect("pair a device in alice's workspace");

    let resolved = RemoteAccessQueries::device_for_token(a_db.pool(), &a_hash)
        .await
        .expect("resolve alice's device token")
        .expect("the token resolves in alice's workspace");
    assert_eq!(resolved.id, a_device.to_string());
    assert_eq!(resolved.name, "Alice phone");

    let stored: String = sqlx::query_scalar("SELECT token_hash FROM remote_devices LIMIT 1")
        .fetch_one(a_db.pool())
        .await
        .expect("read the stored token hash");
    assert_eq!(stored, a_hash);
    assert_ne!(
        stored, a_token,
        "the raw device token must never be stored in the database"
    );

    assert!(
        RemoteAccessQueries::device_for_token(b_db.pool(), &a_hash)
            .await
            .expect("resolve alice's device token in bob's workspace")
            .is_none(),
        "alice's device token must not authenticate against bob's workspace"
    );
    assert!(RemoteAccessQueries::list_devices(b_db.pool())
        .await
        .expect("list bob's devices")
        .is_empty());

    // Bob's device is equally invisible to alice's workspace.
    let b_hash = ravenbot_remote::hash_secret("bob-tablet-device-token");
    RemoteAccessQueries::create_device(b_db.pool(), "Bob tablet", "operator", &b_hash)
        .await
        .expect("pair a device in bob's workspace");
    assert!(RemoteAccessQueries::device_for_token(a_db.pool(), &b_hash)
        .await
        .expect("resolve bob's device token in alice's workspace")
        .is_none());

    // Revocation in alice's database does not disturb bob's device.
    assert!(RemoteAccessQueries::revoke_device(a_db.pool(), a_device)
        .await
        .expect("revoke alice's device"));
    assert!(RemoteAccessQueries::device_for_token(a_db.pool(), &a_hash)
        .await
        .expect("resolve the revoked token")
        .is_none());
    assert!(RemoteAccessQueries::device_for_token(b_db.pool(), &b_hash)
        .await
        .expect("resolve bob's device after alice revoked hers")
        .is_some());
}

/// Invariant: a thread-share link is a read-only capability in exactly one
/// workspace database. Minting or revoking a share in one workspace cannot
/// expose or close a share in another.
#[tokio::test]
async fn thread_share_tokens_are_workspace_scoped() {
    let root = TempRoot::new("thread-share");
    let base = root.path();
    let (alice, bob, _carol) = three_accounts(base).await;
    let a_db = workspace_db(&alice).await;
    let b_db = workspace_db(&bob).await;
    let (_a_bot, a_thread, _) = seed_thread(&a_db, "alice shared thread").await;
    let (_b_bot, b_thread, _) = seed_thread(&b_db, "bob private thread").await;

    let expires = (Utc::now() + Duration::hours(1)).to_rfc3339();
    let a_hash = ravenbot_remote::hash_secret("alice-thread-share-token");
    let b_hash = ravenbot_remote::hash_secret("bob-thread-share-token");
    let a_share = ThreadShareQueries::create(a_db.pool(), a_thread.id, &a_hash, &expires)
        .await
        .expect("create a share in alice's workspace");
    let b_share = ThreadShareQueries::create(b_db.pool(), b_thread.id, &b_hash, &expires)
        .await
        .expect("create a share in bob's workspace");
    assert_ne!(a_share.id, b_share.id);

    let resolved = ThreadShareQueries::for_token(a_db.pool(), &a_hash)
        .await
        .expect("resolve alice's share token")
        .expect("the share resolves in alice's workspace");
    assert_eq!(resolved.thread_id, a_thread.id);
    assert!(
        ThreadShareQueries::for_token(a_db.pool(), &b_hash)
            .await
            .expect("resolve bob's share token in alice's workspace")
            .is_none(),
        "bob's share token must not resolve in alice's workspace"
    );
    assert!(
        ThreadShareQueries::for_token(b_db.pool(), &a_hash)
            .await
            .expect("resolve alice's share token in bob's workspace")
            .is_none(),
        "alice's share token must not resolve in bob's workspace"
    );
    assert_eq!(
        ThreadShareQueries::for_token(b_db.pool(), &b_hash)
            .await
            .expect("resolve bob's share token in bob's workspace")
            .expect("bob's share resolves in bob's workspace")
            .thread_id,
        b_thread.id
    );

    // Revocation is local to the workspace that minted the share.
    ThreadShareQueries::revoke(a_db.pool(), a_share.id)
        .await
        .expect("revoke alice's share");
    assert!(ThreadShareQueries::for_token(a_db.pool(), &a_hash)
        .await
        .expect("resolve the revoked share")
        .is_none());
    assert!(ThreadShareQueries::for_token(b_db.pool(), &b_hash)
        .await
        .expect("resolve bob's share after alice revoked hers")
        .is_some());
}

/// Invariant: usernames are unique case-insensitively, and a disabled account
/// cannot authenticate, keep a session, or resolve into any workspace (its own
/// or one where a stale grant still names it).
#[tokio::test]
async fn duplicate_usernames_are_rejected_and_disabled_accounts_cannot_enter() {
    let root = TempRoot::new("identity");
    let base = root.path();
    let alice = accounts::create_account(base, "Alice", ALICE_PASSWORD)
        .await
        .expect("create Alice");
    let duplicate = accounts::create_account(base, "alice", BOB_PASSWORD)
        .await
        .expect_err("a case-insensitive duplicate username must be rejected");
    assert!(
        duplicate.contains("already taken"),
        "unexpected duplicate-username error: {duplicate}"
    );
    assert!(accounts::account_by_username(base, "ALICE")
        .await
        .expect("look up Alice case-insensitively")
        .is_some());

    let dave = accounts::create_account(base, "dave", "dave correct horse battery")
        .await
        .expect("create dave");
    let session = accounts::authenticate(base, "dave", "dave correct horse battery")
        .await
        .expect("sign dave in");
    accounts::grant_workspace(base, "Alice", "dave", "viewer")
        .await
        .expect("grant dave access before disabling him");
    assert!(accounts::workspace_for_actor(base, &dave.id, &alice.id)
        .await
        .expect("resolve dave's grant before disabling")
        .is_some());

    accounts::set_disabled(base, "dave", true)
        .await
        .expect("disable dave");
    assert!(
        accounts::authenticate(base, "dave", "dave correct horse battery")
            .await
            .is_err(),
        "a disabled account must not authenticate"
    );
    assert!(
        accounts::validate_session(base, &session.token)
            .await
            .expect("validate dave's session after disabling")
            .is_none(),
        "disabling an account must invalidate its existing sessions"
    );
    assert!(
        accounts::workspace_for_actor(base, &dave.id, &dave.id)
            .await
            .expect("resolve dave's own disabled workspace")
            .is_none(),
        "a disabled account must not resolve into its own workspace"
    );
    assert!(
        accounts::workspace_for_actor(base, &dave.id, &alice.id)
            .await
            .expect("resolve dave's stale grant after disabling")
            .is_none(),
        "a stale grant must not resolve a disabled actor into another workspace"
    );
    assert!(
        accounts::add_workspace_member(base, "Alice", "dave", None, "viewer")
            .await
            .is_err(),
        "a disabled account cannot be re-added as a workspace member"
    );
}
