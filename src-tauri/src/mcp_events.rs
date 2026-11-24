//! Long-lived MCP notification listeners feeding routines.
//!
//! The supervisor groups enabled triggers by MCP server, starts one stdio
//! listener per server, and restarts it when the trigger set or server config
//! changes. Notifications are matched against each trigger's method, resource
//! URI and substring filter; a matched trigger claims a cooldown slot in SQL
//! before the routine is dispatched, so a log storm cannot start the same
//! routine dozens of times.
//!
//! HTTP MCP servers are intentionally reported as unsupported: their transport
//! here is request/response, and pretending a one-shot POST can receive pushes
//! would be a lie. Stdio listeners are real, reconnect with backoff, and run in
//! both the desktop process and the headless daemon.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use ravenbot_db::queries::{McpEventTrigger, McpEventTriggerQueries};
use ravenbot_db::Database;
use ravenbot_mcp::servers::{McpServerConfig, McpTransport};
use ravenbot_mcp::{notification_matches, McpClient, McpNotification, McpRegistry};
use ravenbot_scheduler::routine::RoutineManager;
use serde::Serialize;
use serde_json::json;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use tokio::time::{Instant, MissedTickBehavior};

use crate::{execute_routine_instruction_with_context, HostEvents};

const TICK: Duration = Duration::from_secs(15);
const INITIAL_DELAY: Duration = Duration::from_secs(8);
const RETRY_AFTER_EXIT: Duration = Duration::from_secs(10);
const MAX_IN_FLIGHT: usize = 6;
const MAX_CONTEXT_CHARS: usize = 4_000;

#[derive(Debug, Clone, Serialize)]
pub struct McpEventStatus {
    pub server_id: String,
    /// `starting`, `running`, `retrying`, `unsupported`, `error`, `stopped`.
    pub state: String,
    pub detail: String,
    pub updated_at: String,
}

fn status_map() -> &'static Arc<RwLock<HashMap<String, McpEventStatus>>> {
    static STATUS: OnceLock<Arc<RwLock<HashMap<String, McpEventStatus>>>> = OnceLock::new();
    STATUS.get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
}

fn set_status(server_id: &str, state: &str, detail: impl Into<String>) {
    let entry = McpEventStatus {
        server_id: server_id.to_string(),
        state: state.to_string(),
        detail: detail.into(),
        updated_at: Utc::now().to_rfc3339(),
    };
    if let Ok(mut map) = status_map().write() {
        map.insert(server_id.to_string(), entry);
    }
}

/// Status snapshot for Settings → Routines and the event-trigger panel.
pub fn statuses() -> Vec<McpEventStatus> {
    let mut out: Vec<McpEventStatus> = status_map()
        .read()
        .map(|map| map.values().cloned().collect())
        .unwrap_or_default();
    out.sort_by(|a, b| a.server_id.cmp(&b.server_id));
    out
}

fn running_triggers() -> &'static Mutex<HashSet<uuid::Uuid>> {
    static RUNNING: OnceLock<Mutex<HashSet<uuid::Uuid>>> = OnceLock::new();
    RUNNING.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Prevents a routine that takes longer than its cooldown from overlapping
/// with itself. The DB cooldown gates event storms; this gates slow runs.
struct TriggerRunGuard(uuid::Uuid);

impl TriggerRunGuard {
    fn acquire(id: uuid::Uuid) -> Option<Self> {
        let mut running = running_triggers().lock().ok()?;
        if !running.insert(id) {
            return None;
        }
        Some(Self(id))
    }
}

impl Drop for TriggerRunGuard {
    fn drop(&mut self) {
        if let Ok(mut running) = running_triggers().lock() {
            running.remove(&self.0);
        }
    }
}

fn in_flight() -> &'static Arc<Semaphore> {
    static PERMITS: OnceLock<Arc<Semaphore>> = OnceLock::new();
    PERMITS.get_or_init(|| Arc::new(Semaphore::new(MAX_IN_FLIGHT)))
}

