//! OIDC single sign-on for the paired-device remote surface.
//!
//! This module is deliberately narrow:
//!
//! * authorization code + PKCE S256 with a public client (no client secret);
//! * RS256 ID tokens only, verified against the provider's JWKS;
//! * an existing RAVENBOT account is still required; SSO never provisions one;
//! * HTTPS everywhere except loopback, so the test IdP can run on `127.0.0.1`;
//! * bounded network work: 10 s per request, 256 KiB per response and at most
//!   two redirects, each revalidated against the same URL rules.
//!
//! Authorization codes, access tokens and ID tokens are never persisted or
//! logged. A pending attempt stores only its state hash plus the nonce and PKCE
//! verifier that the callback needs, for ten minutes.

use std::net::{Ipv4Addr, Ipv6Addr};
use std::time::Duration;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use jsonwebtoken::jwk::{AlgorithmParameters, Jwk, JwkSet, KeyAlgorithm, PublicKeyUse};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use url::Url;

/// A pending flow lives for ten minutes: long enough for a password prompt and
/// a second factor, short enough that a stale browser tab cannot revive it.
pub const ATTEMPT_TTL_SECS: i64 = 10 * 60;
/// Keep a single device from filling the attempt table.
pub const MAX_ATTEMPTS_PER_DEVICE: i64 = 5;
/// And keep the whole host bounded.
pub const MAX_GLOBAL_ATTEMPTS: i64 = 500;
pub const MAX_RESPONSE_BYTES: usize = 256 * 1024;
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
pub const MAX_REDIRECTS: usize = 2;
/// OIDC deployments are allowed a small clock skew. The task caps it at 60 s.
pub const CLOCK_LEEWAY_SECS: u64 = 60;

const MAX_URL_CHARS: usize = 2048;
const MAX_CLIENT_ID_CHARS: usize = 256;
const MAX_SCOPES_CHARS: usize = 1024;
const MAX_ID_TOKEN_BYTES: usize = 64 * 1024;
const MAX_CODE_BYTES: usize = 8 * 1024;

/// Non-secret OIDC settings. The presence of this value means SSO is enabled;
/// none of it belongs in a keychain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SsoConfig {
    pub issuer: String,
    pub client_id: String,
    pub redirect_url: String,
    pub scopes: String,
    pub username_claim: String,
}

impl SsoConfig {
    pub const DEFAULT_SCOPES: &'static str = "openid profile email";
    pub const DEFAULT_USERNAME_CLAIM: &'static str = "preferred_username";

    /// Validate and normalize settings. The issuer is stored without a trailing
    /// slash and scopes are collapsed to single spaces.
    pub fn new(
        issuer: &str,
        client_id: &str,
        redirect_url: &str,
        scopes: &str,
        username_claim: &str,
    ) -> Result<Self, String> {
        let config = Self {
            issuer: issuer.trim().trim_end_matches('/').to_string(),
            client_id: client_id.trim().to_string(),
            redirect_url: redirect_url.trim().to_string(),
            scopes: normalize_scopes(scopes),
            username_claim: username_claim.trim().to_string(),
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), String> {
        validate_issuer(&self.issuer)?;
        validate_client_id(&self.client_id)?;
        validate_redirect_url(&self.redirect_url)?;
        validate_scopes(&self.scopes)?;
        validate_username_claim(&self.username_claim)?;
        Ok(())
    }

    /// The origin a post-callback browser redirect may use. It is derived from
    /// the configured redirect URL only, never from the request Host header.
    pub fn redirect_origin(&self) -> Result<String, String> {
        let url = validate_redirect_url(&self.redirect_url)?;
        Ok(url.origin().ascii_serialization())
    }

    pub fn discovery_url(&self) -> String {
        format!("{}/.well-known/openid-configuration", self.issuer)
    }
}

fn normalize_scopes(scopes: &str) -> String {
    scopes.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn validate_url_length(value: &str, label: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > MAX_URL_CHARS {
        return Err(format!("{label} must be 1–{MAX_URL_CHARS} characters."));
    }
    Ok(())
}

fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Ipv4(ip)) => ip == Ipv4Addr::LOCALHOST,
        Some(url::Host::Ipv6(ip)) => ip == Ipv6Addr::LOCALHOST,
        Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        _ => false,
    }
}

