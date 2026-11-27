//! End-to-end OIDC tests against a loopback identity provider.
//!
//! The RSA key in `fixtures/sso_test_key.pem` is **test-only**, committed on
//! purpose, and must never be used for anything except these tests. It signs
//! the mock IdP's ID tokens with RS256. No key is generated per test.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use axum::extract::{Form, State as AxumState};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use jsonwebtoken::jwk::{AlgorithmParameters, Jwk};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use ravenbot_remote::{
    generate_token, hash_secret, start_with_authenticator, AccountAuthenticator, RemoteAccount,
    RemoteConfig, SsoConfig,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use url::Url;
use uuid::Uuid;

/// Committed test fixture. The matching private key is intentionally public:
/// it only signs tokens for this test module and has no other use.
const SSO_TEST_ONLY_KEY: &[u8] = include_bytes!("fixtures/sso_test_key.pem");
const CLIENT_ID: &str = "ravenbot-test-client";
const KID: &str = "sso-test-key";

// ─────────────────────────────────────────────────────────────────────────────
// Mock identity provider
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenMode {
    Rs256,
    NoneAlg,
    Hs256,
    TamperedSignature,
}

#[derive(Debug, Clone)]
struct TokenSpec {
    issuer: Option<String>,
    audience: Option<String>,
    authorized_party: Option<String>,
    nonce: Option<String>,
    omit_nonce: bool,
    username: Option<String>,
    omit_username: bool,
    expires_in: i64,
    mode: TokenMode,
}

impl Default for TokenSpec {
    fn default() -> Self {
        Self {
            issuer: None,
            audience: None,
            authorized_party: None,
            nonce: None,
            omit_nonce: false,
            username: None,
            omit_username: false,
            expires_in: 300,
            mode: TokenMode::Rs256,
        }
    }
}

impl TokenSpec {
    fn username(mut self, username: &str) -> Self {
        self.username = Some(username.to_string());
        self
    }
}

#[derive(Clone)]
struct Grant {
    client_id: String,
    redirect_uri: String,
    challenge: String,
    nonce: String,
    spec: TokenSpec,
}

struct IdpInner {
    base: String,
    key: EncodingKey,
    jwk: Jwk,
    codes: Mutex<HashMap<String, Grant>>,
    counter: AtomicUsize,
}

#[derive(Clone)]
struct MockIdp {
    inner: Arc<IdpInner>,
}

impl MockIdp {
    fn base(&self) -> &str {
        &self.inner.base
    }

    /// Register the authorization request the browser would have approved and
    /// return the one-time code.
    fn issue_code(&self, authorization_url: &str, spec: TokenSpec) -> String {
        let url = Url::parse(authorization_url).expect("authorization URL");
        let params: HashMap<String, String> = url.query_pairs().into_owned().collect();
        let nonce = params.get("nonce").expect("nonce").clone();
        let challenge = params.get("code_challenge").expect("challenge").clone();
        assert_eq!(
            params.get("code_challenge_method").map(String::as_str),
            Some("S256")
        );
        let code = format!("code-{}", self.inner.counter.fetch_add(1, Ordering::SeqCst));
        self.inner.codes.lock().expect("codes").insert(
            code.clone(),
            Grant {
                client_id: params.get("client_id").expect("client_id").clone(),
                redirect_uri: params.get("redirect_uri").expect("redirect_uri").clone(),
                challenge,
                nonce,
                spec,
            },
        );
        code
    }
}

async fn start_idp() -> MockIdp {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock IdP bind");
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let key = EncodingKey::from_rsa_pem(SSO_TEST_ONLY_KEY).expect("test-only RSA key");
    let mut jwk = Jwk::from_encoding_key(&key, Algorithm::RS256).expect("test JWK");
    jwk.common.key_id = Some(KID.to_string());
    assert!(matches!(jwk.algorithm, AlgorithmParameters::RSA(_)));
    let idp = MockIdp {
        inner: Arc::new(IdpInner {
            base: base.clone(),
            key,
            jwk,
            codes: Mutex::new(HashMap::new()),
            counter: AtomicUsize::new(0),
        }),
    };
    let app = Router::new()
        .route("/.well-known/openid-configuration", get(discovery))
        .route("/jwks", get(jwks))
        .route("/token", post(token))
        .with_state(idp.clone());
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("mock IdP serve");
    });
    idp
}

