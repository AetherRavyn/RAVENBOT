//! Graph executor for parallel task execution
//!
//! This module executes a task graph, running ready nodes in parallel
//! and managing the shared blackboard.

use crate::graph::{Blackboard, NodeState, TaskGraph};
use crate::Runtime;
use ravenbot_core::Run;
use ravenbot_db::Database;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

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

fn preview_chars(s: &str, max: usize) -> String {
    let mut out: String = s.chars().take(max).collect();
    if s.chars().count() > max {
        out.push('…');
    }
    out
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
        }
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
            let (ready_nodes, skipped_event) = {
                let mut g = graph.lock().await;
                // Cascade skips first so a failed branch can't strand dependents.
                let skipped = g.propagate_skips();
                if skipped > 0 {
                    tracing::info!(skipped, "Skipped nodes with failed dependencies");
                }
                let skipped_event = if skipped > 0 {
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
                        (n.id, n.bot_id, n.instruction.clone(), input)
                    })
                    .collect::<Vec<_>>();
                (ready, skipped_event)
            };

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
                for (node_id, bot_id, instruction, input) in chunk.iter() {
                    let graph = graph.clone();
                    let runtime = self.runtime.clone();
                    let db = self.db.clone();
                    let node_event = self.node_event.clone();
                    let thread_emitter = self.thread_emitter.clone();
                    let working_dirs = self.working_dirs.clone();
                    let node_id = *node_id;
                    let bot_id = *bot_id;
                    let instruction = instruction.clone();
                    let input = input.clone();
                    let handle = tokio::spawn(async move {
                        Self::execute_node(
                            graph,
                            runtime,
                            db,
                            node_id,
                            bot_id,
                            instruction,
                            input,
                            working_dirs,
                            node_event,
                            thread_emitter,
                        )
                        .await
                    });
                    handles.push(handle);
                }

                for handle in handles {
                    if let Err(e) = handle.await {
                        tracing::error!("Task execution error: {}", e);
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

        // Return the final blackboard state
        let final_blackboard = graph.lock().await.blackboard.clone();
        Ok(final_blackboard)
    }

    /// Execute a single node
    async fn execute_node(
        graph: Arc<Mutex<TaskGraph>>,
        runtime: Arc<Runtime>,
        db: Database,
        node_id: Uuid,
        bot_id: Uuid,
        instruction: String,
        input: Option<String>,
        working_dirs: Vec<String>,
        node_event: Option<NodeEventSink>,
        thread_emitter: Option<ThreadEmitterFactory>,
    ) -> Result<(), String> {
        // Mark as running
        {
            let mut g = graph.lock().await;
            let run_id = Uuid::new_v4();
            g.mark_running(node_id, run_id).map_err(|e| e.to_string())?;
        }

        tracing::info!(
            node_id = %node_id,
            bot_id = %bot_id,
            instruction = %instruction,
            "Executing node"
        );

        // Get bot
        let _bot = ravenbot_db::queries::BotQueries::get(db.pool(), bot_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Bot {} not found", bot_id))?;

        // Create a thread and run for this task
        let thread = ravenbot_core::Thread::new(bot_id, &instruction);
        ravenbot_db::queries::ThreadQueries::create(db.pool(), &thread)
            .await
            .map_err(|e| e.to_string())?;
        // Tell the office board which thread this node streams on, so deltas
        // can be attributed to the right card even for parallel same-bot nodes.
        emit_node_event(
            &node_event,
            serde_json::json!({
                "kind": "node_open",
                "node_id": node_id.to_string(),
                "bot_id": bot_id.to_string(),
                "node_thread_id": thread.id.to_string(),
                "instruction": instruction.as_str(),
            }),
        );
        // Route this node's stream to its own thread. The guard clears the
        // registration on every exit path, including an early `?` or a panic
        // unwinding the task.
        let _emitter_guard = thread_emitter.as_ref().and_then(|factory| {
            factory(thread.id).map(|emitter| {
                runtime.set_thread_emitter(thread.id, Some(emitter));
                ThreadEmitterGuard {
                    runtime: runtime.clone(),
                    thread_id: thread.id,
                }
            })
        });
        // Remember which project folders this node may work in.
        if !working_dirs.is_empty() {
            let json = serde_json::to_string(&working_dirs).unwrap_or_else(|_| "[]".to_string());
            let _ = sqlx::query("UPDATE threads SET project_folders = ? WHERE id = ?")
                .bind(json)
                .bind(thread.id.to_string())
                .execute(db.pool())
                .await;
        }

        // Add input as user message if provided
        if let Some(input) = &input {
            let input_msg = ravenbot_core::Message::user(
                thread.id,
                format!("Input from previous task:\n\n{}", input),
            );
            ravenbot_db::queries::MessageQueries::insert(db.pool(), &input_msg)
                .await
                .map_err(|e| e.to_string())?;
        }

        // Add instruction as user message
        let user_msg = ravenbot_core::Message::user(thread.id, &instruction);
        ravenbot_db::queries::MessageQueries::insert(db.pool(), &user_msg)
            .await
            .map_err(|e| e.to_string())?;

        // Create and execute run
        let mut run = Run::new(bot_id, thread.id);
        ravenbot_db::queries::RunQueries::insert(db.pool(), &run)
            .await
            .map_err(|e| e.to_string())?;

        // No UI watches graph nodes: auto-allow gates (audited) for THIS run
        // only. A global flag would race across the parallel nodes.
        runtime.allow_approvals_for_run(run.id, true);
        // Bound each node so a stuck provider (hung HTTP call, retry storm)
        // can never hang the whole office indefinitely. Office work is long-
        // running, so the default is 30 minutes rather than 5.
        let node_timeout_secs = std::env::var("RAVENBOT_NODE_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(1800);
        let result = match tokio::time::timeout(
            tokio::time::Duration::from_secs(node_timeout_secs),
            runtime.execute_run(&mut run),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => {
                runtime.request_cancel(run.id);
                let _ = runtime.cancel_run(&mut run).await;
                Err(crate::RuntimeError::TaskFailed(format!(
                    "node timed out after {}s (set RAVENBOT_NODE_TIMEOUT_SECS to change)",
                    node_timeout_secs
                )))
            }
        };
        runtime.allow_approvals_for_run(run.id, false);

        // Get the response
        let messages = ravenbot_db::queries::MessageQueries::list_by_thread(db.pool(), thread.id)
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
                        && response
                            .as_deref()
                            .map(str::trim)
                            .is_some_and(|text| !text.is_empty()) =>
                {
                    let text = response.unwrap_or_default();
                    let preview = preview_chars(&text, 160);
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
