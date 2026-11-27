//! SCIM 2.0 user provisioning for the remote listener.
//!
//! This is a deliberately small server-side surface:
//!
//! * off by default and authenticated with one workspace-scoped bearer token;
//! * only the core `User` resource, and only list/get/create/patch/put/delete;
//! * no Groups, bulk, sort, password, email or group attributes;
//! * every account operation goes through `AccountProvisioner`, which proves a
//!   grant on this workspace before exposing or changing an account;
//! * the workspace owner is never SCIM-managed and is excluded from lists;
//! * deleting a user is a soft deprovision (`active: false`), not a purge.
//!
//! Responses use `application/scim+json`. Errors never include SQL or provider
//! detail: an internal failure is logged and returned as generic copy.

use std::net::IpAddr;
use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::{ConnectInfo, OriginalUri, Path, Query, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Map, Value};

use ravenbot_db::queries::{AppSettingsQueries, RemoteAccessQueries};

use super::{
    bearer_token, hash_secret, AccountProvisioner, PeerAddr, ProvisionError, RemoteState,
    ScimConfig, ScimFilterField, ScimUser, SCIM_MAX_RESULTS,
};

const SCIM_CONTENT_TYPE: &str = "application/scim+json";
const USER_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:User";
const LIST_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:ListResponse";
const ERROR_SCHEMA: &str = "urn:ietf:params:scim:api:messages:2.0:Error";
const SERVICE_PROVIDER_CONFIG_SCHEMA: &str =
    "urn:ietf:params:scim:schemas:core:2.0:ServiceProviderConfig";
const RESOURCE_TYPE_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:ResourceType";

/// Mount the SCIM routes with their own bearer middleware. They must never pass
/// through the paired-device middleware: a SCIM token is not a device token.
pub(super) fn router(state: Arc<RemoteState>) -> Router<Arc<RemoteState>> {
    Router::new()
        .route(
            "/scim/v2/ServiceProviderConfig",
            get(service_provider_config),
        )
        .route("/scim/v2/ResourceTypes", get(resource_types))
        .route("/scim/v2/ResourceTypes/", get(resource_types))
        .route("/scim/v2/Users", get(list_users).post(create_user))
        .route("/scim/v2/Users/", get(list_users).post(create_user))
        .route(
            "/scim/v2/Users/{id}",
            get(get_user)
                .patch(patch_user)
                .put(put_user)
                .delete(delete_user),
        )
        .route_layer(axum::middleware::from_fn_with_state(state, authenticate))
}

fn scim_json(status: StatusCode, body: Value) -> Response {
    let mut response = (status, Json(body)).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(SCIM_CONTENT_TYPE),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn scim_error(status: StatusCode, scim_type: Option<&str>, detail: &str) -> Response {
    let mut body = json!({
        "schemas": [ERROR_SCHEMA],
        "status": status.as_u16().to_string(),
        "detail": detail,
    });
    if let Some(scim_type) = scim_type {
        body["scimType"] = json!(scim_type);
    }
    scim_json(status, body)
}

fn unauthorized() -> Response {
    let mut response = scim_error(
        StatusCode::UNAUTHORIZED,
        None,
        "A SCIM bearer token is required.",
    );
    response
        .headers_mut()
        .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
    response
}

fn not_found() -> Response {
    scim_error(StatusCode::NOT_FOUND, None, "The user does not exist.")
}

fn invalid_value() -> Response {
    scim_error(
        StatusCode::BAD_REQUEST,
        Some("invalidValue"),
        "The request is not valid SCIM.",
    )
}

fn internal_error() -> Response {
    scim_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        None,
        "The local RAVENBOT service could not complete that request.",
    )
}

fn provision_failure(error: ProvisionError) -> Response {
    match error {
        ProvisionError::Conflict(_) => scim_error(
            StatusCode::CONFLICT,
            Some("uniqueness"),
            "A user with that name or external id already exists.",
        ),
        ProvisionError::Invalid(_) => invalid_value(),
        ProvisionError::Forbidden(_) => scim_error(
            StatusCode::FORBIDDEN,
            Some("mutability"),
            "The workspace owner and shared accounts cannot be changed by SCIM.",
        ),
        ProvisionError::Internal(message) => {
            tracing::error!(%message, "SCIM provisioner failed");
            internal_error()
        }
    }
}

