//! Graph executor for parallel task execution
//!
//! This module executes a task graph, running ready nodes in parallel
//! and managing the shared blackboard.

use crate::graph::{reports_incomplete_work, Blackboard, NodeState, TaskGraph, WorkflowNodeType};
use crate::work::WorkLedger;
use crate::Runtime;
use ravenbot_core::Run;
use ravenbot_db::Database;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Has a run gone `threshold` without touching its heartbeat?
///
/// Split out from the watchdog loop so the boundary is testable without
/// sleeping for a real stall interval.
fn progress_stalled(
    last: chrono::DateTime<chrono::Utc>,
    now: chrono::DateTime<chrono::Utc>,
    threshold: std::time::Duration,
) -> bool {
    now.signed_duration_since(last).num_milliseconds()
        >= threshold.as_millis().min(i64::MAX as u128) as i64
}

/// Callback that ships office-run telemetry (plan/node status) to the UI as
/// ad-hoc JSON on the `agent-stream` channel. Kept separate from `StreamEvent`
/// so node-lifecycle facts never touch the per-thread token routing.
pub type NodeEventSink = Arc<dyn Fn(serde_json::Value) + Send + Sync>;

/// Builds the live stream emitter for a node's own thread.
///
/// Office nodes create their own threads, so they used to share the runtime's
/// single fallback emitter. That forced every office graph in the process to
/// run one at a time. A factory lets each node bind its stream to its own
/// thread, so offices (and ordinary chats) can run concurrently without mixing
/// token streams.
pub type ThreadEmitterFactory = Arc<dyn Fn(Uuid) -> Option<crate::StreamEmitter> + Send + Sync>;

fn emit_node_event(sink: &Option<NodeEventSink>, event: serde_json::Value) {
    if let Some(f) = sink {
        f(event);
    }
}

async fn finish_graph_node(
    graph: &Arc<Mutex<TaskGraph>>,
    node_event: &Option<NodeEventSink>,
    node_id: Uuid,
    bot_id: Uuid,
    text: String,
    max_iterations: u32,
) -> Result<(), String> {
    let mut g = graph.lock().await;
    if reports_incomplete_work(&text) {
        let message = "The node reported its own task as incomplete; its report is a progress note, not a completed artifact.".to_string();
        if let Some(node) = g.nodes.get_mut(&node_id) {
            node.output = Some(text);
        }
        let skipped_ids = fail_node_and_skip(&mut g, node_id, message.clone())?;
        emit_node_event(
            node_event,
            serde_json::json!({
                "kind": "node_finished",
                "node_id": node_id.to_string(),
                "bot_id": bot_id.to_string(),
                "state": "failed",
                "preview": preview_chars(&message, 160),
                "skipped": skipped_ids,
            }),
        );
        return Ok(());
    }
    match g
        .mark_loop_iteration(node_id, &text)
        .map_err(|error| error.to_string())?
    {
        Some(next_iteration) => emit_node_event(
            node_event,
            serde_json::json!({
                "kind": "node_iteration",
                "node_id": node_id.to_string(),
                "bot_id": bot_id.to_string(),
                "iteration": next_iteration,
                "max_iterations": max_iterations,
                "preview": preview_chars(&text, 160),
            }),
        ),
        None => {
            g.mark_done(node_id, text.clone())
                .map_err(|error| error.to_string())?;
            emit_node_event(
                node_event,
                serde_json::json!({
                    "kind": "node_finished",
                    "node_id": node_id.to_string(),
                    "bot_id": bot_id.to_string(),
                    "state": "done",
                    "preview": preview_chars(&text, 160),
                }),
            );
        }
    }
    Ok(())
}

fn preview_chars(s: &str, max: usize) -> String {
    let mut out: String = s.chars().take(max).collect();
    if s.chars().count() > max {
        out.push('…');
    }
    out
}

/// Pull a single JSON value out of a model answer.
///
/// Models routinely wrap structured output in a fenced block or add one
/// sentence of preamble. Both are accepted; prose that only mentions JSON is
/// not. Returning the parsed value (not the raw text) is what lets the graph
/// pass a clean, canonical payload to dependents.
fn extract_json_answer(text: &str) -> Result<serde_json::Value, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("the answer was empty".to_string());
    }
    if let Ok(value) = serde_json::from_str(trimmed) {
        return Ok(value);
    }

    // Fenced blocks, with or without a language label.
    let mut rest = trimmed;
    while let Some(start) = rest.find("```") {
        let after = &rest[start + 3..];
        let Some(end) = after.find("```") else {
            break;
        };
        let raw_block = after[..end].trim();
        let block = match raw_block.split_once('\n') {
            Some((label, body))
                if label.trim().eq_ignore_ascii_case("json")
                    || label.trim().eq_ignore_ascii_case("jsonc") =>
            {
                body.trim()
            }
            _ => raw_block,
        };
        if let Ok(value) = serde_json::from_str(block) {
            return Ok(value);
        }
        rest = &after[end + 3..];
    }

    // First opening delimiter through the last matching closing delimiter.
    let start = [trimmed.find('{'), trimmed.find('[')]
        .into_iter()
        .flatten()
        .min();
    if let Some(start) = start {
        let closing = if trimmed[start..].starts_with('{') {
            '}'
        } else {
            ']'
        };
        if let Some(end) = trimmed.rfind(closing) {
            if end > start {
                if let Ok(value) = serde_json::from_str(&trimmed[start..=end]) {
                    return Ok(value);
                }
            }
        }
    }

    Err("the answer did not contain a parseable JSON value".to_string())
}

fn validate_against_schema(
    schema: &serde_json::Value,
    value: &serde_json::Value,
) -> Result<(), String> {
    let validator = jsonschema::validator_for(schema)
        .map_err(|error| format!("the output schema is invalid: {error}"))?;
    if let Err(errors) = validator.validate(value) {
        let details: Vec<String> = errors.take(3).map(|error| error.to_string()).collect();
        return Err(format!(
            "the answer did not match the output schema: {}",
            details.join("; ")
        ));
    }
    Ok(())
}

struct ThreadEmitterGuard {
    runtime: Arc<Runtime>,
    thread_id: Uuid,
}

impl Drop for ThreadEmitterGuard {
    fn drop(&mut self) {
        self.runtime.set_thread_emitter(self.thread_id, None);
    }
}

/// Executor for running task graphs
pub struct GraphExecutor {
    runtime: Arc<Runtime>,
    db: Database,
    /// Project folders every node in this graph works in (JSON array). Stamped
    /// onto each node thread so the runtime confines tool work correctly.
    working_dirs: Vec<String>,
    /// Optional live node-lifecycle events for the office board UI.
    node_event: Option<NodeEventSink>,
    /// Optional per-node stream emitter factory (one emitter per node thread).
    thread_emitter: Option<ThreadEmitterFactory>,
    /// Office this graph belongs to. When set, every node thread is linked to
    /// the office so it inherits the office context, roster, memory and
    /// delegation permissions — not just the project folders.
    office_id: Option<Uuid>,
    /// Interrupted node runs whose checkpoints should be replayed. Keyed by
    /// node id; a node without an entry starts a fresh run.
    resume_runs: std::collections::HashMap<Uuid, ravenbot_core::Run>,
    /// Threads of nodes interrupted by a process restart. Their instruction is
    /// already in the thread even when the node had no checkpoint.
    resume_threads: std::collections::HashSet<Uuid>,
}

impl GraphExecutor {
    /// Create a new executor
    pub fn new(runtime: Arc<Runtime>, db: Database) -> Self {
        Self {
            runtime,
            db,
            working_dirs: Vec::new(),
            node_event: None,
            thread_emitter: None,
            office_id: None,
            resume_runs: std::collections::HashMap::new(),
            resume_threads: std::collections::HashSet::new(),
        }
    }

    /// Resume specific interrupted node runs from their checkpoints.
    pub fn with_resume_runs(
        mut self,
        runs: std::collections::HashMap<Uuid, ravenbot_core::Run>,
    ) -> Self {
        self.resume_runs = runs;
        self
    }

    /// Threads whose instruction already exists in the transcript because the
    /// node was interrupted by a process restart.
    pub fn with_resume_threads(mut self, threads: std::collections::HashSet<Uuid>) -> Self {
        self.resume_threads = threads;
        self
    }

    /// Link every node thread to the office that owns this graph.
    pub fn with_office(mut self, office_id: Uuid) -> Self {
        self.office_id = Some(office_id);
        self
    }

    /// Set the project folders every node thread inherits.
    pub fn with_working_dirs(mut self, dirs: Vec<String>) -> Self {
        self.working_dirs = dirs;
        self
    }

    /// Receive `node_open` / `node_finished` / `graph_status` events while the
    /// graph runs (used by `src-tauri` to feed the office planner board).
    pub fn with_node_events(mut self, sink: NodeEventSink) -> Self {
        self.node_event = Some(sink);
        self
    }