/// Parse a provider or redirect URL and enforce the shared transport rules.
/// Plain HTTP is allowed only for loopback so the loopback mock IdP works.
fn parse_secure_url(value: &str, label: &str) -> Result<Url, String> {
    validate_url_length(value, label)?;
    let url = Url::parse(value).map_err(|_| format!("{label} is not a valid URL."))?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(format!("{label} must not contain credentials."));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(format!("{label} must not contain a query or fragment."));
    }
    match url.scheme() {
        "https" => {}
        "http" if is_loopback(&url) => {}
        _ => {
            return Err(format!(
                "{label} must use HTTPS (plain HTTP is allowed only for loopback)."
            ))
        }
    }
    if url.host().is_none() {
        return Err(format!("{label} must name a host."));
    }
    Ok(url)
}

fn validate_issuer(value: &str) -> Result<Url, String> {
    if value.ends_with('/') {
        return Err("The issuer must not end with a slash.".to_string());
    }
    parse_secure_url(value, "The issuer")
}

fn validate_client_id(value: &str) -> Result<(), String> {
    let count = value.chars().count();
    if !(1..=MAX_CLIENT_ID_CHARS).contains(&count) {
        return Err(format!(
            "The client ID must be 1–{MAX_CLIENT_ID_CHARS} printable characters."
        ));
    }
    if !value.chars().all(|ch| ch.is_ascii_graphic()) {
        return Err("The client ID must contain printable ASCII characters only.".to_string());
    }
    Ok(())
}

fn validate_redirect_url(value: &str) -> Result<Url, String> {
    let url = parse_secure_url(value, "The redirect URL")?;
    if !url.path().ends_with("/api/sso/callback") {
        return Err("The redirect URL must end in /api/sso/callback.".to_string());
    }
    Ok(url)
}

fn validate_scopes(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > MAX_SCOPES_CHARS {
        return Err(format!(
            "Scopes must be 1–{MAX_SCOPES_CHARS} characters and include openid."
        ));
    }
    let scopes: Vec<&str> = value.split_whitespace().collect();
    if !scopes.contains(&"openid") {
        return Err("Scopes must include openid.".to_string());
    }
    for scope in scopes {
        // RFC 6749 scope-token: %x21 / %x23-5B / %x5D-7E
        if !scope
            .chars()
            .all(|ch| ch == '!' || ('#'..='[').contains(&ch) || (']'..='~').contains(&ch))
        {
            return Err("A scope contains an invalid character.".to_string());
        }
    }
    Ok(())
}

fn validate_username_claim(value: &str) -> Result<(), String> {
    let count = value.chars().count();
    if !(1..=64).contains(&count) {
        return Err("The username claim must be 1–64 characters.".to_string());
    }
    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | ':' | '-'))
    {
        return Err(
            "The username claim may contain letters, digits, '_', '.', ':' and '-'.".to_string(),
        );
    }
    Ok(())
}