struct RunningListener {
    signature: String,
    task: JoinHandle<()>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ServerSignature(String);

impl ServerSignature {
    fn new(
        config: &McpServerConfig,
        env: &HashMap<String, String>,
        triggers: &[McpEventTrigger],
        subscriptions: &[String],
    ) -> Self {
        let mut signature = format!(
            "{}\u{0}{}\u{0}{:?}\u{0}{}\u{0}{:?}",
            config.id,
            config.command,
            config.args,
            config.url.as_deref().unwrap_or(""),
            config.transport
        );
        let mut env_pairs: Vec<_> = env.iter().collect();
        env_pairs.sort_by(|a, b| a.0.cmp(b.0));
        for (key, value) in env_pairs {
            signature.push('\u{0}');
            signature.push_str(key);
            signature.push('=');
            signature.push_str(value);
        }
        let mut ids: Vec<_> = triggers.iter().collect();
        ids.sort_by_key(|trigger| trigger.id);
        for trigger in ids {
            signature.push('\u{1}');
            signature.push_str(&trigger.id.to_string());
            signature.push('|');
            signature.push_str(&trigger.event);
            signature.push('|');
            signature.push_str(&trigger.resource_uri);
            signature.push('|');
            signature.push_str(&trigger.match_text);
            signature.push('|');
            signature.push_str(&trigger.cooldown_secs.to_string());
        }
        for uri in subscriptions {
            signature.push('\u{2}');
            signature.push_str(uri);
        }
        Self(signature)
    }
}

/// Background supervisor. Runs first in the desktop process and in the
/// headless daemon; exactly one fleet owner means exactly one set of listeners.
pub(crate) async fn run_loop(
    db: Database,
    runtime: Arc<ravenbot_runtime::Runtime>,
    events: HostEvents,
) {
    tokio::time::sleep(INITIAL_DELAY).await;
    let mut running: HashMap<String, RunningListener> = HashMap::new();
    let mut retry_at: HashMap<String, Instant> = HashMap::new();
    let mut ticker = tokio::time::interval(TICK);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        ticker.tick().await;
        let triggers = match McpEventTriggerQueries::list_enabled(db.pool()).await {
            Ok(triggers) => triggers,
            Err(error) => {
                tracing::warn!(%error, "MCP event trigger list failed");
                continue;
            }
        };
        let mut groups: HashMap<String, Vec<McpEventTrigger>> = HashMap::new();
        for trigger in triggers {
            groups
                .entry(trigger.server_id.clone())
                .or_default()
                .push(trigger);
        }
        let active: HashSet<String> = groups.keys().cloned().collect();
        let gone: Vec<String> = running
            .keys()
            .filter(|server| !active.contains(*server))
            .cloned()
            .collect();
        for server in gone {
            if let Some(listener) = running.remove(&server) {
                listener.task.abort();
            }
            retry_at.remove(&server);
            set_status(&server, "stopped", "No enabled event triggers.");
        }

        for (server_id, server_triggers) in groups {
            if retry_at
                .get(&server_id)
                .is_some_and(|at| *at > Instant::now())
            {
                continue;
            }
            let registry = McpRegistry::new(db.pool().clone());
            let resolved = match registry.server_config_with_env(&server_id).await {
                Ok(Some(resolved)) => resolved,
                Ok(None) => {
                    stop_listener(&mut running, &server_id);
                    set_status(
                        &server_id,
                        "error",
                        "The MCP server is not configured on this device.",
                    );
                    continue;
                }
                Err(error) => {
                    stop_listener(&mut running, &server_id);
                    set_status(
                        &server_id,
                        "error",
                        format!("Configuration failed: {error}"),
                    );
                    continue;
                }
            };
            let (config, env) = resolved;
            let builtin = config.command == "ravenbot-builtin" || config.command == "builtin";
            if builtin
                || config.transport != McpTransport::Stdio
                || config
                    .url
                    .as_deref()
                    .is_some_and(|url| !url.trim().is_empty())
            {
                stop_listener(&mut running, &server_id);
                set_status(
                    &server_id,
                    "unsupported",
                    if builtin {
                        "Built-in connectors do not emit MCP notifications."
                    } else {
                        "Event triggers require a stdio MCP server; this connector uses HTTP."
                    },
                );
                continue;
            }

            let mut subscriptions: Vec<String> = server_triggers
                .iter()
                .map(|trigger| trigger.resource_uri.trim().to_string())
                .filter(|uri| !uri.is_empty())
                .collect();
            subscriptions.sort();
            subscriptions.dedup();
            let signature = ServerSignature::new(&config, &env, &server_triggers, &subscriptions);
            if running.get(&server_id).is_some_and(|listener| {
                !listener.task.is_finished() && listener.signature == signature.0
            }) {
                continue;
            }
            stop_listener(&mut running, &server_id);
            let db = db.clone();
            let runtime = runtime.clone();
            let events = events.clone();
            let trigger_snapshot = Arc::new(server_triggers);
            let task = tokio::spawn(server_listener(
                db,
                runtime,
                events,
                server_id.clone(),
                config,
                env,
                trigger_snapshot,
                subscriptions,
            ));
            running.insert(
                server_id.clone(),
                RunningListener {
                    signature: signature.0,
                    task,
                },
            );
            retry_at.remove(&server_id);
            set_status(
                &server_id,
                "starting",
                "Starting the stdio notification listener.",
            );
        }

        // A listener that panicked must not be respawned in a hot loop.
        let now = Instant::now();
        for (server, listener) in running.iter_mut() {
            if listener.task.is_finished() {
                retry_at.insert(server.clone(), now + RETRY_AFTER_EXIT);
            }
        }
    }
}