fn read_failure(context: &'static str, error: String) -> Response {
    tracing::error!(%error, context, "SCIM registry read failed");
    internal_error()
}

fn hashes_equal(left: &str, right: &str) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.bytes()
        .zip(right.bytes())
        .fold(0u8, |difference, (left, right)| difference | (left ^ right))
        == 0
}

/// A SCIM request only reaches a handler with a valid token and a loaded
/// configuration. Rate limiting happens before any database work.
async fn authenticate(
    State(state): State<Arc<RemoteState>>,
    ConnectInfo(peer): ConnectInfo<PeerAddr>,
    request: Request,
    next: Next,
) -> Response {
    let Some(config) = state.scim.clone() else {
        return scim_error(
            StatusCode::NOT_FOUND,
            None,
            "SCIM provisioning is not enabled.",
        );
    };
    let ip: IpAddr = peer.0.ip();
    if !state.scim_allowed(ip) {
        return scim_error(
            StatusCode::TOO_MANY_REQUESTS,
            None,
            "Too many SCIM requests. Retry in a minute.",
        );
    }
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let authenticated = bearer_token(&request)
        .as_deref()
        .map(hash_secret)
        .is_some_and(|hash| hashes_equal(&hash, &config.token_hash));
    if !authenticated {
        let _ = RemoteAccessQueries::audit(
            &state.pool,
            None,
            Some("SCIM"),
            None,
            None,
            method.as_str(),
            &path,
            401,
        )
        .await;
        return unauthorized();
    }
    // This is the only state a SCIM request writes outside its own mutation.
    // Reads are not audited, but the owner still sees that the token is live.
    if let Err(error) = AppSettingsQueries::set(
        &state.pool,
        "remote_scim_last_used_at",
        &Utc::now().to_rfc3339(),
    )
    .await
    {
        tracing::warn!(%error, "Could not update the SCIM last-used timestamp");
    }
    next.run(request).await
}

fn provisioner(state: &RemoteState) -> Result<Arc<dyn AccountProvisioner>, Response> {
    state.provisioner.clone().ok_or_else(|| {
        scim_error(
            StatusCode::NOT_FOUND,
            None,
            "SCIM provisioning is not enabled.",
        )
    })
}

async fn audit(
    state: &RemoteState,
    method: &str,
    path: &str,
    user_name: Option<&str>,
    status: StatusCode,
) {
    let _ = RemoteAccessQueries::audit(
        &state.pool,
        None,
        Some("SCIM"),
        None,
        user_name,
        method,
        path,
        status.as_u16() as i64,
    )
    .await;
}

fn user_json(user: &ScimUser) -> Value {
    let mut body = json!({
        "schemas": [USER_SCHEMA],
        "id": user.id,
        "userName": user.user_name,
        "active": user.active,
        "meta": {
            "resourceType": "User",
            "created": user.created_at,
            "lastModified": user.created_at,
        },
    });
    if let Some(external_id) = &user.external_id {
        body["externalId"] = json!(external_id);
    }
    body
}

// ─────────────────────────────────────────────────────────────────────────────
// Filter and query parsing
// ─────────────────────────────────────────────────────────────────────────────

/// Parse only `userName eq "..."`, `externalId eq "..."` and `id eq "..."`.
/// Field names and `eq` are case-insensitive; the quoted value supports `\"`
/// and `\\` escapes and nothing else.
fn parse_filter(raw: &str) -> Result<(ScimFilterField, String), ()> {
    let raw = raw.trim();
    let field_end = raw.find(char::is_whitespace).ok_or(())?;
    let field = &raw[..field_end];
    let mut rest = raw[field_end..].trim_start();
    if !rest
        .get(..2)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("eq"))
    {
        return Err(());
    }
    if rest
        .get(2..3)
        .is_some_and(|next| !next.chars().all(char::is_whitespace))
    {
        return Err(());
    }
    rest = rest[2..].trim_start();
    if !rest.starts_with('"') {
        return Err(());
    }
    let mut value = String::new();
    let mut escaped = false;
    let mut closed = None;
    for (index, ch) in rest.char_indices().skip(1) {
        if escaped {
            match ch {
                '"' | '\\' => value.push(ch),
                _ => return Err(()),
            }
            escaped = false;
        } else {
            match ch {
                '\\' => escaped = true,
                '"' => {
                    closed = Some(index);
                    break;
                }
                _ => value.push(ch),
            }
        }
    }
    if escaped {
        return Err(());
    }
    let closed = closed.ok_or(())?;
    if !rest[closed + 1..].trim().is_empty() {
        return Err(());
    }
    let field = match field.to_ascii_lowercase().as_str() {
        "id" => ScimFilterField::Id,
        "username" => ScimFilterField::UserName,
        "externalid" => ScimFilterField::ExternalId,
        _ => return Err(()),
    };
    Ok((field, value))
}

