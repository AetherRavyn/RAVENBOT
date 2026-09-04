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
}

impl GraphExecutor {
    /// Create a new executor
    pub fn new(runtime: Arc<Runtime>, db: Database) -> Self {
        Self { runtime, db }
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
                let g = graph.lock().await;
                let ready = g.ready_nodes();
                
                if ready.is_empty() {
                    if g.is_complete() {
                        tracing::info!("Graph execution complete");
                        break;
                    }
                    if g.has_deadlock() {
                        return Err("Deadlock detected in task graph".to_string());
                    }
                    // Wait for running tasks to complete
                    drop(g);
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    continue;
                }
                
                ready.into_iter()
                    .map(|n| (n.id, n.bot_id, n.instruction.clone(), n.input.clone()))
                    .collect::<Vec<_>>()
            };

            tracing::info!("Running {} ready nodes", ready_nodes.len());

            // Run ready nodes in parallel
            let mut handles = Vec::new();
            for (node_id, bot_id, instruction, input) in ready_nodes {
                let graph = graph.clone();
                let runtime = self.runtime.clone();
                let db = self.db.clone();
                
                let handle = tokio::spawn(async move {
                    Self::execute_node(graph, runtime, db, node_id, bot_id, instruction, input).await
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

        // Execute the run
        let result = runtime.execute_run(&mut run).await;
        
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