async fn discovery(AxumState(idp): AxumState<MockIdp>) -> Json<Value> {
    Json(json!({
        "issuer": idp.base(),
        "authorization_endpoint": format!("{}/authorize", idp.base()),
        "token_endpoint": format!("{}/token", idp.base()),
        "jwks_uri": format!("{}/jwks", idp.base()),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code"],
        "code_challenge_methods_supported": ["S256"],
    }))
}

async fn jwks(AxumState(idp): AxumState<MockIdp>) -> Json<Value> {
    Json(json!({ "keys": [idp.inner.jwk.clone()] }))
}

async fn token(
    AxumState(idp): AxumState<MockIdp>,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let Some(code) = form.get("code") else {
        return (axum::http::StatusCode::BAD_REQUEST, "missing code").into_response();
    };
    let Some(grant) = idp.inner.codes.lock().expect("codes").remove(code) else {
        return (axum::http::StatusCode::BAD_REQUEST, "unknown code").into_response();
    };
    if form.get("grant_type").map(String::as_str) != Some("authorization_code")
        || form.get("client_id") != Some(&grant.client_id)
        || form.get("redirect_uri") != Some(&grant.redirect_uri)
    {
        return (axum::http::StatusCode::BAD_REQUEST, "bad grant").into_response();
    }
    let Some(verifier) = form.get("code_verifier") else {
        return (axum::http::StatusCode::BAD_REQUEST, "missing verifier").into_response();
    };
    let expected = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    if expected != grant.challenge {
        return (axum::http::StatusCode::BAD_REQUEST, "pkce mismatch").into_response();
    }

    let now = chrono::Utc::now().timestamp();
    let issuer = grant
        .spec
        .issuer
        .clone()
        .unwrap_or_else(|| idp.base().to_string());
    let audience = grant
        .spec
        .audience
        .clone()
        .unwrap_or_else(|| grant.client_id.clone());
    let mut claims = json!({
        "iss": issuer,
        "aud": audience,
        "sub": "subject-alice",
        "iat": now,
        "exp": now + grant.spec.expires_in,
        "preferred_username": grant.spec.username.clone().unwrap_or_else(|| "alice".to_string()),
    });
    if grant.spec.omit_username {
        claims
            .as_object_mut()
            .expect("claims object")
            .remove("preferred_username");
    }
    if let Some(authorized_party) = &grant.spec.authorized_party {
        claims["azp"] = json!(authorized_party);
    }
    if let Some(nonce) = &grant.spec.nonce {
        claims["nonce"] = json!(nonce);
    } else if !grant.spec.omit_nonce {
        claims["nonce"] = json!(grant.nonce);
    }

    let id_token = match grant.spec.mode {
        TokenMode::Rs256 => sign_rs256(&idp.inner.key, &claims),
        TokenMode::TamperedSignature => {
            let good = sign_rs256(&idp.inner.key, &claims);
            let mut parts: Vec<&str> = good.split('.').collect();
            let fake = URL_SAFE_NO_PAD.encode([0u8; 256]);
            parts[2] = &fake;
            parts.join(".")
        }
        TokenMode::NoneAlg => {
            let header = json!({ "alg": "none", "typ": "JWT", "kid": KID });
            let header = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap());
            let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap());
            format!("{header}.{payload}.")
        }
        TokenMode::Hs256 => {
            let mut header = Header::new(Algorithm::HS256);
            header.kid = Some(KID.to_string());
            encode(
                &header,
                &claims,
                &EncodingKey::from_secret(b"not-the-rsa-key").expect("HMAC key"),
            )
            .expect("HS256 token")
        }
    };
    Json(json!({
        "access_token": "access-token-that-must-never-be-stored",
        "token_type": "Bearer",
        "expires_in": 300,
        "id_token": id_token,
    }))
    .into_response()
}

fn sign_rs256(key: &EncodingKey, claims: &Value) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(KID.to_string());
    encode(&header, claims, key).expect("RS256 token")
}