    /// Bind each node's live token stream to that node's own thread.
    pub fn with_thread_emitter(mut self, factory: ThreadEmitterFactory) -> Self {
        self.thread_emitter = Some(factory);
        self
    }

    /// Execute a task graph, running ready tasks in parallel
    pub async fn execute(&self, graph: Arc<Mutex<TaskGraph>>) -> Result<Blackboard, String> {
        tracing::info!("Starting graph execution");

        loop {
            // Get ready nodes
            let (ready_nodes, skipped_event, retried_event, loop_event) = {
                let mut g = graph.lock().await;
                // Retry failed nodes before the skip cascade, so a retry can
                // still rescue its dependents.
                let retried = g.retry_failed_nodes();
                if !retried.is_empty() {
                    tracing::info!(count = retried.len(), "Retrying failed workflow nodes");
                }
                let retried_event = if retried.is_empty() {
                    None
                } else {
                    Some(serde_json::json!({
                        "kind": "graph_status",
                        "nodes": retried
                            .iter()
                            .map(|id| serde_json::json!({
                                "node_id": id.to_string(),
                                "state": "pending",
                                "retrying": true,
                            }))
                            .collect::<Vec<_>>(),
                    }))
                };
                // A completed multi-task group either resets for another
                // iteration or publishes its terminal output. This runs before
                // conditions and readiness, so external dependents cannot see a
                // Done group member while the group is still looping.
                let advanced = g.advance_loop_groups();
                let loop_event = if advanced.is_empty() {
                    None
                } else {
                    Some(serde_json::json!({
                        "kind": "graph_status",
                        "nodes": advanced
                            .iter()
                            .map(|(id, iteration)| serde_json::json!({
                                "node_id": id.to_string(),
                                "state": "pending",
                                "looping": true,
                                "group_iteration": iteration,
                            }))
                            .collect::<Vec<_>>(),
                    }))
                };

                // Evaluate branch conditions, then cascade skips so a failed or
                // condition-skipped branch can't strand dependents.
                let condition_skipped = g.apply_conditions();
                let skipped = g.propagate_skips();
                if !condition_skipped.is_empty() || skipped > 0 {
                    tracing::info!(
                        conditions = condition_skipped.len(),
                        failed_dependencies = skipped,
                        "Skipped workflow nodes"
                    );
                }
                let skipped_event = if !condition_skipped.is_empty() || skipped > 0 {
                    let nodes: Vec<serde_json::Value> = g
                        .nodes
                        .values()
                        .filter(|n| matches!(n.state, NodeState::Skipped))
                        .map(|n| {
                            serde_json::json!({
                                "node_id": n.id.to_string(),
                                "bot_id": n.bot_id.to_string(),
                                "state": "skipped",
                            })
                        })
                        .collect();
                    Some(serde_json::json!({ "kind": "graph_status", "nodes": nodes }))
                } else {
                    None
                };
                let ready = g.ready_nodes();

                if ready.is_empty() {
                    if g.is_complete() {
                        tracing::info!("Graph execution complete");
                        break;
                    }
                    if g.has_deadlock() {
                        return Err(format!(
                            "Deadlock detected in task graph (states: {:?})",
                            g.state_counts()
                        ));
                    }
                    // Wait for running tasks to complete
                    drop(g);
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    continue;
                }

                let ready = ready
                    .into_iter()
                    .map(|n| {
                        // Prefer live dependency outputs (real DAG data flow) over
                        // any input captured when the node was created.
                        let input = g.input_for(n.id).or_else(|| n.input.clone());
                        (
                            n.id,
                            n.bot_id,
                            n.instruction.clone(),
                            input,
                            self.resume_runs.get(&n.id).cloned(),
                        )
                    })
                    .collect::<Vec<_>>();
                (ready, skipped_event, retried_event, loop_event)
            };

            if let Some(ev) = retried_event {
                emit_node_event(&self.node_event, ev);
            }
            if let Some(ev) = loop_event {
                emit_node_event(&self.node_event, ev);
            }
            if let Some(ev) = skipped_event {
                emit_node_event(&self.node_event, ev);
            }

            tracing::info!("Running {} ready nodes", ready_nodes.len());

            // Run ready nodes in bounded batches, not all at once. Seven
            // independent nodes used to become seven simultaneous model calls;
            // one bad key then produced seven identical failures and the whole
            // office stalled. Three is also what the provider governor allows
            // in flight, so the batch size and the wire limit agree.
            const MAX_PARALLEL_NODES: usize = 3;
            let mut fatal = false;

            for chunk in ready_nodes.chunks(MAX_PARALLEL_NODES) {
                let mut handles = Vec::new();
                for (node_id, bot_id, instruction, input, resume_run) in chunk.iter() {
                    let graph = graph.clone();
                    let runtime = self.runtime.clone();
                    let db = self.db.clone();
                    let node_event = self.node_event.clone();
                    let thread_emitter = self.thread_emitter.clone();
                    let working_dirs = self.working_dirs.clone();
                    let office_id = self.office_id;
                    let node_id = *node_id;
                    let bot_id = *bot_id;
                    let instruction = instruction.clone();
                    let input = input.clone();
                    let resume_run = resume_run.clone();
                    let resume_threads = self.resume_threads.clone();
                    let handle = tokio::spawn(async move {
                        Self::execute_node(
                            graph,
                            runtime,
                            db,
                            node_id,
                            bot_id,
                            instruction,
                            input,
                            resume_run,
                            resume_threads,
                            working_dirs,
                            node_event,
                            thread_emitter,
                            office_id,
                        )
                        .await
                    });
                    handles.push(handle);
                }

                for handle in handles {
                    match handle.await {
                        Ok(Ok(())) => {}
                        Ok(Err(e)) => tracing::error!("Task execution error: {}", e),
                        Err(e) => tracing::error!("Task panicked: {}", e),
                    }
                }

                // A panic skips the inner error paths entirely. Any node still
                // Running after its task ended can never finish, so fail it and
                // cascade the skip instead of leaving the board spinning.
                let wedged: Vec<(Uuid, Uuid)> = {
                    let g = graph.lock().await;
                    g.nodes
                        .values()
                        .filter(|node| matches!(node.state, NodeState::Running(_)))
                        .map(|node| (node.id, node.bot_id))
                        .collect()
                };
                for (id, bot_id) in wedged {
                    let mut g = graph.lock().await;
                    let error = "The node task ended without recording a result.".to_string();
                    if let Ok(skipped) = fail_node_and_skip(&mut g, id, error.clone()) {
                        emit_node_event(
                            &self.node_event,
                            serde_json::json!({
                                "kind": "node_finished",
                                "node_id": id.to_string(),
                                "bot_id": bot_id.to_string(),
                                "state": "failed",
                                "preview": error,
                                "skipped": skipped,
                            }),
                        );
                    }
                }

                // Fail fast on configuration-class errors: if a node died
                // because its provider key, model or provider is wrong, running
                // the rest of the office only repeats the same failure. Stop
                // before the next batch and skip everything still pending so
                // the user sees one clear problem instead of seven.
                fatal = {
                    let g = graph.lock().await;
                    g.nodes.values().any(|n| {
                        if let NodeState::Failed(err) = &n.state {
                            crate::is_config_error(err)
                        } else {
                            false
                        }
                    })
                };
                if fatal {
                    break;
                }
            }

            if fatal {
                let mut g = graph.lock().await;
                let pending: Vec<Uuid> = g
                    .nodes
                    .values()
                    .filter(|n| matches!(n.state, NodeState::Pending))
                    .map(|n| n.id)
                    .collect();
                for id in pending {
                    let _ = g.mark_skipped(id);
                }
                tracing::warn!("Stopping the office run: a node failed on a configuration error");
                break;
            }
        }

        // Return the final blackboard state. Reconcile the ledger once more so
        // skipped dependents and retries that no single node transition touched
        // still have an accurate record.
        let (final_blackboard, final_graph) = {
            let g = graph.lock().await;
            (g.blackboard.clone(), g.clone())
        };
        if let Some(office_id) = self.office_id {
            WorkLedger::new(self.db.pool().clone(), office_id)
                .reconcile(&final_graph)
                .await;
            // Publish the readable projection into the office's workspace.
            // The SQLite ledger is authoritative; STATUS.md is for the next
            // agent and the human who opens the folder.
            if let Ok(items) = ravenbot_db::queries::WorkItemQueries::list_by_office(
                self.db.pool(),
                office_id,
                500,
            )
            .await
            {
                let projection = crate::work::status_projection(&items);
                for dir in &self.working_dirs {
                    crate::workspace::write_status_projection(
                        std::path::Path::new(dir),
                        &projection,
                    );
                }
            }
        }
        Ok(final_blackboard)
    }