fn stop_listener(running: &mut HashMap<String, RunningListener>, server_id: &str) {
    if let Some(listener) = running.remove(server_id) {
        listener.task.abort();
    }
}

#[allow(clippy::too_many_arguments)]
async fn server_listener(
    db: Database,
    runtime: Arc<ravenbot_runtime::Runtime>,
    events: HostEvents,
    server_id: String,
    config: McpServerConfig,
    env: HashMap<String, String>,
    triggers: Arc<Vec<McpEventTrigger>>,
    subscriptions: Vec<String>,
) {
    let mut backoff = Duration::from_secs(5);
    loop {
        set_status(
            &server_id,
            "starting",
            format!("Opening a {server_id} notification session."),
        );
        let client = McpClient::with_env(config.clone(), env.clone());
        let db_for_event = db.clone();
        let runtime_for_event = runtime.clone();
        let events_for_event = events.clone();
        let triggers_for_event = triggers.clone();
        let status_server = server_id.clone();
        let ready_server = server_id.clone();
        let db_for_subscription = db.clone();
        let triggers_for_subscription = triggers.clone();
        let result = client
            .listen_notifications(
                &subscriptions,
                move |notification| {
                    set_status(
                        &status_server,
                        "running",
                        format!("Last event: {}", notification.method),
                    );
                    dispatch(
                        db_for_event.clone(),
                        runtime_for_event.clone(),
                        events_for_event.clone(),
                        triggers_for_event.clone(),
                        notification,
                    );
                    true
                },
                move |uri, error| {
                    record_subscription_error(
                        db_for_subscription.clone(),
                        triggers_for_subscription.clone(),
                        uri,
                        &error,
                    );
                },
                move || {
                    set_status(&ready_server, "running", "Notification session ready.");
                },
            )
            .await;
        if let Err(error) = result {
            set_status(
                &server_id,
                "retrying",
                format!(
                    "Listener stopped: {error}. Retrying in {}s.",
                    backoff.as_secs()
                ),
            );
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(Duration::from_secs(60));
        } else {
            set_status(&server_id, "stopped", "The notification listener stopped.");
            return;
        }
    }
}

fn record_subscription_error(
    db: Database,
    triggers: Arc<Vec<McpEventTrigger>>,
    uri: &str,
    error: &str,
) {
    for trigger in triggers
        .iter()
        .filter(|trigger| trigger.resource_uri == uri)
    {
        let db = db.clone();
        let id = trigger.id;
        let detail = format!("resources/subscribe failed: {error}");
        tokio::spawn(async move {
            let _ = McpEventTriggerQueries::record_error(db.pool(), id, &detail).await;
        });
    }
}

fn dispatch(
    db: Database,
    runtime: Arc<ravenbot_runtime::Runtime>,
    events: HostEvents,
    triggers: Arc<Vec<McpEventTrigger>>,
    notification: McpNotification,
) {
    for trigger in triggers.iter().filter(|trigger| {
        notification_matches(
            &trigger.event,
            &trigger.resource_uri,
            &trigger.match_text,
            &notification,
        )
    }) {
        let permit = match in_flight().clone().try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => {
                tracing::warn!(
                    server = %notification.server_id,
                    method = %notification.method,
                    "MCP event dropped: too many routine starts are already in flight"
                );
                continue;
            }
        };
        let db = db.clone();
        let runtime = runtime.clone();
        let events = events.clone();
        let trigger = trigger.clone();
        let notification = notification.clone();
        tokio::spawn(async move {
            let _permit = permit;
            fire_trigger(db, runtime, events, trigger, notification).await;
        });
    }
}