// ─────────────────────────────────────────────────────────────────────────────
// Remote listener and account directory
// ─────────────────────────────────────────────────────────────────────────────

struct Directory {
    accounts: Mutex<Vec<RemoteAccount>>,
    password: String,
}

impl Directory {
    fn new(accounts: Vec<RemoteAccount>, password: &str) -> Self {
        Self {
            accounts: Mutex::new(accounts),
            password: password.to_string(),
        }
    }

    fn remove_all(&self) {
        self.accounts.lock().expect("accounts").clear();
    }
}

#[async_trait::async_trait]
impl AccountAuthenticator for Directory {
    async fn authenticate(
        &self,
        username: &str,
        password: &str,
    ) -> Result<Option<RemoteAccount>, String> {
        if password != self.password {
            return Ok(None);
        }
        Ok(self
            .accounts
            .lock()
            .expect("accounts")
            .iter()
            .find(|account| account.username.eq_ignore_ascii_case(username.trim()))
            .cloned())
    }

    async fn revalidate(&self, account_id: &str) -> Result<Option<RemoteAccount>, String> {
        Ok(self
            .accounts
            .lock()
            .expect("accounts")
            .iter()
            .find(|account| account.id == account_id)
            .cloned())
    }

    async fn account_by_identity(&self, identity: &str) -> Result<Option<RemoteAccount>, String> {
        Ok(self
            .accounts
            .lock()
            .expect("accounts")
            .iter()
            .find(|account| account.username.eq_ignore_ascii_case(identity.trim()))
            .cloned())
    }
}

fn alice() -> RemoteAccount {
    RemoteAccount {
        id: "account-alice".to_string(),
        username: "alice".to_string(),
        role: "owner".to_string(),
    }
}

struct Harness {
    server: Option<ravenbot_remote::RemoteServer>,
    pool: SqlitePool,
    base: String,
    callback_url: String,
    db_path: PathBuf,
}

impl Harness {
    async fn stop(mut self) {
        if let Some(server) = self.server.take() {
            server.shutdown().await;
        }
        let _ = self.pool.close().await;
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", self.db_path.display()));
        }
    }
}

fn redirect_origin(callback_url: &str) -> String {
    Url::parse(callback_url)
        .expect("callback URL")
        .origin()
        .ascii_serialization()
}

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("free port");
    listener.local_addr().expect("addr").port()
}

async fn start_harness(
    idp: &MockIdp,
    authenticator: Arc<dyn AccountAuthenticator>,
    sso_enabled: bool,
) -> Harness {
    let port = free_port();
    let db_path = std::env::temp_dir().join(format!("ravenbot-sso-test-{}.db", Uuid::new_v4()));
    let options = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .expect("test database");
    ravenbot_db::migrations::run(&pool)
        .await
        .expect("migrations");
    let callback_url = format!("http://127.0.0.1:{port}/api/sso/callback");
    let sso = sso_enabled.then(|| {
        SsoConfig::new(
            idp.base(),
            CLIENT_ID,
            &callback_url,
            "openid profile email",
            "preferred_username",
        )
        .expect("valid SSO config")
    });
    let config = RemoteConfig {
        port,
        allow_lan: false,
        tls: false,
        tls_dir: None,
        sso,
        scim: None,
    };
    let db = ravenbot_db::Database::from_pool(pool.clone());
    let runtime = Arc::new(ravenbot_runtime::Runtime::new(db));
    let server = start_with_authenticator(pool.clone(), runtime, config, Some(authenticator))
        .await
        .expect("remote listener");
    let base = format!("http://127.0.0.1:{}", server.addr().port());
    Harness {
        server: Some(server),
        pool,
        base,
        callback_url,
        db_path,
    }
}

async fn new_device(pool: &SqlitePool, name: &str, role: &str) -> (String, String) {
    let token = generate_token();
    let id = ravenbot_db::queries::RemoteAccessQueries::create_device(
        pool,
        name,
        role,
        &hash_secret(&token),
    )
    .await
    .expect("device");
    (token, id.to_string())
}