/// Validate a discovered endpoint. It must be absolute, HTTPS (or loopback
/// HTTP), and carry no credentials, query or fragment of its own.
pub(crate) fn validate_endpoint_url(value: &str) -> Result<Url, String> {
    parse_secure_url(value, "The provider endpoint")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SsoFailure {
    State,
    Provider,
    Exchange,
    Token,
    Account,
    Config,
}

impl SsoFailure {
    pub(crate) fn reason(self) -> &'static str {
        match self {
            Self::State => "state",
            Self::Provider => "provider",
            Self::Exchange => "exchange",
            Self::Token => "token",
            Self::Account => "account",
            Self::Config => "config",
        }
    }

    /// Authentication failures are 401; host/provider failures are 500.
    pub(crate) fn audit_status(self) -> i64 {
        match self {
            Self::State | Self::Provider | Self::Token | Self::Account => 401,
            Self::Exchange | Self::Config => 500,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SsoSecrets {
    pub state: String,
    pub nonce: String,
    pub verifier: String,
    pub challenge: String,
}

fn random_hex(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    OsRng.fill_bytes(&mut buffer);
    buffer.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// 32 random bytes for state and nonce and a 64-character verifier. All are
/// inside the RFC 7636 verifier alphabet and length range.
pub(crate) fn generate_secrets() -> SsoSecrets {
    let state = random_hex(32);
    let nonce = random_hex(32);
    let verifier = random_hex(32);
    let challenge = pkce_challenge(&verifier);
    SsoSecrets {
        state,
        nonce,
        verifier,
        challenge,
    }
}

#[derive(Debug, Clone, Deserialize)]
struct DiscoveryDocument {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    jwks_uri: String,
    #[serde(default)]
    response_types_supported: Option<Vec<String>>,
    #[serde(default)]
    grant_types_supported: Option<Vec<String>>,
    #[serde(default)]
    code_challenge_methods_supported: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub(crate) struct DiscoveredProvider {
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub jwks_uri: String,
}

pub(crate) fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= MAX_REDIRECTS {
                return attempt.error(std::io::Error::other("too many redirects"));
            }
            if validate_endpoint_url(attempt.url().as_str()).is_err() {
                return attempt.error(std::io::Error::other("unsafe redirect"));
            }
            attempt.follow()
        }))
        .build()
        .map_err(|_| "Cannot create the SSO HTTP client.".to_string())
}

async fn read_json(
    mut response: reqwest::Response,
    failure: SsoFailure,
) -> Result<Value, SsoFailure> {
    if !response.status().is_success() {
        return Err(failure);
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err(failure);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| failure)? {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(failure);
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body).map_err(|_| failure)
}

pub(crate) async fn discover(
    client: &reqwest::Client,
    config: &SsoConfig,
) -> Result<DiscoveredProvider, SsoFailure> {
    let url = Url::parse(&config.discovery_url()).map_err(|_| SsoFailure::Config)?;
    validate_endpoint_url(url.as_str()).map_err(|_| SsoFailure::Config)?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|_| SsoFailure::Config)?;
    let value = read_json(response, SsoFailure::Config).await?;
    let document: DiscoveryDocument =
        serde_json::from_value(value).map_err(|_| SsoFailure::Config)?;
    if document.issuer != config.issuer {
        return Err(SsoFailure::Config);
    }
    if document
        .response_types_supported
        .as_ref()
        .is_some_and(|types| !types.iter().any(|value| value == "code"))
    {
        return Err(SsoFailure::Config);
    }
    if document
        .grant_types_supported
        .as_ref()
        .is_some_and(|types| !types.iter().any(|value| value == "authorization_code"))
    {
        return Err(SsoFailure::Config);
    }
    if document
        .code_challenge_methods_supported
        .as_ref()
        .is_some_and(|methods| !methods.iter().any(|value| value == "S256"))
    {
        return Err(SsoFailure::Config);
    }
    validate_endpoint_url(&document.authorization_endpoint).map_err(|_| SsoFailure::Config)?;
    validate_endpoint_url(&document.token_endpoint).map_err(|_| SsoFailure::Config)?;
    validate_endpoint_url(&document.jwks_uri).map_err(|_| SsoFailure::Config)?;
    Ok(DiscoveredProvider {
        authorization_endpoint: document.authorization_endpoint,
        token_endpoint: document.token_endpoint,
        jwks_uri: document.jwks_uri,
    })
}