#[derive(Debug, Default, Deserialize)]
struct ListQuery {
    filter: Option<String>,
    #[serde(rename = "startIndex")]
    start_index: Option<String>,
    count: Option<String>,
}

fn parse_non_negative(value: &str) -> Option<usize> {
    value.trim().parse::<usize>().ok()
}

fn list_response(users: Vec<ScimUser>, start_index: usize, count: usize) -> Response {
    let total = users.len();
    let offset = start_index.saturating_sub(1);
    let page: Vec<Value> = users
        .into_iter()
        .skip(offset)
        .take(count)
        .map(|user| user_json(&user))
        .collect();
    scim_json(
        StatusCode::OK,
        json!({
            "schemas": [LIST_SCHEMA],
            "totalResults": total,
            "startIndex": start_index,
            "itemsPerPage": page.len(),
            "Resources": page,
        }),
    )
}

/// SCIM discovery is static capability metadata. It is authenticated by the
/// same workspace token but deliberately does not consult the provisioner.
async fn service_provider_config() -> Response {
    scim_json(
        StatusCode::OK,
        json!({
            "schemas": [SERVICE_PROVIDER_CONFIG_SCHEMA],
            "patch": { "supported": true },
            "bulk": {
                "supported": false,
                "maxOperations": 0,
                "maxPayloadSize": 0,
            },
            "filter": {
                "supported": true,
                "maxResults": SCIM_MAX_RESULTS,
            },
            "changePassword": { "supported": false },
            "sort": { "supported": false },
            "etag": { "supported": false },
            "authenticationSchemes": [{
                "type": "oauthbearertoken",
                "name": "Bearer token",
                "description": "Workspace-scoped SCIM bearer token",
                "primary": true,
            }],
        }),
    )
}

/// The only resource type this server implements. Groups and extensions are
/// deliberately absent.
async fn resource_types() -> Response {
    let resource = json!({
        "schemas": [RESOURCE_TYPE_SCHEMA],
        "id": "User",
        "name": "User",
        "endpoint": "/scim/v2/Users",
        "schema": USER_SCHEMA,
        "meta": { "resourceType": "ResourceType" },
    });
    scim_json(
        StatusCode::OK,
        json!({
            "schemas": [LIST_SCHEMA],
            "totalResults": 1,
            "startIndex": 1,
            "itemsPerPage": 1,
            "Resources": [resource],
        }),
    )
}

async fn list_users(
    State(state): State<Arc<RemoteState>>,
    Query(query): Query<ListQuery>,
) -> Response {
    let start_index = match query.start_index.as_deref() {
        None => 1,
        Some(value) => match parse_non_negative(value) {
            Some(value) if value >= 1 => value,
            _ => return invalid_value(),
        },
    };
    let count = match query.count.as_deref() {
        None => 100,
        Some(value) => match parse_non_negative(value) {
            Some(value) if value <= SCIM_MAX_RESULTS => value,
            _ => return invalid_value(),
        },
    };
    let provisioner = match provisioner(&state) {
        Ok(provisioner) => provisioner,
        Err(response) => return response,
    };
    let filter = match query.filter.as_deref() {
        None => None,
        Some(raw) => match parse_filter(raw) {
            Ok((field, value)) => Some((field, value)),
            Err(()) => {
                return scim_error(
                    StatusCode::BAD_REQUEST,
                    Some("invalidFilter"),
                    "Only userName eq, externalId eq and id eq filters are supported.",
                )
            }
        },
    };
    let users = match filter {
        Some((field, value)) => match provisioner.find_user(field, &value).await {
            Ok(user) => user.into_iter().collect(),
            Err(error) => return read_failure("SCIM filter", error),
        },
        None => match provisioner.list_users().await {
            Ok(users) => users,
            Err(error) => return read_failure("SCIM list", error),
        },
    };
    list_response(users, start_index, count)
}