async fn post_start(client: &reqwest::Client, base: &str, token: &str) -> reqwest::Response {
    client
        .post(format!("{base}/api/sso/start"))
        .bearer_auth(token)
        .send()
        .await
        .expect("sso start request")
}

async fn start_flow(client: &reqwest::Client, base: &str, token: &str) -> Url {
    let response = post_start(client, base, token).await;
    assert_eq!(
        response.status(),
        reqwest::StatusCode::OK,
        "{}",
        response.text().await.unwrap_or_default()
    );
    let body: Value = response.json().await.expect("start JSON");
    Url::parse(body["url"].as_str().expect("authorization URL")).expect("url")
}

fn state_of(authorization_url: &Url) -> String {
    authorization_url
        .query_pairs()
        .find(|(key, _)| key == "state")
        .map(|(_, value)| value.into_owned())
        .expect("state")
}

fn no_redirect_client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client")
}

async fn callback(
    client: &reqwest::Client,
    base: &str,
    query: &[(&str, String)],
) -> reqwest::Response {
    let mut url = Url::parse(&format!("{base}/api/sso/callback")).expect("callback URL");
    url.query_pairs_mut()
        .extend_pairs(query.iter().map(|(key, value)| (*key, value.as_str())));
    client.get(url).send().await.expect("callback request")
}

fn location(response: &reqwest::Response) -> String {
    response
        .headers()
        .get(reqwest::header::LOCATION)
        .expect("Location header")
        .to_str()
        .expect("location text")
        .to_string()
}

fn assert_reason(response: &reqwest::Response, reason: &str) {
    assert_eq!(response.status(), reqwest::StatusCode::SEE_OTHER);
    let location = location(response);
    assert!(
        location.contains(&format!("sso=error&reason={reason}")),
        "{location}"
    );
}