pub(crate) fn authorization_url(
    config: &SsoConfig,
    provider: &DiscoveredProvider,
    secrets: &SsoSecrets,
) -> Result<String, SsoFailure> {
    let mut url = Url::parse(&provider.authorization_endpoint).map_err(|_| SsoFailure::Config)?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &config.client_id)
        .append_pair("redirect_uri", &config.redirect_url)
        .append_pair("scope", &config.scopes)
        .append_pair("state", &secrets.state)
        .append_pair("nonce", &secrets.nonce)
        .append_pair("code_challenge", &secrets.challenge)
        .append_pair("code_challenge_method", "S256");
    Ok(url.to_string())
}

/// Exchange the authorization code. Only the ID token is returned; an access
/// token in the response is ignored and never persisted.
pub(crate) async fn exchange_code(
    client: &reqwest::Client,
    token_endpoint: &str,
    client_id: &str,
    redirect_url: &str,
    verifier: &str,
    code: &str,
) -> Result<String, SsoFailure> {
    if code.is_empty() || code.len() > MAX_CODE_BYTES {
        return Err(SsoFailure::Exchange);
    }
    let url = validate_endpoint_url(token_endpoint).map_err(|_| SsoFailure::Exchange)?;
    let response = client
        .post(url)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_url),
            ("client_id", client_id),
            ("code_verifier", verifier),
        ])
        .send()
        .await
        .map_err(|_| SsoFailure::Exchange)?;
    let value = read_json(response, SsoFailure::Exchange).await?;
    let id_token = value
        .get("id_token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty() && token.len() <= MAX_ID_TOKEN_BYTES)
        .ok_or(SsoFailure::Exchange)?;
    Ok(id_token.to_string())
}

fn signing_keys(jwks: &JwkSet) -> Vec<&Jwk> {
    jwks.keys
        .iter()
        .filter(|jwk| matches!(&jwk.algorithm, AlgorithmParameters::RSA(_)))
        .filter(|jwk| {
            jwk.common
                .key_algorithm
                .is_none_or(|algorithm| algorithm == KeyAlgorithm::RS256)
        })
        .filter(|jwk| match &jwk.common.public_key_use {
            None => true,
            Some(PublicKeyUse::Signature) => true,
            _ => false,
        })
        .collect()
}

fn select_key<'a>(keys: &'a [&'a Jwk], kid: Option<&str>) -> Result<&'a Jwk, SsoFailure> {
    match kid {
        Some(kid) => keys
            .iter()
            .copied()
            .find(|jwk| jwk.common.key_id.as_deref() == Some(kid))
            .ok_or(SsoFailure::Token),
        None if keys.len() == 1 => Ok(keys[0]),
        None => Err(SsoFailure::Token),
    }
}

/// Verify an RS256 ID token and return the configured identity claim.
///
/// `none`, HMAC and every non-RS256 algorithm are rejected before any key is
/// used. The signature, issuer, audience, expiry, optional nbf and nonce are
/// checked; `sub` is required.
pub(crate) async fn verify_id_token(
    client: &reqwest::Client,
    jwks_uri: &str,
    token: &str,
    issuer: &str,
    client_id: &str,
    nonce: &str,
    username_claim: &str,
) -> Result<String, SsoFailure> {
    if token.is_empty() || token.len() > MAX_ID_TOKEN_BYTES {
        return Err(SsoFailure::Token);
    }
    let header = decode_header(token).map_err(|_| SsoFailure::Token)?;
    if header.alg != Algorithm::RS256 {
        return Err(SsoFailure::Token);
    }
    let url = validate_endpoint_url(jwks_uri).map_err(|_| SsoFailure::Token)?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|_| SsoFailure::Token)?;
    let value = read_json(response, SsoFailure::Token).await?;
    let jwks: JwkSet = serde_json::from_value(value).map_err(|_| SsoFailure::Token)?;
    let keys = signing_keys(&jwks);
    let jwk = select_key(&keys, header.kid.as_deref())?;
    let decoding_key = DecodingKey::from_jwk(jwk).map_err(|_| SsoFailure::Token)?;

    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[issuer]);
    validation.set_audience(&[client_id]);
    validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
    validation.validate_exp = true;
    validation.validate_nbf = true;
    validation.leeway = CLOCK_LEEWAY_SECS;
    let claims = decode::<Value>(token, &decoding_key, &validation)
        .map_err(|_| SsoFailure::Token)?
        .claims;

    if claims.get("iss").and_then(Value::as_str) != Some(issuer) {
        return Err(SsoFailure::Token);
    }
    // `azp` is optional, but when an IdP sends it, it names the client the
    // token was issued to. A token minted for another client must not become a
    // RAVENBOT session merely because our client id also appears in `aud`.
    if claims
        .get("azp")
        .and_then(Value::as_str)
        .is_some_and(|authorized_party| authorized_party != client_id)
    {
        return Err(SsoFailure::Token);
    }
    if claims
        .get("sub")
        .and_then(Value::as_str)
        .is_none_or(|sub| sub.trim().is_empty())
    {
        return Err(SsoFailure::Token);
    }
    let token_nonce = claims
        .get("nonce")
        .and_then(Value::as_str)
        .ok_or(SsoFailure::Token)?;
    if token_nonce.is_empty() || token_nonce != nonce {
        return Err(SsoFailure::Token);
    }
    claim_identity(&claims, username_claim)
}

