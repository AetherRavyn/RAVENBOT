//! Graph executor for parallel task execution
//!
//! This module executes a task graph, running ready nodes in parallel
//! and managing the shared blackboard.

use crate::graph::{TaskGraph, Blackboard};
use crate::Runtime;
use ravenbot_core::Run;
use ravenbot_db::Database;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Executor for running task graphs
pub struct GraphExecutor {
    runtime: Arc<Runtime>,
    db: Database,
    /// Project folders every node in this graph works in (JSON array). Stamped
    /// onto each node thread so the runtime confines tool work correctly.
    working_dirs: Vec<String>,
}

impl GraphExecutor {
    /// Create a new executor
    pub fn new(runtime: Arc<Runtime>, db: Database) -> Self {
        Self { runtime, db, working_dirs: Vec::new() }
    }

    /// Set the project folders every node thread inherits.
    pub fn with_working_dirs(mut self, dirs: Vec<String>) -> Self {
        self.working_dirs = dirs;
        self
    }

    /// Execute a task graph, running ready tasks in parallel
    pub async fn execute(
        &self,
        graph: Arc<Mutex<TaskGraph>>,
    ) -> Result<Blackboard, String> {
        tracing::info!("Starting graph execution");
        
        loop {
            // Get ready nodes
            let ready_nodes = {
                let mut g = graph.lock().await;
                // Cascade skips first so a failed branch can't strand dependents.
                let skipped = g.propagate_skips();
                if skipped > 0 {
                    tracing::info!(skipped, "Skipped nodes with failed dependencies");
                }
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

                ready.into_iter()
                    .map(|n| {
                        // Prefer live dependency outputs (real DAG data flow) over
                        // any input captured when the node was created.
                        let input = g.input_for(n.id).or_else(|| n.input.clone());
                        (n.id, n.bot_id, n.instruction.clone(), input)
                    })
                    .collect::<Vec<_>>()
            };

            tracing::info!("Running {} ready nodes", ready_nodes.len());

            // Run ready nodes in parallel
            let mut handles = Vec::new();
            for (node_id, bot_id, instruction, input) in ready_nodes {
                let graph = graph.clone();
                let runtime = self.runtime.clone();
                let db = self.db.clone();
                
                let working_dirs = self.working_dirs.clone();
                let handle = tokio::spawn(async move {
                    Self::execute_node(graph, runtime, db, node_id, bot_id, instruction, input, working_dirs).await
                });
                handles.push(handle);
            }

            // Wait for all to complete
            for handle in handles {
                if let Err(e) = handle.await {
                    tracing::error!("Task execution error: {}", e);
                }
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
    ) -> Result<(), String> {
        // Mark as running
        {
            let mut g = graph.lock().await;
            let run_id = Uuid::new_v4();
            g.mark_running(node_id, run_id)
                .map_err(|e| e.to_string())?;
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
            let input_msg = ravenbot_core::Message::user(thread.id, format!(
                "Input from previous task:\n\n{}", input
            ));
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
        // can never hang the whole office indefinitely.
        let node_timeout_secs = std::env::var("RAVENBOT_NODE_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(300);
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

        let response = messages.iter()
            .rev()
            .find(|m| m.role == ravenbot_core::MessageRole::Assistant)
            .and_then(|m| match &m.content {
                ravenbot_core::MessageContent::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .unwrap_or_else(|| "No response".to_string());

        // Update graph node
        {
            let mut g = graph.lock().await;
            match result {
                Ok(_) => {
                    g.mark_done(node_id, response)
                        .map_err(|e| e.to_string())?;
                    tracing::info!(node_id = %node_id, "Node completed");
                }
                Err(e) => {
                    g.mark_failed(node_id, e.to_string())
                        .map_err(|e| e.to_string())?;
                    tracing::error!(node_id = %node_id, error = %e, "Node failed");
                    
                    // Skip dependent nodes
                    let dependents: Vec<Uuid> = g.edges.iter()
                        .filter(|(from, _)| *from == node_id)
                        .map(|(_, to)| *to)
                        .collect();
                    
                    for dep_id in dependents {
                        let _ = g.mark_skipped(dep_id);
                    }
                }
            }
        }

        Ok(())
    }
}