async fn account_name(pool: &SqlitePool, device_id: &str) -> Option<String> {
    ravenbot_db::queries::RemoteAccessQueries::device_by_id(pool, device_id)
        .await
        .expect("device")
        .expect("device row")
        .account_name
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn the_start_url_carries_the_oidc_parameters_and_a_valid_callback_binds_the_device() {
    let idp = start_idp().await;
    let directory = Arc::new(Directory::new(vec![alice()], "correct horse battery"));
    let harness = start_harness(&idp, directory, true).await;
    let (token, device_id) = new_device(&harness.pool, "Alice phone", "admin").await;
    let client = no_redirect_client();

    let authorization_url = start_flow(&client, &harness.base, &token).await;
    let params: HashMap<String, String> = authorization_url.query_pairs().into_owned().collect();
    assert_eq!(params["response_type"], "code");
    assert_eq!(params["client_id"], CLIENT_ID);
    assert_eq!(params["redirect_uri"], harness.callback_url);
    assert_eq!(params["scope"], "openid profile email");
    assert_eq!(params["state"].len(), 64);
    assert_eq!(params["nonce"].len(), 64);
    assert_eq!(params["code_challenge"].len(), 43);
    assert_eq!(params["code_challenge_method"], "S256");

    let code = idp.issue_code(
        authorization_url.as_str(),
        TokenSpec::default().username("ALICE"),
    );
    let response = callback(
        &client,
        &harness.base,
        &[("code", code), ("state", state_of(&authorization_url))],
    )
    .await;
    assert_eq!(response.status(), reqwest::StatusCode::SEE_OTHER);
    assert_eq!(
        location(&response),
        format!("{}/?sso=ok", redirect_origin(&harness.callback_url))
    );
    assert_eq!(
        account_name(&harness.pool, &device_id).await.as_deref(),
        Some("alice")
    );

    let status: Value = client
        .get(format!("{}/api/status", harness.base))
        .bearer_auth(&token)
        .send()
        .await
        .expect("status")
        .json()
        .await
        .expect("status JSON");
    assert_eq!(status["account"]["username"], "alice");
    assert_eq!(status["device"]["role"], "admin");
    assert_eq!(status["sso_enabled"], true);

    let audit = ravenbot_db::queries::RemoteAccessQueries::recent_audit(&harness.pool, 20)
        .await
        .expect("audit");
    let sso = audit
        .iter()
        .find(|entry| entry.method == "SSO" && entry.status == 200)
        .expect("SSO audit row");
    assert_eq!(sso.account_name.as_deref(), Some("alice"));
    assert_eq!(sso.role.as_deref(), Some("admin"));

    // The verifier and nonce are deleted once the flow is consumed.
    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM remote_sso_attempts")
        .fetch_one(&harness.pool)
        .await
        .expect("attempt count");
    assert_eq!(remaining, 0);
    harness.stop().await;
}

#[tokio::test]
async fn replaying_a_consumed_state_fails_and_does_not_bind() {
    let idp = start_idp().await;
    let directory = Arc::new(Directory::new(vec![alice()], "correct horse battery"));
    let harness = start_harness(&idp, directory, true).await;
    let (token, device_id) = new_device(&harness.pool, "Phone", "admin").await;
    let client = no_redirect_client();

    let authorization_url = start_flow(&client, &harness.base, &token).await;
    let state = state_of(&authorization_url);
    // Consume the state with a token that fails validation.
    let code = idp.issue_code(
        authorization_url.as_str(),
        TokenSpec {
            nonce: Some("wrong-nonce".to_string()),
            ..TokenSpec::default()
        },
    );
    let first = callback(
        &client,
        &harness.base,
        &[("code", code.clone()), ("state", state.clone())],
    )
    .await;
    assert_reason(&first, "token");
    assert!(account_name(&harness.pool, &device_id).await.is_none());

    let replay = callback(&client, &harness.base, &[("code", code), ("state", state)]).await;
    assert_reason(&replay, "state");
    assert!(account_name(&harness.pool, &device_id).await.is_none());
    harness.stop().await;
}

#[tokio::test]
async fn invalid_id_tokens_and_wrong_algorithms_fail_closed() {
    let idp = start_idp().await;
    let directory = Arc::new(Directory::new(vec![alice()], "correct horse battery"));
    let harness = start_harness(&idp, directory, true).await;
    let client = no_redirect_client();

    let cases = [
        (
            "wrong nonce",
            TokenSpec {
                nonce: Some("other-nonce".to_string()),
                ..TokenSpec::default()
            },
        ),
        (
            "missing nonce",
            TokenSpec {
                omit_nonce: true,
                ..TokenSpec::default()
            },
        ),
        (
            "wrong issuer",
            TokenSpec {
                issuer: Some("https://evil.example".to_string()),
                ..TokenSpec::default()
            },
        ),
        (
            "wrong audience",
            TokenSpec {
                audience: Some("another-client".to_string()),
                ..TokenSpec::default()
            },
        ),
        (
            "wrong authorized party",
            TokenSpec {
                authorized_party: Some("another-client".to_string()),
                ..TokenSpec::default()
            },
        ),
        (
            "expired",
            TokenSpec {
                expires_in: -300,
                ..TokenSpec::default()
            },
        ),
        (
            "tampered signature",
            TokenSpec {
                mode: TokenMode::TamperedSignature,
                ..TokenSpec::default()
            },
        ),
        (
            "alg none",
            TokenSpec {
                mode: TokenMode::NoneAlg,
                ..TokenSpec::default()
            },
        ),
        (
            "alg HS256",
            TokenSpec {
                mode: TokenMode::Hs256,
                ..TokenSpec::default()
            },
        ),
    ];
    for (name, spec) in cases {
        let (token, device_id) = new_device(&harness.pool, name, "admin").await;
        let authorization_url = start_flow(&client, &harness.base, &token).await;
        let code = idp.issue_code(authorization_url.as_str(), spec);
        let response = callback(
            &client,
            &harness.base,
            &[("code", code), ("state", state_of(&authorization_url))],
        )
        .await;
        assert_reason(&response, "token");
        assert!(
            account_name(&harness.pool, &device_id).await.is_none(),
            "{name} must not bind"
        );
    }
    harness.stop().await;
}

#[tokio::test]
async fn a_bad_pkce_verifier_fails_at_the_token_endpoint_without_binding() {
    let idp = start_idp().await;
    let directory = Arc::new(Directory::new(vec![alice()], "correct horse battery"));
    let harness = start_harness(&idp, directory, true).await;
    let (token, device_id) = new_device(&harness.pool, "Phone", "admin").await;
    let client = no_redirect_client();

    let authorization_url = start_flow(&client, &harness.base, &token).await;
    let code = idp.issue_code(authorization_url.as_str(), TokenSpec::default());
    // Replace the recoverable verifier after start. The mock IdP recomputes
    // S256(verifier) and rejects the mismatch, so this exercises a real PKCE
    // failure rather than a canned error response.
    sqlx::query("UPDATE remote_sso_attempts SET verifier = 'not-the-verifier'")
        .execute(&harness.pool)
        .await
        .expect("tamper verifier");
    let response = callback(
        &client,
        &harness.base,
        &[("code", code), ("state", state_of(&authorization_url))],
    )
    .await;
    assert_reason(&response, "exchange");
    assert!(account_name(&harness.pool, &device_id).await.is_none());
    harness.stop().await;
}

#[tokio::test]
async fn unknown_and_no_longer_authorized_accounts_fail_with_reason_account() {
    let idp = start_idp().await;
    let directory = Arc::new(Directory::new(vec![alice()], "correct horse battery"));
    let harness = start_harness(&idp, directory.clone(), true).await;
    let client = no_redirect_client();

    // Unknown identity.
    let (token, device_id) = new_device(&harness.pool, "Phone", "admin").await;
    let authorization_url = start_flow(&client, &harness.base, &token).await;
    let code = idp.issue_code(
        authorization_url.as_str(),
        TokenSpec::default().username("ghost"),
    );
    let response = callback(
        &client,
        &harness.base,
        &[("code", code), ("state", state_of(&authorization_url))],
    )
    .await;
    assert_reason(&response, "account");
    assert!(account_name(&harness.pool, &device_id).await.is_none());

    // The same identity now resolves to no account, which is how the real
    // authenticator reports a disabled or grant-less account.
    directory.remove_all();
    let (token, device_id) = new_device(&harness.pool, "Phone 2", "admin").await;
    let authorization_url = start_flow(&client, &harness.base, &token).await;
    let code = idp.issue_code(
        authorization_url.as_str(),
        TokenSpec::default().username("alice"),
    );
    let response = callback(
        &client,
        &harness.base,
        &[("code", code), ("state", state_of(&authorization_url))],
    )
    .await;
    assert_reason(&response, "account");
    assert!(account_name(&harness.pool, &device_id).await.is_none());
    harness.stop().await;
}

#[tokio::test]
async fn a_missing_or_non_string_claim_fails_with_reason_account() {
    let idp = start_idp().await;
    let directory = Arc::new(Directory::new(vec![alice()], "correct horse battery"));
    let harness = start_harness(&idp, directory, true).await;
    let client = no_redirect_client();
    let (token, device_id) = new_device(&harness.pool, "Phone", "admin").await;
    let authorization_url = start_flow(&client, &harness.base, &token).await;
    let code = idp.issue_code(
        authorization_url.as_str(),
        TokenSpec {
            omit_username: true,
            ..TokenSpec::default()
        },
    );
    let response = callback(
        &client,
        &harness.base,
        &[("code", code), ("state", state_of(&authorization_url))],
    )
    .await;
    assert_reason(&response, "account");
    assert!(account_name(&harness.pool, &device_id).await.is_none());
    harness.stop().await;
}

#[tokio::test]
async fn a_device_revoked_between_start_and_callback_does_not_bind() {
    let idp = start_idp().await;
    let directory = Arc::new(Directory::new(vec![alice()], "correct horse battery"));
    let harness = start_harness(&idp, directory, true).await;
    let (token, device_id) = new_device(&harness.pool, "Lost phone", "admin").await;
    let client = no_redirect_client();

    let authorization_url = start_flow(&client, &harness.base, &token).await;
    let code = idp.issue_code(authorization_url.as_str(), TokenSpec::default());
    ravenbot_db::queries::RemoteAccessQueries::revoke_device(
        &harness.pool,
        Uuid::parse_str(&device_id).expect("device uuid"),
    )
    .await
    .expect("revoke");
    let response = callback(
        &client,
        &harness.base,
        &[("code", code), ("state", state_of(&authorization_url))],
    )
    .await;
    assert_reason(&response, "account");
    assert!(account_name(&harness.pool, &device_id).await.is_none());
    assert_eq!(
        client
            .get(format!("{}/api/status", harness.base))
            .bearer_auth(&token)
            .send()
            .await
            .expect("status")
            .status(),
        reqwest::StatusCode::UNAUTHORIZED
    );
    harness.stop().await;
}

#[tokio::test]
async fn sso_disabled_returns_409_and_the_mobile_status_keeps_only_a_boolean() {
    let idp = start_idp().await;
    let directory = Arc::new(Directory::new(vec![alice()], "correct horse battery"));
    let harness = start_harness(&idp, directory, false).await;
    let (token, _) = new_device(&harness.pool, "Phone", "admin").await;
    let client = no_redirect_client();

    let response = post_start(&client, &harness.base, &token).await;
    assert_eq!(response.status(), reqwest::StatusCode::CONFLICT);

    let status: Value = client
        .get(format!("{}/api/status", harness.base))
        .bearer_auth(&token)
        .send()
        .await
        .expect("status")
        .json()
        .await
        .expect("status JSON");
    assert_eq!(status["sso_enabled"], false);
    assert!(status.get("sso").is_none(), "the summary is desktop-only");
    harness.stop().await;
}

#[tokio::test]
async fn the_redirect_origin_comes_from_config_not_the_request_host() {
    let idp = start_idp().await;
    let directory = Arc::new(Directory::new(vec![alice()], "correct horse battery"));
    let harness = start_harness(&idp, directory, true).await;
    let client = no_redirect_client();

    let mut url = Url::parse(&format!("{}/api/sso/callback", harness.base)).expect("callback");
    url.query_pairs_mut()
        .append_pair("state", "unknown-state")
        .append_pair("code", "unknown-code");
    let response = client
        .get(url)
        .header(reqwest::header::HOST, "evil.example")
        .send()
        .await
        .expect("callback");
    assert_reason(&response, "state");
    let location = location(&response);
    assert!(
        location.starts_with(&format!("{}/?", redirect_origin(&harness.callback_url))),
        "{location}"
    );
    assert!(!location.contains("evil.example"), "{location}");
    harness.stop().await;
}

#[tokio::test]
async fn provider_error_text_is_never_reflected() {
    let idp = start_idp().await;
    let directory = Arc::new(Directory::new(vec![alice()], "correct horse battery"));
    let harness = start_harness(&idp, directory, true).await;
    let (token, _) = new_device(&harness.pool, "Phone", "admin").await;
    let client = no_redirect_client();
    let authorization_url = start_flow(&client, &harness.base, &token).await;
    let response = callback(
        &client,
        &harness.base,
        &[
            ("error", "access_denied".to_string()),
            (
                "error_description",
                "SUPER-SECRET-PROVIDER-TEXT".to_string(),
            ),
            ("state", state_of(&authorization_url)),
        ],
    )
    .await;
    assert_reason(&response, "provider");
    let location = location(&response);
    assert!(!location.contains("SUPER-SECRET"), "{location}");
    harness.stop().await;
}

#[tokio::test]
async fn password_login_still_works_with_sso_configured() {
    let idp = start_idp().await;
    let directory = Arc::new(Directory::new(vec![alice()], "correct horse battery"));
    let harness = start_harness(&idp, directory, true).await;
    let (token, device_id) = new_device(&harness.pool, "Phone", "admin").await;
    let client = no_redirect_client();

    let response = client
        .post(format!("{}/api/login", harness.base))
        .bearer_auth(&token)
        .json(&json!({
            "username": "alice",
            "password": "correct horse battery",
        }))
        .send()
        .await
        .expect("login");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        account_name(&harness.pool, &device_id).await.as_deref(),
        Some("alice")
    );
    harness.stop().await;
}

#[test]
fn the_committed_signing_key_is_a_test_fixture() {
    let text = std::str::from_utf8(SSO_TEST_ONLY_KEY).expect("pem");
    assert!(text.starts_with("-----BEGIN PRIVATE KEY-----"));
}