fn claim_identity(claims: &Value, claim: &str) -> Result<String, SsoFailure> {
    claims
        .get(claim)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or(SsoFailure::Account)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_valid_https_config_is_normalized() {
        let config = SsoConfig::new(
            "  https://idp.example.com/tenant/  ",
            " ravenbot-remote ",
            "https://bot.example.com/api/sso/callback",
            "  openid   profile  email ",
            " preferred_username ",
        )
        .expect("config");
        assert_eq!(config.issuer, "https://idp.example.com/tenant");
        assert_eq!(config.client_id, "ravenbot-remote");
        assert_eq!(config.scopes, "openid profile email");
        assert_eq!(config.username_claim, "preferred_username");
        assert_eq!(
            config.discovery_url(),
            "https://idp.example.com/tenant/.well-known/openid-configuration"
        );
        assert_eq!(
            config.redirect_origin().expect("origin"),
            "https://bot.example.com"
        );
    }

    #[test]
    fn loopback_http_is_allowed_but_remote_http_is_not() {
        for issuer in [
            "http://127.0.0.1:9000",
            "http://localhost:9000",
            "http://[::1]:9000",
        ] {
            SsoConfig::new(
                issuer,
                "client",
                "http://127.0.0.1:8790/api/sso/callback",
                "openid",
                "preferred_username",
            )
            .unwrap_or_else(|error| panic!("{issuer}: {error}"));
        }
        assert!(SsoConfig::new(
            "http://idp.example.com",
            "client",
            "http://127.0.0.1:8790/api/sso/callback",
            "openid",
            "preferred_username",
        )
        .is_err());
        assert!(SsoConfig::new(
            "https://idp.example.com",
            "client",
            "http://bot.example.com/api/sso/callback",
            "openid",
            "preferred_username",
        )
        .is_err());
    }

    #[test]
    fn credentials_queries_and_fragments_are_rejected() {
        for issuer in [
            "https://user:pass@idp.example.com",
            "https://idp.example.com?x=1",
            "https://idp.example.com#fragment",
        ] {
            assert!(
                SsoConfig::new(
                    issuer,
                    "client",
                    "https://bot.example.com/api/sso/callback",
                    "openid",
                    "preferred_username"
                )
                .is_err(),
                "{issuer}"
            );
        }
        assert!(validate_endpoint_url("https://user@idp.example.com/token").is_err());
        assert!(validate_endpoint_url("https://idp.example.com/token?x=1").is_err());
        assert!(validate_endpoint_url("https://idp.example.com/token#frag").is_err());
        assert!(validate_endpoint_url("http://idp.example.com/token").is_err());
        assert!(validate_endpoint_url("http://127.0.0.1:9000/token").is_ok());
    }

    #[test]
    fn redirect_client_scopes_and_claim_are_validated() {
        for redirect in [
            "https://bot.example.com/callback",
            "https://bot.example.com/api/sso/callback?x=1",
            "http://bot.example.com/api/sso/callback",
        ] {
            assert!(
                SsoConfig::new(
                    "https://idp.example.com",
                    "client",
                    redirect,
                    "openid",
                    "preferred_username"
                )
                .is_err(),
                "{redirect}"
            );
        }
        assert!(SsoConfig::new(
            "https://idp.example.com",
            "",
            "https://bot.example.com/api/sso/callback",
            "openid",
            "preferred_username"
        )
        .is_err());
        let long_client_id = "x".repeat(257);
        assert!(SsoConfig::new(
            "https://idp.example.com",
            &long_client_id,
            "https://bot.example.com/api/sso/callback",
            "openid",
            "preferred_username"
        )
        .is_err());
        assert!(SsoConfig::new(
            "https://idp.example.com",
            "client",
            "https://bot.example.com/api/sso/callback",
            "profile email",
            "preferred_username"
        )
        .is_err());
        let long_claim = "x".repeat(65);
        for claim in ["", "bad claim", "a b", long_claim.as_str()] {
            assert!(
                SsoConfig::new(
                    "https://idp.example.com",
                    "client",
                    "https://bot.example.com/api/sso/callback",
                    "openid",
                    claim
                )
                .is_err(),
                "{claim}"
            );
        }
        assert!(SsoConfig::new(
            "https://idp.example.com",
            "client",
            "https://bot.example.com/api/sso/callback",
            "openid",
            "email"
        )
        .is_ok());
    }

    #[test]
    fn generated_secrets_are_high_entropy_and_the_challenge_matches() {
        let first = generate_secrets();
        let second = generate_secrets();
        assert_eq!(first.state.len(), 64);
        assert_eq!(first.nonce.len(), 64);
        assert_eq!(first.verifier.len(), 64);
        assert!((43..=128).contains(&first.verifier.len()));
        assert!(first
            .verifier
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '.' | '_' | '~')));
        assert_eq!(
            first.challenge,
            URL_SAFE_NO_PAD.encode(Sha256::digest(first.verifier.as_bytes()))
        );
        assert_eq!(first.challenge.len(), 43);
        assert!(!first.challenge.contains('='));
        assert_ne!(first.state, second.state);
        assert_ne!(first.nonce, second.nonce);
        assert_ne!(first.verifier, second.verifier);
    }

    #[test]
    fn claim_extraction_accepts_only_a_non_empty_string() {
        let claims = serde_json::json!({
            "preferred_username": "  Alice  ",
            "email": "alice@example.com",
            "groups": ["admins"],
            "empty": "   ",
            "number": 42,
        });
        assert_eq!(
            claim_identity(&claims, "preferred_username").expect("claim"),
            "Alice"
        );
        assert_eq!(
            claim_identity(&claims, "email").expect("claim"),
            "alice@example.com"
        );
        for claim in ["missing", "groups", "empty", "number"] {
            assert_eq!(
                claim_identity(&claims, claim).unwrap_err(),
                SsoFailure::Account,
                "{claim}"
            );
        }
    }

    #[test]
    fn signing_key_selection_rejects_unusable_keys() {
        let jwks: JwkSet = serde_json::from_value(serde_json::json!({
            "keys": [
                { "kty": "oct", "k": "c2VjcmV0", "kid": "hmac", "alg": "HS256" },
                { "kty": "EC", "crv": "P-256", "x": "a", "y": "b", "kid": "ec", "alg": "ES256" },
                { "kty": "RSA", "n": "AQAB", "e": "AQAB", "kid": "rsa", "alg": "RS256" }
            ]
        }))
        .expect("jwks");
        let keys = signing_keys(&jwks);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].common.key_id.as_deref(), Some("rsa"));
        assert!(select_key(&keys, Some("hmac")).is_err());
        assert!(select_key(&keys, Some("rsa")).is_ok());
        assert!(select_key(&keys, None).is_ok());
    }
}