async fn fire_trigger(
    db: Database,
    runtime: Arc<ravenbot_runtime::Runtime>,
    events: HostEvents,
    trigger: McpEventTrigger,
    notification: McpNotification,
) {
    let Some(_single_flight) = TriggerRunGuard::acquire(trigger.id) else {
        tracing::debug!(
            trigger_id = %trigger.id,
            "MCP event skipped: the previous run of this trigger is still active"
        );
        return;
    };
    let now = Utc::now();
    let cutoff = now - ChronoDuration::seconds(trigger.cooldown_secs.clamp(0, 3_600));
    let claimed = match McpEventTriggerQueries::claim(
        db.pool(),
        trigger.id,
        &now.to_rfc3339(),
        &cutoff.to_rfc3339(),
    )
    .await
    {
        Ok(claimed) => claimed,
        Err(error) => {
            tracing::warn!(trigger_id = %trigger.id, %error, "MCP event claim failed");
            return;
        }
    };
    if !claimed {
        return;
    }

    let routine = match RoutineManager::new(db.pool().clone())
        .get(trigger.routine_id)
        .await
    {
        Ok(Some(routine)) => routine,
        Ok(None) => {
            let _ = McpEventTriggerQueries::record_error(
                db.pool(),
                trigger.id,
                "The routine no longer exists.",
            )
            .await;
            return;
        }
        Err(error) => {
            let _ = McpEventTriggerQueries::record_error(
                db.pool(),
                trigger.id,
                &format!("Could not load the routine: {error}"),
            )
            .await;
            return;
        }
    };

    let context = notification.context(MAX_CONTEXT_CHARS);
    match execute_routine_instruction_with_context(&db, &runtime, &events, &routine, Some(&context))
        .await
    {
        Ok(()) => {
            events.emit(
                "mcp-event-fired",
                json!({
                    "trigger_id": trigger.id.to_string(),
                    "routine_id": trigger.routine_id.to_string(),
                    "server_id": notification.server_id,
                    "method": notification.method,
                }),
            );
        }
        Err(error) => {
            let _ = McpEventTriggerQueries::record_error(
                db.pool(),
                trigger.id,
                &format!("Routine start failed: {error}"),
            )
            .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trigger(id: u128, event: &str, resource_uri: &str) -> McpEventTrigger {
        McpEventTrigger {
            id: uuid::Uuid::from_u128(id),
            routine_id: uuid::Uuid::from_u128(9),
            server_id: "github".to_string(),
            event: event.to_string(),
            resource_uri: resource_uri.to_string(),
            match_text: String::new(),
            cooldown_secs: 60,
            enabled: true,
            last_fired_at: None,
            last_error: String::new(),
            created_at: "2026-01-01T00:00:00+00:00".to_string(),
            updated_at: "2026-01-01T00:00:00+00:00".to_string(),
        }
    }

    #[test]
    fn one_trigger_does_not_overlap_its_own_previous_run() {
        let id = uuid::Uuid::new_v4();
        let first = TriggerRunGuard::acquire(id).expect("first run");
        assert!(TriggerRunGuard::acquire(id).is_none());
        drop(first);
        assert!(TriggerRunGuard::acquire(id).is_some());
    }

    #[test]
    fn a_signature_changes_with_the_trigger_set_and_subscriptions() {
        let config = ravenbot_mcp::servers::all_servers()
            .into_iter()
            .find(|server| server.id == "github")
            .expect("github");
        let env = HashMap::new();
        let first = ServerSignature::new(
            &config,
            &env,
            &[trigger(1, "notifications/message", "")],
            &[],
        );
        let second = ServerSignature::new(
            &config,
            &env,
            &[
                trigger(1, "notifications/message", ""),
                trigger(2, "notifications/resources/updated", "file:///a"),
            ],
            &["file:///a".to_string()],
        );
        assert_ne!(first, second);
    }
}
