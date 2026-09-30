//! Main scheduler for managing routine execution

use crate::routine::RoutineManager;
use crate::trigger::{EventTrigger, TriggerEvent};
use ravenbot_core::Routine;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use uuid::Uuid;

/// Scheduler configuration
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// How often to check for due routines (in seconds)
    pub check_interval_secs: u64,
    /// Maximum concurrent routine executions
    pub max_concurrent: usize,
    /// Timeout for individual routine execution (in seconds)
    pub routine_timeout_secs: u64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            check_interval_secs: 60,
            max_concurrent: 5,
            routine_timeout_secs: 300,
        }
    }
}

/// Callback that actually executes a due routine (send instruction to bot).
/// Installed by the application shell; without it, due routines are only logged.
pub type RoutineExecutor = Arc<
    dyn Fn(Routine) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>>
        + Send
        + Sync,
>;

/// Main scheduler
pub struct Scheduler {
    routine_manager: Arc<RoutineManager>,
    config: SchedulerConfig,
    triggers: Arc<RwLock<Vec<EventTrigger>>>,
    running: Arc<RwLock<bool>>,
    executor: RwLock<Option<RoutineExecutor>>,
}

impl Scheduler {
    pub fn new(routine_manager: Arc<RoutineManager>, config: SchedulerConfig) -> Self {
        Self {
            routine_manager,
            config,
            triggers: Arc::new(RwLock::new(Vec::new())),
            running: Arc::new(RwLock::new(false)),
            executor: RwLock::new(None),
        }
    }

    /// Install the routine executor so due routines really run
    pub async fn set_executor(&self, executor: RoutineExecutor) {
        *self.executor.write().await = Some(executor);
    }

    /// Start the scheduler
    pub async fn start(&self) {
        let mut running = self.running.write().await;
        *running = true;
        drop(running);

        tracing::info!("Scheduler started");

        let mut ticker = interval(Duration::from_secs(self.config.check_interval_secs));

        loop {
            ticker.tick().await;

            // Check if still running
            if !*self.running.read().await {
                break;
            }

            // Check for due routines
            match self.routine_manager.check_schedules().await {
                Ok(due_routines) => {
                    for routine in due_routines {
                        tracing::info!(
                            routine_id = %routine.id,
                            routine_name = %routine.name,
                            "Executing scheduled routine"
                        );
                        
                        // Execute routine (in production, this would trigger the bot)
                        if let Err(e) = self.execute_routine(&routine).await {
                            tracing::error!(
                                routine_id = %routine.id,
                                error = %e,
                                "Failed to execute routine"
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to check schedules");
                }
            }
        }

        tracing::info!("Scheduler stopped");
    }

    /// Stop the scheduler
    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
    }

    /// Execute a routine
    async fn execute_routine(&self, routine: &Routine) -> Result<(), String> {
        // Mark as executed
        self.routine_manager.mark_executed(routine.id).await?;

        let executor = self.executor.read().await.clone();
        match executor {
            Some(executor) => executor(routine.clone()).await,
            None => {
                tracing::warn!(
                    routine_id = %routine.id,
                    "No routine executor installed; routine skipped"
                );
                Ok(())
            }
        }
    }

    /// Add an event trigger
    pub async fn add_trigger(&self, trigger: EventTrigger) {
        let mut triggers = self.triggers.write().await;
        triggers.push(trigger);
    }

    /// Remove a trigger
    pub async fn remove_trigger(&self, trigger_id: Uuid) {
        let mut triggers = self.triggers.write().await;
        triggers.retain(|t| t.id != trigger_id);
    }

    /// Trigger all routines matching an event
    pub async fn trigger_event(&self, event: TriggerEvent) -> Result<Vec<Uuid>, String> {
        let triggers = self.triggers.read().await;
        let mut routine_ids = Vec::new();

        for trigger in triggers.iter() {
            if !trigger.enabled {
                continue;
            }

            let matches = match (&trigger.event, &event) {
                (TriggerEvent::CronSchedule { routine_id: r1 }, TriggerEvent::CronSchedule { routine_id: r2 }) => {
                    r1 == r2
                }
                (TriggerEvent::UserMessage { bot_id: b1, .. }, TriggerEvent::UserMessage { bot_id: b2, .. }) => {
                    b1 == b2
                }
                (TriggerEvent::TaskCompleted { bot_id: b1, .. }, TriggerEvent::TaskCompleted { bot_id: b2, .. }) => {
                    b1 == b2
                }
                _ => false,
            };

            if matches {
                routine_ids.push(trigger.routine_id);
            }
        }

        Ok(routine_ids)
    }

    /// Check if scheduler is running
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }
}