// ─────────────────────────────────────────────────────────────────────────────
// Body parsing
// ─────────────────────────────────────────────────────────────────────────────

fn json_body(payload: Result<Json<Value>, JsonRejection>) -> Result<Value, Response> {
    match payload {
        Ok(Json(value)) if value.is_object() => Ok(value),
        Ok(_) => Err(invalid_value()),
        Err(rejection) => {
            let status = rejection.status();
            Err(scim_error(
                status,
                Some("invalidValue"),
                "The request body is not valid SCIM JSON.",
            ))
        }
    }
}

fn required_user_name(body: &Value) -> Result<String, Response> {
    body.get("userName")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(invalid_value)
}

fn optional_external_id(body: &Value) -> Result<Option<String>, Response> {
    match body.get("externalId") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => {
            let value = value.trim();
            Ok((!value.is_empty()).then(|| value.to_string()))
        }
        Some(_) => Err(invalid_value()),
    }
}

fn active_value(body: &Value) -> Result<bool, Response> {
    match body.get("active") {
        None => Ok(true),
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => Err(invalid_value()),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// User handlers
// ─────────────────────────────────────────────────────────────────────────────

async fn create_user(
    State(state): State<Arc<RemoteState>>,
    OriginalUri(uri): OriginalUri,
    payload: Result<Json<Value>, JsonRejection>,
) -> Response {
    let body = match json_body(payload) {
        Ok(body) => body,
        Err(response) => {
            audit(&state, "SCIM POST", uri.path(), None, response.status()).await;
            return response;
        }
    };
    let target = body
        .get("userName")
        .and_then(Value::as_str)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let user_name = match required_user_name(&body) {
        Ok(user_name) => user_name,
        Err(response) => {
            audit(
                &state,
                "SCIM POST",
                uri.path(),
                target.as_deref(),
                response.status(),
            )
            .await;
            return response;
        }
    };
    let external_id = match optional_external_id(&body) {
        Ok(external_id) => external_id,
        Err(response) => {
            audit(
                &state,
                "SCIM POST",
                uri.path(),
                Some(&user_name),
                response.status(),
            )
            .await;
            return response;
        }
    };
    let active = match active_value(&body) {
        Ok(active) => active,
        Err(response) => {
            audit(
                &state,
                "SCIM POST",
                uri.path(),
                Some(&user_name),
                response.status(),
            )
            .await;
            return response;
        }
    };
    let provisioner = match provisioner(&state) {
        Ok(provisioner) => provisioner,
        Err(response) => return response,
    };
    let role = state
        .scim
        .as_ref()
        .map(ScimConfig::default_role)
        .unwrap_or(ScimConfig::DEFAULT_ROLE)
        .to_string();
    let result = provisioner
        .create_user(&user_name, external_id.as_deref(), active, &role)
        .await;
    let response = match result {
        Ok(user) => {
            let mut response = scim_json(StatusCode::CREATED, user_json(&user));
            if let Ok(location) = HeaderValue::from_str(&format!("/scim/v2/Users/{}", user.id)) {
                response.headers_mut().insert(header::LOCATION, location);
            }
            response
        }
        Err(error) => provision_failure(error),
    };
    audit(
        &state,
        "SCIM POST",
        uri.path(),
        Some(&user_name),
        response.status(),
    )
    .await;
    response
}

async fn get_user_handler(state: &RemoteState, id: &str) -> Result<ScimUser, Response> {
    let provisioner = provisioner(state)?;
    match provisioner.get_user(id).await {
        Ok(Some(user)) => Ok(user),
        Ok(None) => Err(not_found()),
        Err(error) => Err(read_failure("SCIM get", error)),
    }
}

async fn get_user(State(state): State<Arc<RemoteState>>, Path(id): Path<String>) -> Response {
    match get_user_handler(&state, &id).await {
        Ok(user) => scim_json(StatusCode::OK, user_json(&user)),
        Err(response) => response,
    }
}

#[derive(Debug)]
struct PatchChanges {
    active: Option<bool>,
    user_name: Option<String>,
}

fn patch_string(value: Option<&Value>) -> Result<String, Response> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(invalid_value)
}

fn apply_patch_object(
    changes: &mut PatchChanges,
    object: &Map<String, Value>,
    is_add: bool,
) -> Result<(), Response> {
    for (key, value) in object {
        if key.eq_ignore_ascii_case("active") {
            match value {
                Value::Bool(active) => changes.active = Some(*active),
                _ => return Err(invalid_value()),
            }
        } else if key.eq_ignore_ascii_case("userName") {
            if is_add {
                return Err(scim_error(
                    StatusCode::BAD_REQUEST,
                    Some("invalidPath"),
                    "add is only supported for active.",
                ));
            }
            changes.user_name = Some(patch_string(Some(value))?);
        } else {
            return Err(scim_error(
                StatusCode::BAD_REQUEST,
                Some("invalidPath"),
                "Only active and userName can be changed by SCIM.",
            ));
        }
    }
    Ok(())
}

fn parse_patch(body: &Value) -> Result<PatchChanges, Response> {
    let operations = body
        .get("Operations")
        .and_then(Value::as_array)
        .ok_or_else(invalid_value)?;
    let mut changes = PatchChanges {
        active: None,
        user_name: None,
    };
    for operation in operations {
        let object = operation.as_object().ok_or_else(invalid_value)?;
        let op = object
            .get("op")
            .and_then(Value::as_str)
            .map(|value| value.trim().to_ascii_lowercase())
            .ok_or_else(invalid_value)?;
        if op != "replace" && op != "add" {
            return Err(scim_error(
                StatusCode::BAD_REQUEST,
                Some("invalidPath"),
                "Only replace is supported (add is accepted for active).",
            ));
        }
        let is_add = op == "add";
        match object.get("path").and_then(Value::as_str) {
            Some(path) => {
                if path.eq_ignore_ascii_case("active") {
                    match object.get("value") {
                        Some(Value::Bool(active)) => changes.active = Some(*active),
                        _ => return Err(invalid_value()),
                    }
                } else if path.eq_ignore_ascii_case("userName") {
                    if is_add {
                        return Err(scim_error(
                            StatusCode::BAD_REQUEST,
                            Some("invalidPath"),
                            "add is only supported for active.",
                        ));
                    }
                    changes.user_name = Some(patch_string(object.get("value"))?);
                } else {
                    return Err(scim_error(
                        StatusCode::BAD_REQUEST,
                        Some("invalidPath"),
                        "Only active and userName can be changed by SCIM.",
                    ));
                }
            }
            None => {
                let value = object.get("value").ok_or_else(invalid_value)?;
                let value = value.as_object().ok_or_else(invalid_value)?;
                apply_patch_object(&mut changes, value, is_add)?;
            }
        }
    }
    Ok(changes)
}

async fn apply_changes(
    provisioner: Arc<dyn AccountProvisioner>,
    user: ScimUser,
    changes: PatchChanges,
) -> Result<ScimUser, Response> {
    let mut current = user;
    if let Some(user_name) = changes.user_name {
        current = provisioner
            .rename(&current.id, &user_name)
            .await
            .map_err(provision_failure)?;
    }
    if let Some(active) = changes.active {
        current = provisioner
            .set_active(&current.id, active)
            .await
            .map_err(provision_failure)?;
    }
    Ok(current)
}

async fn patch_user(
    State(state): State<Arc<RemoteState>>,
    OriginalUri(uri): OriginalUri,
    Path(id): Path<String>,
    payload: Result<Json<Value>, JsonRejection>,
) -> Response {
    let user = match get_user_handler(&state, &id).await {
        Ok(user) => user,
        Err(response) => {
            audit(&state, "SCIM PATCH", uri.path(), None, response.status()).await;
            return response;
        }
    };
    let body = match json_body(payload) {
        Ok(body) => body,
        Err(response) => {
            audit(
                &state,
                "SCIM PATCH",
                uri.path(),
                Some(&user.user_name),
                response.status(),
            )
            .await;
            return response;
        }
    };
    let changes = match parse_patch(&body) {
        Ok(changes) => changes,
        Err(response) => {
            audit(
                &state,
                "SCIM PATCH",
                uri.path(),
                Some(&user.user_name),
                response.status(),
            )
            .await;
            return response;
        }
    };
    let provisioner = match provisioner(&state) {
        Ok(provisioner) => provisioner,
        Err(response) => return response,
    };
    let response = match apply_changes(provisioner.clone(), user.clone(), changes).await {
        Ok(updated) => scim_json(StatusCode::OK, user_json(&updated)),
        Err(response) => response,
    };
    audit(
        &state,
        "SCIM PATCH",
        uri.path(),
        Some(&user.user_name),
        response.status(),
    )
    .await;
    response
}

async fn put_user(
    State(state): State<Arc<RemoteState>>,
    OriginalUri(uri): OriginalUri,
    Path(id): Path<String>,
    payload: Result<Json<Value>, JsonRejection>,
) -> Response {
    let user = match get_user_handler(&state, &id).await {
        Ok(user) => user,
        Err(response) => {
            audit(&state, "SCIM PUT", uri.path(), None, response.status()).await;
            return response;
        }
    };
    let body = match json_body(payload) {
        Ok(body) => body,
        Err(response) => {
            audit(
                &state,
                "SCIM PUT",
                uri.path(),
                Some(&user.user_name),
                response.status(),
            )
            .await;
            return response;
        }
    };
    let user_name = match required_user_name(&body) {
        Ok(user_name) => user_name,
        Err(response) => {
            audit(
                &state,
                "SCIM PUT",
                uri.path(),
                Some(&user.user_name),
                response.status(),
            )
            .await;
            return response;
        }
    };
    let active = match active_value(&body) {
        Ok(active) => active,
        Err(response) => {
            audit(
                &state,
                "SCIM PUT",
                uri.path(),
                Some(&user.user_name),
                response.status(),
            )
            .await;
            return response;
        }
    };
    let provisioner = match provisioner(&state) {
        Ok(provisioner) => provisioner,
        Err(response) => return response,
    };
    let changes = PatchChanges {
        active: Some(active),
        user_name: (user_name != user.user_name).then_some(user_name),
    };
    let response = match apply_changes(provisioner.clone(), user.clone(), changes).await {
        Ok(updated) => scim_json(StatusCode::OK, user_json(&updated)),
        Err(response) => response,
    };
    audit(
        &state,
        "SCIM PUT",
        uri.path(),
        Some(&user.user_name),
        response.status(),
    )
    .await;
    response
}

async fn delete_user(
    State(state): State<Arc<RemoteState>>,
    OriginalUri(uri): OriginalUri,
    Path(id): Path<String>,
) -> Response {
    let user = match get_user_handler(&state, &id).await {
        Ok(user) => user,
        Err(response) => {
            audit(&state, "SCIM DELETE", uri.path(), None, response.status()).await;
            return response;
        }
    };
    if !user.active {
        audit(
            &state,
            "SCIM DELETE",
            uri.path(),
            Some(&user.user_name),
            StatusCode::NO_CONTENT,
        )
        .await;
        return StatusCode::NO_CONTENT.into_response();
    }
    let provisioner = match provisioner(&state) {
        Ok(provisioner) => provisioner,
        Err(response) => return response,
    };
    let response = match provisioner.set_active(&user.id, false).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => provision_failure(error),
    };
    audit(
        &state,
        "SCIM DELETE",
        uri.path(),
        Some(&user.user_name),
        response.status(),
    )
    .await;
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_parsing_supports_the_three_fields_and_escapes() {
        assert_eq!(
            parse_filter("userName eq \"alice\"").expect("username"),
            (ScimFilterField::UserName, "alice".to_string())
        );
        assert_eq!(
            parse_filter("EXTERNALID EQ \"00u1\"").expect("external id"),
            (ScimFilterField::ExternalId, "00u1".to_string())
        );
        assert_eq!(
            parse_filter("id eq \"a\\\"b\\\\c\"").expect("escaped"),
            (ScimFilterField::Id, "a\"b\\c".to_string())
        );
        assert!(parse_filter("displayName eq \"x\"").is_err());
        assert!(parse_filter("userName sw \"a\"").is_err());
        assert!(parse_filter("userName eq alice").is_err());
        assert!(parse_filter("userName eq \"a\\n\"").is_err());
        assert!(parse_filter("userName eq \"a\" trailing").is_err());
    }
}