    /// Execute a single node and guarantee it reaches a terminal state.
    ///
    /// The inner function can fail before its normal failure paths (DB error,
    /// missing bot, thread lookup). Without this wrapper such a node stayed
    /// `Running` forever and the office wedged; dependents also never got the
    /// skip cascade.
    #[allow(clippy::too_many_arguments)]
    async fn execute_node(
        graph: Arc<Mutex<TaskGraph>>,
        runtime: Arc<Runtime>,
        db: Database,
        node_id: Uuid,
        bot_id: Uuid,
        instruction: String,
        input: Option<String>,
        resume_run: Option<ravenbot_core::Run>,
        resume_threads: std::collections::HashSet<Uuid>,
        working_dirs: Vec<String>,
        node_event: Option<NodeEventSink>,
        thread_emitter: Option<ThreadEmitterFactory>,
        office_id: Option<Uuid>,
    ) -> Result<(), String> {
        let sink = node_event.clone();
        let result = Self::execute_node_inner(
            graph.clone(),
            runtime,
            db.clone(),
            node_id,
            bot_id,
            instruction,
            input,
            resume_run,
            resume_threads,
            working_dirs,
            node_event,
            thread_emitter,
            office_id,
        )
        .await;
        if let Err(error) = &result {
            let mut g = graph.lock().await;
            let running = g
                .nodes
                .get(&node_id)
                .is_some_and(|node| matches!(node.state, NodeState::Running(_)));
            if running {
                match fail_node_and_skip(&mut g, node_id, error.clone()) {
                    Ok(skipped) => emit_node_event(
                        &sink,
                        serde_json::json!({
                            "kind": "node_finished",
                            "node_id": node_id.to_string(),
                            "bot_id": bot_id.to_string(),
                            "state": "failed",
                            "preview": preview_chars(error, 160),
                            "skipped": skipped,
                        }),
                    ),
                    Err(failure) => {
                        tracing::error!(node_id = %node_id, error = %failure, "Could not fail a stopped node")
                    }
                }
            }
        }
        // Every terminal path goes through here, including trigger/output
        // nodes and failures before a run row existed.
        if let Some(office_id) = office_id {
            let node = graph.lock().await.nodes.get(&node_id).cloned();
            if let Some(node) = node {
                WorkLedger::new(db.pool().clone(), office_id)
                    .sync(&node)
                    .await;
            }
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_node_inner(
        graph: Arc<Mutex<TaskGraph>>,
        runtime: Arc<Runtime>,
        db: Database,
        node_id: Uuid,
        bot_id: Uuid,
        instruction: String,
        input: Option<String>,
        resume_run: Option<ravenbot_core::Run>,
        resume_threads: std::collections::HashSet<Uuid>,
        working_dirs: Vec<String>,
        node_event: Option<NodeEventSink>,
        thread_emitter: Option<ThreadEmitterFactory>,
        office_id: Option<Uuid>,
    ) -> Result<(), String> {
        // Mark as running. A resumed run keeps its original id so the
        // checkpoint and the ledger point at the same execution.
        let (node_type, output_schema, node_thread_id, iteration, max_iterations, run_id) = {
            let mut g = graph.lock().await;
            let run_id = resume_run
                .as_ref()
                .map(|run| run.id)
                .unwrap_or_else(Uuid::new_v4);
            g.mark_running(node_id, run_id).map_err(|e| e.to_string())?;
            let node = g
                .nodes
                .get(&node_id)
                .ok_or_else(|| format!("Node not found: {node_id}"))?;
            (
                node.node_type,
                node.output_schema.clone(),
                resume_run
                    .as_ref()
                    .map(|run| run.thread_id)
                    .or(node.thread_id),
                node.iterations,
                node.max_iterations,
                run_id,
            )
        };

        // Reflect the claim in the durable ledger before any work starts.
        if let Some(office_id) = office_id {
            let node = graph.lock().await.nodes.get(&node_id).cloned();
            if let Some(node) = node {
                WorkLedger::new(db.pool().clone(), office_id)
                    .started(&node, run_id)
                    .await;
            }
        }

        tracing::info!(
            node_id = %node_id,
            bot_id = %bot_id,
            instruction = %instruction,
            "Executing node"
        );

        // Trigger and Output nodes do not need a bot or a model call.
        if matches!(
            node_type,
            WorkflowNodeType::Trigger | WorkflowNodeType::Output
        ) {
            let mut text = if node_type == WorkflowNodeType::Output {
                input.clone().unwrap_or_else(|| instruction.clone())
            } else {
                instruction.clone()
            };
            if let Some(schema) = &output_schema {
                let value = extract_json_answer(&text)?;
                validate_against_schema(schema, &value)?;
                text = serde_json::to_string(&value).unwrap_or(text);
            }
            let preview = preview_chars(&text, 160);
            graph
                .lock()
                .await
                .mark_done(node_id, text)
                .map_err(|e| e.to_string())?;
            emit_node_event(
                &node_event,
                serde_json::json!({
                    "kind": "node_finished",
                    "node_id": node_id.to_string(),
                    "bot_id": bot_id.to_string(),
                    "state": "done",
                    "preview": preview,
                }),
            );
            return Ok(());
        }

        // Get bot
        let bot = ravenbot_db::queries::BotQueries::get(db.pool(), bot_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Bot {} not found", bot_id))?;

        // Reuse the node's thread across loop iterations and retries so one
        // task reads as one conversation. If the owner deleted it, create a
        // fresh one rather than failing the node forever.
        let thread_id = match node_thread_id {
            Some(tid)
                if ravenbot_db::queries::ThreadQueries::get(db.pool(), tid)
                    .await
                    .map_err(|e| e.to_string())?
                    .is_some() =>
            {
                tid
            }
            _ => {
                let thread = ravenbot_core::Thread::new_internal(
                    ravenbot_core::ThreadKind::Node,
                    bot_id,
                    &instruction,
                );
                ravenbot_db::queries::ThreadQueries::create(db.pool(), &thread)
                    .await
                    .map_err(|e| e.to_string())?;
                {
                    let mut g = graph.lock().await;
                    if let Some(node) = g.nodes.get_mut(&node_id) {
                        node.thread_id = Some(thread.id);
                    }
                }
                // Link the node to its office before any work runs. Without
                // this the runtime cannot resolve the office for the node, so
                // goal/policy/roster/shared memory and delegation permissions
                // are silently absent.
                if let Some(office_id) = office_id {
                    if let Err(error) = link_node_thread(&db, office_id, thread.id).await {
                        tracing::warn!(%office_id, thread_id = %thread.id, %error, "could not link office node thread");
                    }
                }
                thread.id
            }
        };
        // Tell the office board which thread this node streams on, so deltas
        // can be attributed to the right card even for parallel same-bot nodes.
        emit_node_event(
            &node_event,
            serde_json::json!({
                "kind": "node_open",
                "node_id": node_id.to_string(),
                "bot_id": bot_id.to_string(),
                "node_thread_id": thread_id.to_string(),
                "instruction": instruction.as_str(),
            }),
        );
        // Route this node's stream to its own thread. The guard clears the
        // registration on every exit path, including an early `?` or a panic
        // unwinding the task.
        let _emitter_guard = thread_emitter.as_ref().and_then(|factory| {
            factory(thread_id).map(|emitter| {
                runtime.set_thread_emitter(thread_id, Some(emitter));
                ThreadEmitterGuard {
                    runtime: runtime.clone(),
                    thread_id,
                }
            })
        });
        // Remember which project folders this node may work in.
        if !working_dirs.is_empty() {
            let json = serde_json::to_string(&working_dirs).unwrap_or_else(|_| "[]".to_string());
            let _ = sqlx::query("UPDATE threads SET project_folders = ? WHERE id = ?")
                .bind(json)
                .bind(thread_id.to_string())
                .execute(db.pool())
                .await;
        }

        // A checkpointed run already has its task and dependency input in the
        // checkpoint. A thread interrupted without a checkpoint already has
        // the instruction in its transcript; only a genuinely new thread needs
        // it inserted. Either way, inserting it again would duplicate the
        // prompt.
        let resuming = resume_run.is_some() || resume_threads.contains(&thread_id);
        if !resuming {
            if let Some(input) = &input {
                let input_msg = ravenbot_core::Message::user(
                    thread_id,
                    format!("Input from previous task:\n\n{}", input),
                );
                ravenbot_db::queries::MessageQueries::insert(db.pool(), &input_msg)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }

        let mut prompt_instruction = instruction.clone();
        if max_iterations > 1 {
            prompt_instruction.push_str(&format!(
                "\n\n---\n## Iteration\nThis is iteration {} of {}. Improve on your previous answer in this conversation and return a complete, standalone answer.",
                iteration + 1,
                max_iterations
            ));
        }
        // When the node has a typed output, tell the model exactly what shape
        // to produce. The schema is repeated in the prompt instead of being a
        // hidden post-check so the model can satisfy it on the first attempt.
        if let Some(schema) = &output_schema {
            let schema_text =
                serde_json::to_string_pretty(schema).unwrap_or_else(|_| "{}".to_string());
            prompt_instruction.push_str(&format!(
                "\n\n---\n## Required output\nReturn only a single JSON value that satisfies the JSON Schema below. Do not add prose, headings, or markdown fences.\n\n```json\n{schema_text}\n```"
            ));
        }

        // Add instruction as user message on a fresh run. A resumed run's
        // checkpoint already contains it.
        if !resuming {
            let user_msg = ravenbot_core::Message::user(thread_id, &prompt_instruction);
            ravenbot_db::queries::MessageQueries::insert(db.pool(), &user_msg)
                .await
                .map_err(|e| e.to_string())?;
        }

        if node_type == WorkflowNodeType::Llm {
            runtime.emit(crate::StreamEvent::Status {
                bot_id,
                thread_id,
                state: "thinking".to_string(),
            });
            let system = bot
                .config
                .custom_prompt
                .clone()
                .unwrap_or_else(|| format!("You are {}, a helpful AI assistant.", bot.name));
            let max_tokens = bot.config.max_tokens.unwrap_or(4096);
            let text = runtime
                .complete_as_bot(bot_id, &system, &prompt_instruction, max_tokens)
                .await
                .map_err(|error| error.to_string())?;
            if text.trim().is_empty() {
                return Err("The LLM node returned no answer.".to_string());
            }
            let final_text = if let Some(schema) = &output_schema {
                let value = extract_json_answer(&text)?;
                validate_against_schema(schema, &value)?;
                serde_json::to_string(&value).unwrap_or(text)
            } else {
                text
            };
            let saved = ravenbot_core::Message::assistant(thread_id, &final_text);
            ravenbot_db::queries::MessageQueries::insert(db.pool(), &saved)
                .await
                .map_err(|e| e.to_string())?;
            runtime.emit(crate::StreamEvent::Status {
                bot_id,
                thread_id,
                state: "done".to_string(),
            });
            return finish_graph_node(
                &graph,
                &node_event,
                node_id,
                bot_id,
                final_text,
                max_iterations,
            )
            .await;
        }

        if node_type == WorkflowNodeType::Approval {
            let approval = ravenbot_core::ApprovalRequest::pending(
                bot_id,
                thread_id,
                Uuid::new_v4(),
                "workflow_approval",
                format!(
                    "Approve workflow step: {}",
                    preview_chars(&instruction, 120)
                ),
                serde_json::json!({ "instruction": instruction, "node_id": node_id }),
                "high",
            );
            ravenbot_db::queries::ApprovalQueries::create(db.pool(), &approval)
                .await
                .map_err(|e| e.to_string())?;
            runtime.emit(crate::StreamEvent::Status {
                bot_id,
                thread_id,
                state: "waiting_on_user".to_string(),
            });
            runtime.emit(crate::StreamEvent::ApprovalRequested {
                bot_id,
                thread_id,
                approval: approval.clone(),
            });
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(600);
            loop {
                if runtime.is_paused().await {
                    let _ = ravenbot_db::queries::ApprovalQueries::decide(
                        db.pool(),
                        approval.id,
                        false,
                        Some("run paused"),
                    )
                    .await;
                    return Err("Approval cancelled because the run was paused.".to_string());
                }
                match ravenbot_db::queries::ApprovalQueries::get(db.pool(), approval.id)
                    .await
                    .map_err(|e| e.to_string())?
                    .map(|request| request.status)
                {
                    Some(ravenbot_core::ApprovalStatus::Allowed) => {
                        runtime.emit(crate::StreamEvent::ApprovalDecided {
                            thread_id,
                            approval_id: approval.id,
                            allowed: true,
                        });
                        runtime.emit(crate::StreamEvent::Status {
                            bot_id,
                            thread_id,
                            state: "thinking".to_string(),
                        });
                        return finish_graph_node(
                            &graph,
                            &node_event,
                            node_id,
                            bot_id,
                            "Approved".to_string(),
                            max_iterations,
                        )
                        .await;
                    }
                    Some(ravenbot_core::ApprovalStatus::Denied)
                    | Some(ravenbot_core::ApprovalStatus::Expired)
                    | None => {
                        runtime.emit(crate::StreamEvent::ApprovalDecided {
                            thread_id,
                            approval_id: approval.id,
                            allowed: false,
                        });
                        return Err("The workflow approval was denied.".to_string());
                    }
                    _ => {}
                }
                if std::time::Instant::now() >= deadline {
                    let _ = ravenbot_db::queries::ApprovalQueries::decide(
                        db.pool(),
                        approval.id,
                        false,
                        Some("approval timed out"),
                    )
                    .await;
                    return Err("The workflow approval timed out.".to_string());
                }
                tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            }
        }

        // Create or restore the run. A restored run keeps its id, checkpoint,
        // and token/cost totals; `execute_run` replays the checkpoint messages
        // and continues from the interrupted tool round.
        let mut run = if let Some(mut run) = resume_run {
            if run.thread_id != thread_id {
                run.thread_id = thread_id;
            }
            if run.checkpoint.is_some() {
                run.state = ravenbot_core::RunState::Paused;
                run.outcome = None;
                run.completed_at = None;
            }
            ravenbot_db::queries::RunQueries::update(db.pool(), &run)
                .await
                .map_err(|e| e.to_string())?;
            run
        } else {
            let run = Run::new(bot_id, thread_id);
            ravenbot_db::queries::RunQueries::insert(db.pool(), &run)
                .await
                .map_err(|e| e.to_string())?;
            run
        };

        // Office nodes run unattended. Safe work proceeds without a card; the
        // runtime's hard safety policy blocks destructive shell/database/file
        // operations before execution. This is per-run and cannot leak into
        // another node.
        runtime.review_approvals_for_run(run.id, true);
        // Bound each node by *progress*, not wall clock. A node that is still
        // streaming tokens or running tools should be allowed to finish; a node
        // whose heartbeat stopped is the one to stop. The hard cap is a final
        // backstop against a run that touches its heartbeat forever.
        let node_stall_secs = std::env::var("RAVENBOT_NODE_STALL_SECS")
            .ok()
            .or_else(|| std::env::var("RAVENBOT_NODE_TIMEOUT_SECS").ok())
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(1800);
        let node_max_secs = std::env::var("RAVENBOT_NODE_MAX_SECS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or_else(|| node_stall_secs.saturating_mul(8).max(3600));
        let watchdog_pool = db.pool().clone();
        let watchdog_run_id = run.id;
        let watchdog = async move {
            let started = std::time::Instant::now();
            loop {
                // Ten times finer than the default stall threshold, and cheap:
                // one indexed row read per tick.
                tokio::time::sleep(std::time::Duration::from_secs(15)).await;
                if started.elapsed().as_secs() >= node_max_secs {
                    return format!(
                        "node exceeded its {node_max_secs}s hard cap (set RAVENBOT_NODE_MAX_SECS to change)"
                    );
                }
                match ravenbot_db::queries::RunProgressQueries::get(&watchdog_pool, watchdog_run_id)
                    .await
                {
                    Ok(Some(progress)) => {
                        let last = progress.heartbeat_at.unwrap_or(progress.updated_at);
                        if progress_stalled(
                            last,
                            chrono::Utc::now(),
                            std::time::Duration::from_secs(node_stall_secs),
                        ) {
                            return format!(
                                "node made no progress for {node_stall_secs}s (phase: {}, last tool: {}); set RAVENBOT_NODE_STALL_SECS to change",
                                progress.phase,
                                progress.last_tool.as_deref().unwrap_or("none"),
                            );
                        }
                    }
                    // A missing/erroring row is not proof of a stall; the hard
                    // cap still bounds the node.
                    Ok(None) => {}
                    Err(error) => {
                        tracing::warn!(run_id = %watchdog_run_id, %error, "Could not read run heartbeat");
                    }
                }
            }
        };
        let result = tokio::select! {
            result = runtime.execute_run(&mut run) => result,
            stalled = watchdog => {
                runtime.request_cancel(run.id);
                let _ = runtime.cancel_run(&mut run).await;
                let _ = ravenbot_db::queries::RunProgressQueries::finish(
                    db.pool(),
                    run.id,
                    "failed",
                    Some(&stalled),
                )
                .await;
                let _ = ravenbot_db::queries::RunProgressQueries::record(
                    db.pool(),
                    Some(run.id),
                    Some(run.thread_id),
                    "watchdog_stalled",
                    serde_json::json!({ "reason": &stalled }),
                )
                .await;
                Err(crate::RuntimeError::TaskFailed(stalled))
            }
        };
        runtime.review_approvals_for_run(run.id, false);

        // Get the response
        let messages = ravenbot_db::queries::MessageQueries::list_by_thread(db.pool(), thread_id)
            .await
            .map_err(|e| e.to_string())?;

        let response = messages
            .iter()
            .rev()
            .find(|m| m.role == ravenbot_core::MessageRole::Assistant)
            .and_then(|m| match &m.content {
                ravenbot_core::MessageContent::Text { text, .. } => Some(text.clone()),
                _ => None,
            });

        // Validate typed output before the node can be marked done. Invalid
        // output is a failure (and therefore retryable) because dependents
        // receive the canonical JSON, not the model's prose.
        let typed_output: Option<Result<String, String>> =
            match (&output_schema, response.as_deref()) {
                (Some(schema), Some(text)) if !text.trim().is_empty() => {
                    Some(extract_json_answer(text).and_then(|value| {
                        validate_against_schema(schema, &value)?;
                        serde_json::to_string(&value)
                            .map_err(|error| format!("the answer could not be serialized: {error}"))
                    }))
                }
                _ => None,
            };
        let typed_error = typed_output
            .as_ref()
            .and_then(|result| result.as_ref().err().cloned());

        // Update graph node. A run that had to be wrapped up at the tool limit
        // carries `PartialSuccess`: it may have a useful progress report, but
        // its dependents must not build on an artifact that was never finished.
        let partial = matches!(
            run.outcome.as_ref(),
            Some(ravenbot_core::RunOutcome::PartialSuccess { .. })
                | Some(ravenbot_core::RunOutcome::Failure { .. })
        );
        let finished_event = {
            let mut g = graph.lock().await;
            match result {
                Ok(_)
                    if !partial
                        && typed_error.is_none()
                        && response
                            .as_deref()
                            .map(str::trim)
                            .is_some_and(|text| !text.is_empty()) =>
                {
                    let text = typed_output
                        .and_then(Result::ok)
                        .unwrap_or_else(|| response.unwrap_or_default());
                    let preview = preview_chars(&text, 160);
                    if reports_incomplete_work(&text) {
                        // The model exited cleanly but told us the artifact was
                        // never filed. Keep its progress report visible, but
                        // fail the node so dependents cannot build on it and
                        // the completion review cannot call the cycle done.
                        let message = "The node reported its own task as incomplete; its report is a progress note, not a completed artifact.".to_string();
                        if let Some(node) = g.nodes.get_mut(&node_id) {
                            node.output = Some(text);
                        }
                        let skipped_ids = fail_node_and_skip(&mut g, node_id, message.clone())?;
                        tracing::warn!(node_id = %node_id, "Node reported incomplete work; failing the node");
                        serde_json::json!({
                            "kind": "node_finished",
                            "node_id": node_id.to_string(),
                            "bot_id": bot_id.to_string(),
                            "state": "failed",
                            "preview": preview_chars(&message, 160),
                            "skipped": skipped_ids,
                        })
                    } else {
                        match g
                            .mark_loop_iteration(node_id, &text)
                            .map_err(|e| e.to_string())?
                        {
                            Some(next_iteration) => {
                                tracing::info!(
                                    node_id = %node_id,
                                    iteration = next_iteration,
                                    "Node looping"
                                );
                                serde_json::json!({
                                    "kind": "node_iteration",
                                    "node_id": node_id.to_string(),
                                    "bot_id": bot_id.to_string(),
                                    "iteration": next_iteration,
                                    "max_iterations": max_iterations,
                                    "preview": preview,
                                })
                            }
                            None => {
                                g.mark_done(node_id, text).map_err(|e| e.to_string())?;
                                tracing::info!(node_id = %node_id, "Node completed");
                                serde_json::json!({
                                    "kind": "node_finished",
                                    "node_id": node_id.to_string(),
                                    "bot_id": bot_id.to_string(),
                                    "state": "done",
                                    "preview": preview,
                                })
                            }
                        }
                    }
                }
                Ok(_) if !partial && typed_error.is_some() => {
                    let message = format!(
                        "The node's answer was not accepted: {}",
                        typed_error.unwrap_or_default()
                    );
                    tracing::warn!(node_id = %node_id, reason = %message, "Node output failed schema validation");
                    let skipped_ids = fail_node_and_skip(&mut g, node_id, message.clone())?;
                    serde_json::json!({
                        "kind": "node_finished",
                        "node_id": node_id.to_string(),
                        "bot_id": bot_id.to_string(),
                        "state": "failed",
                        "preview": preview_chars(&message, 160),
                        "skipped": skipped_ids,
                    })
                }
                Ok(_) if partial => {
                    let reason = match run.outcome.as_ref() {
                        Some(ravenbot_core::RunOutcome::PartialSuccess { errors, .. })
                            if !errors.is_empty() =>
                        {
                            errors.join("; ")
                        }
                        _ => "the run did not complete successfully".to_string(),
                    };
                    let message = format!(
                        "The node produced a progress report but did not finish the task ({reason}). See its reply for what was done and what remains."
                    );
                    tracing::warn!(node_id = %node_id, reason = %reason, "Node ended partial");
                    let skipped_ids = fail_node_and_skip(&mut g, node_id, message.clone())?;
                    serde_json::json!({
                        "kind": "node_finished",
                        "node_id": node_id.to_string(),
                        "bot_id": bot_id.to_string(),
                        "state": "failed",
                        "preview": preview_chars(&message, 160),
                        "skipped": skipped_ids,
                    })
                }
                Ok(_) => {
                    // A node that produced no answer did not do the job its
                    // dependents were told to read. Marking it Done with the
                    // old "No response" preview made the board lie and let the
                    // next node run against a file that was never written.
                    let err_text = "The node finished without producing a result.".to_string();
                    tracing::error!(node_id = %node_id, "Node finished without a result");
                    let skipped_ids = fail_node_and_skip(&mut g, node_id, err_text.clone())?;
                    serde_json::json!({
                        "kind": "node_finished",
                        "node_id": node_id.to_string(),
                        "bot_id": bot_id.to_string(),
                        "state": "failed",
                        "preview": err_text,
                        "skipped": skipped_ids,
                    })
                }
                Err(e) => {
                    let err_text = e.to_string();
                    tracing::error!(node_id = %node_id, error = %e, "Node failed");
                    let skipped_ids = fail_node_and_skip(&mut g, node_id, err_text.clone())?;
                    serde_json::json!({
                        "kind": "node_finished",
                        "node_id": node_id.to_string(),
                        "bot_id": bot_id.to_string(),
                        "state": "failed",
                        "preview": preview_chars(&err_text, 160),
                        "skipped": skipped_ids,
                    })
                }
            }
        };
        emit_node_event(&node_event, finished_event);

        Ok(())
    }
}

/// Link a graph node's thread to the office that owns it.
///
/// Without this row `Runtime::office_id_for_thread` cannot resolve the office,
/// so the node loses the goal/policy/roster/shared memory block and delegation
/// permission checks see an empty roster. The table is keyed by
/// `(chatroom_id, thread_id)`, so re-linking is a no-op.
async fn link_node_thread(
    db: &Database,
    office_id: Uuid,
    thread_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT OR REPLACE INTO chatroom_office_threads (chatroom_id, thread_id, created_at) VALUES (?, ?, ?)",
    )
    .bind(office_id.to_string())
    .bind(thread_id.to_string())
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(db.pool())
    .await?;
    Ok(())
}

/// Mark one node failed and cascade the failure to every dependent.
///
/// The three failure paths (partial run, empty answer, provider error) must
/// leave the graph in the same shape: failed card, skipped dependents, and a
/// preview that a human can act on. Keeping this in one place is what stopped
/// a partial answer from being displayed as Done.
fn fail_node_and_skip(
    graph: &mut TaskGraph,
    node_id: Uuid,
    message: String,
) -> Result<Vec<String>, String> {
    graph.mark_failed(node_id, message)?;
    let dependents: Vec<Uuid> = graph
        .edges
        .iter()
        .filter(|(from, _)| *from == node_id)
        .map(|(_, to)| *to)
        .collect();
    let mut skipped = Vec::new();
    for dep_id in dependents {
        let _ = graph.mark_skipped(dep_id);
        skipped.push(dep_id.to_string());
    }
    Ok(skipped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ravenbot_core::{Bot, ChatRoom, Thread};
    use ravenbot_models::{ModelProviderTrait, ModelResponse, Usage};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Returns a draft on the first loop iteration and an approved answer on
    /// the second. Only task prompts are counted, so unrelated runtime calls
    /// cannot change the script.
    struct LoopProvider {
        task_rounds: AtomicUsize,
    }

    /// Runs a two-node group twice: the terminal answer is REVISE on the first
    /// pass and APPROVED on the second.
    struct GroupProvider {
        calls: AtomicUsize,
    }

    impl GroupProvider {
        fn new() -> Self {
            Self {
                calls: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait::async_trait]
    impl ModelProviderTrait for GroupProvider {
        fn provider_type(&self) -> ravenbot_core::ModelProvider {
            ravenbot_core::ModelProvider::OpenRouter
        }
        fn with_model(self: Box<Self>, _model: String) -> Box<dyn ModelProviderTrait> {
            self
        }
        async fn complete(
            &self,
            _messages: &[ravenbot_models::Message],
            _tools: &[ravenbot_models::ToolDefinition],
            _temperature: f32,
            _max_tokens: u32,
        ) -> Result<ModelResponse, ravenbot_models::ModelError> {
            let round = self.calls.fetch_add(1, Ordering::SeqCst);
            let text = match round {
                0 => "first pass",
                1 => "REVISE",
                2 => "second pass",
                _ => "APPROVED",
            };
            Ok(ModelResponse {
                content: Some(text.to_string()),
                tool_calls: Vec::new(),
                usage: Usage {
                    input_tokens: 5,
                    output_tokens: 3,
                },
                reasoning: None,
            })
        }
        async fn health_check(&self) -> Result<bool, ravenbot_models::ModelError> {
            Ok(true)
        }
    }

    impl LoopProvider {
        fn new() -> Self {
            Self {
                task_rounds: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait::async_trait]
    impl ModelProviderTrait for LoopProvider {
        fn provider_type(&self) -> ravenbot_core::ModelProvider {
            ravenbot_core::ModelProvider::OpenRouter
        }

        fn with_model(self: Box<Self>, _model_id: String) -> Box<dyn ModelProviderTrait> {
            self
        }

        async fn complete(
            &self,
            messages: &[ravenbot_models::Message],
            _tools: &[ravenbot_models::ToolDefinition],
            _temperature: f32,
            _max_tokens: u32,
        ) -> Result<ModelResponse, ravenbot_models::ModelError> {
            let is_task = messages
                .iter()
                .rev()
                .find(|message| message.role == "user")
                .is_some_and(|message| message.content.contains("Refine this draft"));
            let text = if is_task {
                let round = self.task_rounds.fetch_add(1, Ordering::SeqCst);
                if round == 0 {
                    "draft one: REVISE"
                } else {
                    "draft two: APPROVED"
                }
            } else {
                "noted"
            };
            Ok(ModelResponse {
                content: Some(text.to_string()),
                tool_calls: Vec::new(),
                usage: Usage {
                    input_tokens: 10,
                    output_tokens: 5,
                },
                reasoning: None,
            })
        }

        async fn health_check(&self) -> Result<bool, ravenbot_models::ModelError> {
            Ok(true)
        }
    }

    #[tokio::test]
    async fn a_node_thread_is_linked_to_its_office() {
        let path = std::env::temp_dir().join(format!("rb-executor-{}.db", Uuid::new_v4()));
        let db = Database::new(&path).await.expect("temp db");
        let bot = Bot::new("Node", "test");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &bot)
            .await
            .unwrap();
        let room = ChatRoom::new("Office", "test", "custom");
        ravenbot_db::queries::ChatRoomQueries::create(db.pool(), &room)
            .await
            .unwrap();
        let thread = Thread::new(bot.id, "node work");
        ravenbot_db::queries::ThreadQueries::create(db.pool(), &thread)
            .await
            .unwrap();

        link_node_thread(&db, room.id, thread.id).await.unwrap();
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM chatroom_office_threads WHERE chatroom_id = ? AND thread_id = ?",
        )
        .bind(room.id.to_string())
        .bind(thread.id.to_string())
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(count, 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn extracts_plain_fenced_and_embedded_json_answers() {
        assert_eq!(
            extract_json_answer(r#"{"status":"ok"}"#).unwrap(),
            serde_json::json!({"status": "ok"})
        );
        assert_eq!(
            extract_json_answer("```json\n{\"items\":[1,2]}\n```").unwrap(),
            serde_json::json!({"items": [1, 2]})
        );
        assert_eq!(
            extract_json_answer("Here is the result:\n```\n[true]\n```\nDone.").unwrap(),
            serde_json::json!([true])
        );
        assert_eq!(
            extract_json_answer("Result: {\"count\": 3} — thanks").unwrap(),
            serde_json::json!({"count": 3})
        );
        assert!(extract_json_answer("No JSON here.").is_err());
        assert!(extract_json_answer("   ").is_err());
    }

    #[test]
    fn schema_validation_accepts_matching_and_rejects_mismatched_output() {
        let schema = serde_json::json!({
            "type": "object",
            "required": ["status"],
            "properties": {
                "status": { "type": "string", "enum": ["ok", "failed"] },
                "count": { "type": "integer" }
            },
            "additionalProperties": false
        });
        assert!(
            validate_against_schema(&schema, &serde_json::json!({"status": "ok", "count": 2}))
                .is_ok()
        );
        let missing = validate_against_schema(&schema, &serde_json::json!({"count": 2}))
            .expect_err("missing status must fail");
        assert!(missing.contains("output schema"));
        assert!(validate_against_schema(&schema, &serde_json::json!("ok")).is_err());
        assert!(validate_against_schema(
            &serde_json::json!({"type": "not-a-type"}),
            &serde_json::json!({})
        )
        .is_err());
    }

    #[test]
    fn progress_stall_boundary_is_exact_and_duration_based() {
        let now = chrono::Utc::now();
        assert!(!progress_stalled(
            now,
            now,
            std::time::Duration::from_secs(60)
        ));
        assert!(progress_stalled(
            now - chrono::Duration::seconds(60),
            now,
            std::time::Duration::from_secs(60)
        ));
        assert!(!progress_stalled(
            now - chrono::Duration::seconds(59),
            now,
            std::time::Duration::from_secs(60)
        ));
        assert!(progress_stalled(
            now - chrono::Duration::minutes(5),
            now,
            std::time::Duration::from_secs(60)
        ));
    }

    struct ResumeProvider;

    #[async_trait::async_trait]
    impl ModelProviderTrait for ResumeProvider {
        fn provider_type(&self) -> ravenbot_core::ModelProvider {
            ravenbot_core::ModelProvider::OpenRouter
        }
        fn with_model(self: Box<Self>, _model: String) -> Box<dyn ModelProviderTrait> {
            self
        }
        async fn complete(
            &self,
            _messages: &[ravenbot_models::Message],
            _tools: &[ravenbot_models::ToolDefinition],
            _temperature: f32,
            _max_tokens: u32,
        ) -> Result<ModelResponse, ravenbot_models::ModelError> {
            Ok(ModelResponse {
                content: Some("resumed answer".to_string()),
                tool_calls: Vec::new(),
                usage: Usage {
                    input_tokens: 7,
                    output_tokens: 3,
                },
                reasoning: None,
            })
        }
        async fn health_check(&self) -> Result<bool, ravenbot_models::ModelError> {
            Ok(true)
        }
    }

    struct IncompleteProvider {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl ModelProviderTrait for IncompleteProvider {
        fn provider_type(&self) -> ravenbot_core::ModelProvider {
            ravenbot_core::ModelProvider::OpenRouter
        }
        fn with_model(self: Box<Self>, _model: String) -> Box<dyn ModelProviderTrait> {
            self
        }
        async fn complete(
            &self,
            _messages: &[ravenbot_models::Message],
            _tools: &[ravenbot_models::ToolDefinition],
            _temperature: f32,
            _max_tokens: u32,
        ) -> Result<ModelResponse, ravenbot_models::ModelError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(ModelResponse {
                content: Some(
                    "## Status: incomplete — the milestone was decided, but NOT filed\n\n                     No file was created or modified. Treat it as unfiled."
                        .to_string(),
                ),
                tool_calls: Vec::new(),
                usage: Usage {
                    input_tokens: 12,
                    output_tokens: 8,
                },
                reasoning: None,
            })
        }
        async fn health_check(&self) -> Result<bool, ravenbot_models::ModelError> {
            Ok(true)
        }
    }

    #[tokio::test]
    async fn a_self_reported_incomplete_node_fails_and_blocks_its_dependent() {
        let path = std::env::temp_dir().join(format!("rb-incomplete-{}.db", Uuid::new_v4()));
        let db = Database::new(&path).await.expect("temp db");
        let bot = Bot::new("Reporter", "self-report test");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &bot)
            .await
            .unwrap();
        let office = Uuid::new_v4();
        let workspace = std::env::temp_dir().join(format!("rb-incomplete-ws-{}", Uuid::new_v4()));
        std::fs::create_dir_all(workspace.join("Shared")).unwrap();

        let calls = Arc::new(AtomicUsize::new(0));
        let runtime = Arc::new(Runtime::new(db.clone()));
        runtime
            .set_provider_override(Some(Arc::new(IncompleteProvider {
                calls: calls.clone(),
            }) as Arc<dyn ModelProviderTrait>))
            .await;
        runtime.set_auto_allow_approvals(true);

        let mut graph = TaskGraph::new("incomplete");
        let first = graph.add_node(bot.id, "Write the status report");
        let second = graph.add_node(bot.id, "Ship the report");
        graph.add_edge(first, second);
        WorkLedger::new(db.pool().clone(), office)
            .seed(&graph, 1)
            .await
            .expect("seed");
        let graph = Arc::new(Mutex::new(graph));

        GraphExecutor::new(runtime, db.clone())
            .with_office(office)
            .with_working_dirs(vec![workspace.to_string_lossy().to_string()])
            .execute(graph.clone())
            .await
            .expect("graph executes");

        let snapshot = graph.lock().await.clone();
        assert!(
            matches!(snapshot.nodes[&first].state, NodeState::Failed(_)),
            "{:?}",
            snapshot.nodes[&first].state
        );
        assert!(
            matches!(snapshot.nodes[&second].state, NodeState::Skipped),
            "{:?}",
            snapshot.nodes[&second].state
        );
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "a dependent must not run after an incomplete milestone"
        );
        assert!(
            snapshot.nodes[&first]
                .output
                .as_deref()
                .is_some_and(|text| text.contains("Status: incomplete")),
            "the progress report stays visible for the next cycle"
        );

        let items = ravenbot_db::queries::WorkItemQueries::list_by_office(db.pool(), office, 10)
            .await
            .expect("work items");
        let first_item = items
            .iter()
            .find(|item| item.id == first.to_string())
            .expect("first item");
        assert_eq!(first_item.status, "failed");
        let second_item = items
            .iter()
            .find(|item| item.id == second.to_string())
            .expect("second item");
        assert_eq!(second_item.status, "skipped");

        let status = std::fs::read_to_string(workspace.join("Shared/STATUS.md"))
            .expect("STATUS.md projection");
        assert!(status.contains("| failed |"), "{status}");

        drop(db);
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_dir_all(workspace);
    }

    #[tokio::test]
    async fn an_office_graph_node_records_a_durable_work_item() {
        let path = std::env::temp_dir().join(format!("rb-ledger-{}.db", Uuid::new_v4()));
        let db = Database::new(&path).await.expect("temp db");
        let bot = Bot::new("Ledger", "work ledger test");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &bot)
            .await
            .unwrap();
        let thread = Thread::new(bot.id, "ledger node");
        ravenbot_db::queries::ThreadQueries::create(db.pool(), &thread)
            .await
            .unwrap();
        let office = Uuid::new_v4();
        let workspace = std::env::temp_dir().join(format!("rb-ledger-ws-{}", Uuid::new_v4()));
        std::fs::create_dir_all(workspace.join("Shared")).unwrap();

        let runtime = Arc::new(Runtime::new(db.clone()));
        runtime
            .set_provider_override(Some(Arc::new(ResumeProvider) as Arc<dyn ModelProviderTrait>))
            .await;
        runtime.set_auto_allow_approvals(true);

        let mut graph = TaskGraph::new("ledger");
        let node = graph.add_node(bot.id, "Write the status report");
        graph.nodes.get_mut(&node).unwrap().thread_id = Some(thread.id);
        WorkLedger::new(db.pool().clone(), office)
            .seed(&graph, 1)
            .await
            .expect("seed");
        let graph = Arc::new(Mutex::new(graph));

        GraphExecutor::new(runtime, db.clone())
            .with_office(office)
            .with_working_dirs(vec![workspace.to_string_lossy().to_string()])
            .execute(graph)
            .await
            .expect("graph executes");

        let items = ravenbot_db::queries::WorkItemQueries::list_by_office(db.pool(), office, 10)
            .await
            .expect("work items");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, node.to_string());
        assert_eq!(items[0].status, "done");
        assert_eq!(items[0].result.as_deref(), Some("resumed answer"));
        assert_eq!(
            items[0].thread_id.as_deref(),
            Some(thread.id.to_string().as_str())
        );
        assert_eq!(items[0].attempts, 1);

        let detail = ravenbot_db::queries::WorkItemQueries::detail(db.pool(), &node.to_string())
            .await
            .expect("detail")
            .expect("record");
        let kinds: Vec<&str> = detail
            .events
            .iter()
            .map(|event| event.kind.as_str())
            .collect();
        assert!(kinds.contains(&"claimed"), "{kinds:?}");
        assert!(kinds.contains(&"completed"), "{kinds:?}");

        let status = std::fs::read_to_string(workspace.join("Shared/STATUS.md"))
            .expect("STATUS.md projection");
        assert!(status.contains("Work ledger"), "{status}");
        assert!(status.contains("Write the status report"), "{status}");
        assert!(status.contains("| done |"), "{status}");

        drop(db);
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_dir_all(workspace);
    }

    #[tokio::test]
    async fn an_interrupted_thread_without_a_checkpoint_does_not_repeat_its_instruction() {
        let path = std::env::temp_dir().join(format!("rb-resume-note-{}.db", Uuid::new_v4()));
        let db = Database::new(&path).await.expect("temp db");
        let bot = Bot::new("Resumer", "resume note test");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &bot)
            .await
            .unwrap();
        let thread = Thread::new(bot.id, "interrupted without checkpoint");
        ravenbot_db::queries::ThreadQueries::create(db.pool(), &thread)
            .await
            .unwrap();
        ravenbot_db::queries::MessageQueries::insert(
            db.pool(),
            &ravenbot_core::Message::user(thread.id, "Write the status report"),
        )
        .await
        .unwrap();

        let runtime = Arc::new(Runtime::new(db.clone()));
        runtime
            .set_provider_override(Some(Arc::new(ResumeProvider) as Arc<dyn ModelProviderTrait>))
            .await;
        runtime.set_auto_allow_approvals(true);

        let mut graph = TaskGraph::new("resume without checkpoint");
        let node = graph.add_node(bot.id, "Write the status report");
        graph.nodes.get_mut(&node).unwrap().thread_id = Some(thread.id);
        let graph = Arc::new(Mutex::new(graph));
        GraphExecutor::new(runtime, db.clone())
            .with_resume_threads(std::collections::HashSet::from([thread.id]))
            .execute(graph)
            .await
            .expect("graph executes");

        let user_messages: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM messages WHERE thread_id = ? AND role = 'user'",
        )
        .bind(thread.id.to_string())
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(
            user_messages, 1,
            "a resumed thread must not repeat its instruction"
        );

        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn an_interrupted_node_resumes_its_original_run_and_checkpoint() {
        let path = std::env::temp_dir().join(format!("rb-resume-{}.db", Uuid::new_v4()));
        let db = Database::new(&path).await.expect("temp db");
        let bot = Bot::new("Resumer", "resume test");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &bot)
            .await
            .unwrap();
        let thread = Thread::new(bot.id, "interrupted node");
        ravenbot_db::queries::ThreadQueries::create(db.pool(), &thread)
            .await
            .unwrap();
        let original = ravenbot_core::Message::user(thread.id, "original task");
        ravenbot_db::queries::MessageQueries::insert(db.pool(), &original)
            .await
            .unwrap();

        let mut interrupted = Run::new(bot.id, thread.id);
        interrupted.state = ravenbot_core::RunState::Paused;
        interrupted.checkpoint(serde_json::json!({
            "model_messages": serde_json::to_value(vec![
                ravenbot_models::Message::text("system", "system"),
                ravenbot_models::Message::text("user", "original task"),
            ])
            .unwrap(),
            "rounds": 0u32,
        }));
        ravenbot_db::queries::RunQueries::insert(db.pool(), &interrupted)
            .await
            .unwrap();

        let runtime = Arc::new(Runtime::new(db.clone()));
        runtime
            .set_provider_override(Some(Arc::new(ResumeProvider) as Arc<dyn ModelProviderTrait>))
            .await;
        runtime.set_auto_allow_approvals(true);

        let mut graph = TaskGraph::new("resume");
        let node = graph.add_node(bot.id, "original task");
        graph.nodes.get_mut(&node).unwrap().thread_id = Some(thread.id);
        graph.mark_running(node, interrupted.id).unwrap();
        graph.prepare_resume();
        let graph = Arc::new(Mutex::new(graph));
        let mut resume_runs = std::collections::HashMap::new();
        resume_runs.insert(node, interrupted.clone());

        let blackboard = GraphExecutor::new(runtime, db.clone())
            .with_resume_runs(resume_runs)
            .execute(graph.clone())
            .await
            .expect("resumed graph executes");

        assert_eq!(
            blackboard.get(&node.to_string()),
            Some("resumed answer"),
            "the checkpointed run must produce the answer"
        );
        let snapshot = graph.lock().await.clone();
        assert!(matches!(
            snapshot.nodes[&node].state,
            crate::graph::NodeState::Done(_)
        ));
        let run_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM runs")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(run_count, 1, "resume must not create a second run");
        let user_messages: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM messages WHERE thread_id = ? AND role = 'user'",
        )
        .bind(thread.id.to_string())
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(
            user_messages, 1,
            "resume must not duplicate the original instruction"
        );
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn a_looping_node_reuses_one_thread_and_publishes_the_final_answer() {
        let path = std::env::temp_dir().join(format!("rb-loop-{}.db", Uuid::new_v4()));
        let db = Database::new(&path).await.expect("temp db");
        let bot = Bot::new("Looper", "loop test");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &bot)
            .await
            .unwrap();
        let runtime = Arc::new(Runtime::new(db.clone()));
        let provider = Arc::new(LoopProvider::new());
        runtime
            .set_provider_override(Some(provider.clone() as Arc<dyn ModelProviderTrait>))
            .await;
        runtime.set_auto_allow_approvals(true);

        let mut graph = TaskGraph::new("loop");
        let node = graph.add_node(bot.id, "Refine this draft");
        graph
            .set_loop(
                node,
                3,
                Some(crate::graph::LoopStop {
                    operator: crate::graph::ConditionOperator::Contains,
                    value: "APPROVED".to_string(),
                }),
            )
            .unwrap();
        let graph = Arc::new(Mutex::new(graph));
        let blackboard = GraphExecutor::new(runtime, db.clone())
            .execute(graph.clone())
            .await
            .expect("graph executes");

        assert_eq!(
            blackboard.get(&node.to_string()),
            Some("draft two: APPROVED")
        );
        let snapshot = graph.lock().await.clone();
        assert_eq!(snapshot.nodes[&node].iterations, 2);
        assert_eq!(snapshot.nodes[&node].attempts, 1);
        let threads = ravenbot_db::queries::ThreadQueries::list_by_bot(db.pool(), bot.id)
            .await
            .unwrap();
        assert_eq!(threads.len(), 1, "loop iterations share one node thread");
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn an_early_node_error_terminalizes_the_node_and_skips_dependents() {
        let path = std::env::temp_dir().join(format!("rb-early-{}.db", Uuid::new_v4()));
        let db = Database::new(&path).await.expect("temp db");
        let runtime = Arc::new(Runtime::new(db.clone()));
        let bot_id = Uuid::new_v4(); // never inserted
        let mut graph = TaskGraph::new("early failure");
        let first = graph.add_node(bot_id, "Will fail before it starts");
        let second = graph.add_node(bot_id, "Dependent");
        graph.add_edge(first, second);
        let graph = Arc::new(Mutex::new(graph));

        GraphExecutor::new(runtime, db.clone())
            .execute(graph.clone())
            .await
            .expect("the graph itself completes");

        let snapshot = graph.lock().await.clone();
        assert!(matches!(
            snapshot.nodes[&first].state,
            crate::graph::NodeState::Failed(_)
        ));
        assert!(matches!(
            snapshot.nodes[&second].state,
            crate::graph::NodeState::Skipped
        ));
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn a_multi_task_loop_group_runs_until_its_stop_phrase() {
        let path = std::env::temp_dir().join(format!("rb-group-{}.db", Uuid::new_v4()));
        let db = Database::new(&path).await.expect("temp db");
        let bot = Bot::new("Group Worker", "group test");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &bot)
            .await
            .unwrap();
        let runtime = Arc::new(Runtime::new(db.clone()));
        runtime
            .set_provider_override(Some(
                Arc::new(GroupProvider::new()) as Arc<dyn ModelProviderTrait>
            ))
            .await;
        runtime.set_auto_allow_approvals(true);

        let mut graph = TaskGraph::new("group");
        let first = graph.add_node(bot.id, "first pass");
        let second = graph.add_node(bot.id, "review pass");
        graph.add_edge(first, second);
        graph
            .add_loop_group(
                "refine",
                vec![first, second],
                second,
                3,
                Some(crate::graph::LoopStop {
                    operator: crate::graph::ConditionOperator::Contains,
                    value: "APPROVED".to_string(),
                }),
            )
            .unwrap();
        let graph = Arc::new(Mutex::new(graph));
        let blackboard = GraphExecutor::new(runtime, db.clone())
            .execute(graph.clone())
            .await
            .expect("graph executes");

        assert_eq!(
            blackboard.get(&second.to_string()),
            Some("APPROVED"),
            "the terminal answer is published"
        );
        assert_eq!(
            blackboard.get(&first.to_string()),
            Some("second pass"),
            "the group's first task also ran again"
        );
        let snapshot = graph.lock().await.clone();
        assert_eq!(snapshot.loop_groups.len(), 1);
        assert_eq!(snapshot.loop_groups[0].iterations, 2);
        assert!(snapshot.loop_groups[0].finished);
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn trigger_and_output_nodes_run_without_a_model_call() {
        let path = std::env::temp_dir().join(format!("rb-types-{}.db", Uuid::new_v4()));
        let db = Database::new(&path).await.expect("temp db");
        let bot = Bot::new("Type Bot", "type test");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &bot)
            .await
            .unwrap();
        let runtime = Arc::new(Runtime::new(db.clone()));
        let mut graph = TaskGraph::new("typed graph");
        let trigger = graph.add_node(bot.id, "Kick off the release");
        graph
            .set_node_type(trigger, WorkflowNodeType::Trigger)
            .unwrap();
        let output = graph.add_node(bot.id, "Deliver the result");
        graph
            .set_node_type(output, WorkflowNodeType::Output)
            .unwrap();
        graph.add_edge(trigger, output);
        let graph = Arc::new(Mutex::new(graph));
        let blackboard = GraphExecutor::new(runtime, db.clone())
            .execute(graph.clone())
            .await
            .expect("graph executes");

        assert!(blackboard
            .get(&output.to_string())
            .is_some_and(|text| text.contains("Kick off the release")));
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn an_llm_node_uses_one_completion_without_the_tool_loop() {
        let path = std::env::temp_dir().join(format!("rb-llm-node-{}.db", Uuid::new_v4()));
        let db = Database::new(&path).await.expect("temp db");
        let bot = Bot::new("LLM Node", "llm test");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &bot)
            .await
            .unwrap();
        let runtime = Arc::new(Runtime::new(db.clone()));
        runtime
            .set_provider_override(Some(
                Arc::new(GroupProvider::new()) as Arc<dyn ModelProviderTrait>
            ))
            .await;
        let mut graph = TaskGraph::new("llm");
        let node = graph.add_node(bot.id, "Write one sentence");
        graph.set_node_type(node, WorkflowNodeType::Llm).unwrap();
        let graph = Arc::new(Mutex::new(graph));
        let blackboard = GraphExecutor::new(runtime, db.clone())
            .execute(graph.clone())
            .await
            .expect("graph executes");
        assert_eq!(blackboard.get(&node.to_string()), Some("first pass"));
        let snapshot = graph.lock().await.clone();
        let thread_id = snapshot.nodes[&node].thread_id.expect("node thread");
        let messages = ravenbot_db::queries::MessageQueries::list_by_thread(db.pool(), thread_id)
            .await
            .unwrap();
        assert!(messages
            .iter()
            .any(|message| message.role == ravenbot_core::MessageRole::Assistant));
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn an_approval_node_waits_for_the_user() {
        let path = std::env::temp_dir().join(format!("rb-approval-node-{}.db", Uuid::new_v4()));
        let db = Database::new(&path).await.expect("temp db");
        let bot = Bot::new("Approval Node", "approval test");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &bot)
            .await
            .unwrap();
        let runtime = Arc::new(Runtime::new(db.clone()));
        let mut graph = TaskGraph::new("approval");
        let node = graph.add_node(bot.id, "Ship the release");
        graph
            .set_node_type(node, WorkflowNodeType::Approval)
            .unwrap();
        let graph = Arc::new(Mutex::new(graph));
        let task_graph = graph.clone();
        let task_db = db.clone();
        let handle = tokio::spawn(async move {
            GraphExecutor::new(runtime, task_db)
                .execute(task_graph)
                .await
        });

        let approval_id = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if let Ok(Some((id,))) = sqlx::query_as::<_, (String,)>(
                    "SELECT id FROM approvals WHERE tool_name = 'workflow_approval' AND status = 'pending' LIMIT 1",
                )
                .fetch_optional(db.pool())
                .await
                {
                    return id;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("approval card must appear");
        let approval_id = Uuid::parse_str(&approval_id).unwrap();
        assert!(ravenbot_db::queries::ApprovalQueries::decide(
            db.pool(),
            approval_id,
            true,
            Some("approved in test"),
        )
        .await
        .unwrap());
        handle.await.expect("task joins").expect("graph executes");
        let snapshot = graph.lock().await.clone();
        assert!(matches!(snapshot.nodes[&node].state, NodeState::Done(_)));
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn failing_a_node_cascades_to_its_dependents() {
        let mut graph = TaskGraph::new("goal");
        let first = graph.add_node(Uuid::new_v4(), "first");
        let second = graph.add_node(Uuid::new_v4(), "second");
        graph.add_edge(first, second);

        let skipped =
            fail_node_and_skip(&mut graph, first, "partial report".to_string()).expect("fail node");

        assert_eq!(skipped, vec![second.to_string()]);
        assert!(matches!(
            graph.nodes[&first].state,
            crate::graph::NodeState::Failed(_)
        ));
        assert!(matches!(
            graph.nodes[&second].state,
            crate::graph::NodeState::Skipped
        ));
    }
}
