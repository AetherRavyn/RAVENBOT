//! Durable agent-task and event-wait storage.
//!
//! The task loop is in the host, not here: this module owns the transitions and
//! the atomic claim/fire rules that make a second loop or a duplicate event
//! safe. A task is claimed exactly once, a wait fires exactly once, and a wait
//! payload is consumed exactly once.

use chrono::{DateTime, Utc};
use ravenbot_core::agent_task::{
    self, AgentTask, AgentTaskState, AgentWait, AgentWaitState, MAX_DEPENDENCIES,
};
use serde_json::Value;
use sqlx::SqlitePool;
use uuid::Uuid;

/// Fields for a new task. The caller has already resolved the target bot and
/// permission; the database still validates the text and resource shape.
pub struct NewAgentTask<'a> {
    pub id: Uuid,
    pub owner_bot_id: Uuid,
    pub target_bot_id: Uuid,
    pub owner_thread_id: Option<Uuid>,
    pub owner_run_id: Option<Uuid>,
    pub actor_id: Option<&'a str>,
    pub actor_name: Option<&'a str>,
    pub actor_role: Option<&'a str>,
    pub actor_workspace_id: Option<&'a str>,
    pub thread_id: Option<Uuid>,
    pub office_id: Option<Uuid>,
    pub title: &'a str,
    pub instruction: &'a str,
    pub expected_result: &'a str,
    pub resources: &'a [String],
    pub depends_on: &'a [Uuid],
    pub workspace: Option<&'a str>,
    pub depth: u32,
    pub auto_allow: bool,
    pub approval_policy: Option<&'a str>,
}

/// Fields for a new wait.
pub struct NewAgentWait<'a> {
    pub bot_id: Uuid,
    pub run_id: Uuid,
    pub thread_id: Uuid,
    pub event_kind: &'a str,
    pub filter: Value,
    pub deadline: DateTime<Utc>,
}

/// One claim pass. Dependency failures are reported separately so the host can
/// fire their waits; a task that is merely waiting for a dependency is skipped.
#[derive(Debug, Default)]
pub struct ClaimBatch {
    pub claimed: Option<AgentTask>,
    pub failed: Vec<AgentTask>,
}

#[derive(Debug, sqlx::FromRow)]
struct TaskRow {
    id: String,
    owner_bot_id: String,
    target_bot_id: String,
    owner_thread_id: Option<String>,
    owner_run_id: Option<String>,
    actor_id: Option<String>,
    actor_name: Option<String>,
    actor_role: Option<String>,
    actor_workspace_id: Option<String>,
    thread_id: Option<String>,
    run_id: Option<String>,
    office_id: Option<String>,
    title: String,
    instruction: String,
    expected_result: String,
    state: String,
    resources: String,
    depends_on: String,
    workspace: Option<String>,
    depth: i64,
    auto_allow: i64,
    approval_policy: Option<String>,
    result: Option<String>,
    error: Option<String>,
    created_at: String,
    started_at: Option<String>,
    completed_at: Option<String>,
    updated_at: String,
}

fn parse_dt(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

fn parse_optional_dt(value: Option<&str>) -> Option<DateTime<Utc>> {
    value.and_then(|value| {
        DateTime::parse_from_rfc3339(value)
            .map(|dt| dt.with_timezone(&Utc))
            .ok()
    })
}

impl TaskRow {
    fn into_domain(self) -> AgentTask {
        AgentTask {
            id: Uuid::parse_str(&self.id).unwrap_or_default(),
            owner_bot_id: Uuid::parse_str(&self.owner_bot_id).unwrap_or_default(),
            target_bot_id: Uuid::parse_str(&self.target_bot_id).unwrap_or_default(),
            owner_thread_id: self
                .owner_thread_id
                .as_deref()
                .and_then(|value| Uuid::parse_str(value).ok()),
            owner_run_id: self
                .owner_run_id
                .as_deref()
                .and_then(|value| Uuid::parse_str(value).ok()),
            actor_id: self.actor_id,
            actor_name: self.actor_name,
            actor_role: self.actor_role,
            actor_workspace_id: self.actor_workspace_id,
            thread_id: self
                .thread_id
                .as_deref()
                .and_then(|value| Uuid::parse_str(value).ok()),
            run_id: self
                .run_id
                .as_deref()
                .and_then(|value| Uuid::parse_str(value).ok()),
            office_id: self
                .office_id
                .as_deref()
                .and_then(|value| Uuid::parse_str(value).ok()),
            title: self.title,
            instruction: self.instruction,
            expected_result: self.expected_result,
            state: AgentTaskState::from_db(&self.state),
            resources: serde_json::from_str(&self.resources).unwrap_or_default(),
            depends_on: serde_json::from_str(&self.depends_on).unwrap_or_default(),
            workspace: self.workspace,
            depth: self.depth.max(0) as u32,
            auto_allow: self.auto_allow != 0,
            approval_policy: self.approval_policy,
            result: self.result,
            error: self.error,
            created_at: parse_dt(&self.created_at),
            started_at: parse_optional_dt(self.started_at.as_deref()),
            completed_at: parse_optional_dt(self.completed_at.as_deref()),
            updated_at: parse_dt(&self.updated_at),
        }
    }
}

const TASK_COLUMNS: &str = "id, owner_bot_id, target_bot_id, owner_thread_id, owner_run_id, \
     actor_id, actor_name, actor_role, actor_workspace_id, thread_id, run_id, office_id, title, \
     instruction, expected_result, state, resources, depends_on, workspace, depth, auto_allow, \
     approval_policy, result, error, created_at, started_at, completed_at, updated_at";

pub struct AgentTaskQueries;

impl AgentTaskQueries {
    /// Insert a queued task. The caller creates the task thread and the first
    /// message before this row, so a queued task already has a readable record.
    pub async fn create(pool: &SqlitePool, new: NewAgentTask<'_>) -> Result<AgentTask, String> {
        let title = agent_task::validate_task_title(new.title)?;
        let instruction = agent_task::validate_task_instruction(new.instruction)?;
        let expected_result = agent_task::validate_expected_result(new.expected_result)?;
        let resources = agent_task::normalize_resources(new.resources)?;
        if new.depends_on.len() > MAX_DEPENDENCIES {
            return Err(format!(
                "A task may depend on at most {MAX_DEPENDENCIES} tasks."
            ));
        }
        if let Some(policy) = new.approval_policy {
            if !matches!(policy, "allow_all" | "review") {
                return Err("Unknown task approval policy.".to_string());
            }
        }
        let id = new.id;
        let now = Utc::now().to_rfc3339();
        let resources = serde_json::to_string(&resources).map_err(|error| error.to_string())?;
        let depends_on =
            serde_json::to_string(new.depends_on).map_err(|error| error.to_string())?;
        sqlx::query(
            "INSERT INTO agent_tasks
             (id, owner_bot_id, target_bot_id, owner_thread_id, owner_run_id, actor_id,
              actor_name, actor_role, actor_workspace_id, thread_id, run_id, office_id, title,
              instruction, expected_result, state, resources, depends_on, workspace, depth,
              auto_allow, approval_policy, result, error, created_at, started_at, completed_at,
              updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?, ?, ?, 'queued', ?, ?, ?, ?,
                     ?, ?, NULL, NULL, ?, NULL, NULL, ?)",
        )
        .bind(id.to_string())
        .bind(new.owner_bot_id.to_string())
        .bind(new.target_bot_id.to_string())
        .bind(new.owner_thread_id.map(|id| id.to_string()))
        .bind(new.owner_run_id.map(|id| id.to_string()))
        .bind(new.actor_id)
        .bind(new.actor_name)
        .bind(new.actor_role)
        .bind(new.actor_workspace_id)
        .bind(new.thread_id.map(|id| id.to_string()))
        .bind(new.office_id.map(|id| id.to_string()))
        .bind(&title)
        .bind(&instruction)
        .bind(&expected_result)
        .bind(resources)
        .bind(depends_on)
        .bind(new.workspace)
        .bind(i64::from(new.depth))
        .bind(i64::from(new.auto_allow))
        .bind(new.approval_policy)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|error| format!("Could not create the task: {error}"))?;
        Self::get(pool, id)
            .await?
            .ok_or_else(|| "The created task could not be read back.".to_string())
    }

    pub async fn get(pool: &SqlitePool, id: Uuid) -> Result<Option<AgentTask>, String> {
        let sql = format!("SELECT {TASK_COLUMNS} FROM agent_tasks WHERE id = ?");
        let row: Option<TaskRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(id.to_string())
            .fetch_optional(pool)
            .await
            .map_err(|error| format!("Could not read the task: {error}"))?;
        Ok(row.map(TaskRow::into_domain))
    }

    pub async fn for_thread(
        pool: &SqlitePool,
        thread_id: Uuid,
    ) -> Result<Option<AgentTask>, String> {
        let sql =
            format!("SELECT {TASK_COLUMNS} FROM agent_tasks WHERE thread_id = ? ORDER BY created_at DESC LIMIT 1");
        let row: Option<TaskRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(thread_id.to_string())
            .fetch_optional(pool)
            .await
            .map_err(|error| format!("Could not read the task for this thread: {error}"))?;
        Ok(row.map(TaskRow::into_domain))
    }

    pub async fn list_owned(
        pool: &SqlitePool,
        owner_bot_id: Uuid,
        limit: i64,
    ) -> Result<Vec<AgentTask>, String> {
        let sql = format!(
            "SELECT {TASK_COLUMNS} FROM agent_tasks WHERE owner_bot_id = ? \
             ORDER BY created_at DESC LIMIT ?"
        );
        let rows: Vec<TaskRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(owner_bot_id.to_string())
            .bind(limit.clamp(1, 200))
            .fetch_all(pool)
            .await
            .map_err(|error| format!("Could not list tasks: {error}"))?;
        Ok(rows.into_iter().map(TaskRow::into_domain).collect())
    }

    /// Every task across the fleet, newest first. Bounded like `list_owned`.
    pub async fn list_all(pool: &SqlitePool, limit: i64) -> Result<Vec<AgentTask>, String> {
        let sql =
            format!("SELECT {TASK_COLUMNS} FROM agent_tasks ORDER BY created_at DESC LIMIT ?");
        let rows: Vec<TaskRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(limit.clamp(1, 200))
            .fetch_all(pool)
            .await
            .map_err(|error| format!("Could not list tasks: {error}"))?;
        Ok(rows.into_iter().map(TaskRow::into_domain).collect())
    }

    /// Claim one ready task, or report dependency failures.
    ///
    /// A task is ready when every dependency is completed, no running task
    /// holds one of its resources, and the update from `queued` to `starting`
    /// wins. The update is conditional, so two loops cannot claim the same row.
    pub async fn claim_next(pool: &SqlitePool, now: DateTime<Utc>) -> Result<ClaimBatch, String> {
        let sql = format!(
            "SELECT {TASK_COLUMNS} FROM agent_tasks WHERE state = 'queued' \
             ORDER BY created_at ASC LIMIT 50"
        );
        let queued: Vec<TaskRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .fetch_all(pool)
            .await
            .map_err(|error| format!("Could not read queued tasks: {error}"))?;
        if queued.is_empty() {
            return Ok(ClaimBatch::default());
        }
        let active = active_resources(pool).await?;
        let mut batch = ClaimBatch::default();
        for row in queued {
            let task = row.into_domain();
            match dependency_status(pool, &task).await? {
                DependencyStatus::Ready => {}
                DependencyStatus::Waiting => continue,
                DependencyStatus::Failed(reason) => {
                    if let Some(failed) = fail_queued(pool, task.id, &reason, now).await? {
                        batch.failed.push(failed);
                    }
                    continue;
                }
            }
            if task
                .resources
                .iter()
                .any(|resource| active.contains(resource))
            {
                continue;
            }
            let result = sqlx::query(
                "UPDATE agent_tasks
                 SET state = 'starting', started_at = ?, updated_at = ?
                 WHERE id = ? AND state = 'queued'",
            )
            .bind(now.to_rfc3339())
            .bind(now.to_rfc3339())
            .bind(task.id.to_string())
            .execute(pool)
            .await
            .map_err(|error| format!("Could not claim the task: {error}"))?;
            if result.rows_affected() == 1 {
                if let Some(mut claimed) = Self::get(pool, task.id).await? {
                    claimed.state = AgentTaskState::Starting;
                    batch.claimed = Some(claimed);
                }
                break;
            }
        }
        Ok(batch)
    }

    /// Move a claimed task to running once its run row exists.
    pub async fn mark_running(
        pool: &SqlitePool,
        id: Uuid,
        thread_id: Uuid,
        run_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Option<AgentTask>, String> {
        let result = sqlx::query(
            "UPDATE agent_tasks
             SET state = 'running', thread_id = ?, run_id = ?, updated_at = ?
             WHERE id = ? AND state = 'starting'",
        )
        .bind(thread_id.to_string())
        .bind(run_id.to_string())
        .bind(now.to_rfc3339())
        .bind(id.to_string())
        .execute(pool)
        .await
        .map_err(|error| format!("Could not mark the task running: {error}"))?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }
        Self::get(pool, id).await
    }

    /// Finish a running or starting task. Returns `None` when the task was
    /// already terminal (for example, closed by its owner while the run was
    /// unwinding).
    pub async fn finish(
        pool: &SqlitePool,
        id: Uuid,
        state: AgentTaskState,
        result: Option<&str>,
        error: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Option<AgentTask>, String> {
        if !state.is_terminal() {
            return Err("A finished task must be completed, failed or cancelled.".to_string());
        }
        let updated = sqlx::query(
            "UPDATE agent_tasks
             SET state = ?, result = ?, error = ?, completed_at = ?, updated_at = ?
             WHERE id = ? AND state IN ('starting','running')",
        )
        .bind(state.as_str())
        .bind(result)
        .bind(error)
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .bind(id.to_string())
        .execute(pool)
        .await
        .map_err(|error| format!("Could not finish the task: {error}"))?;
        if updated.rows_affected() == 0 {
            return Ok(None);
        }
        Self::get(pool, id).await
    }

    /// Close a task owned by `owner_bot_id`. Queued, starting and running tasks
    /// become cancelled; a terminal task is returned as `None`.
    pub async fn cancel(
        pool: &SqlitePool,
        id: Uuid,
        owner_bot_id: Uuid,
        reason: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<AgentTask>, String> {
        let reason = if reason.trim().is_empty() {
            "closed by its owner"
        } else {
            reason.trim()
        };
        let updated = sqlx::query(
            "UPDATE agent_tasks
             SET state = 'cancelled', error = ?, completed_at = ?, updated_at = ?
             WHERE id = ? AND owner_bot_id = ?
               AND state IN ('queued','starting','running')",
        )
        .bind(reason)
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .bind(id.to_string())
        .bind(owner_bot_id.to_string())
        .execute(pool)
        .await
        .map_err(|error| format!("Could not close the task: {error}"))?;
        if updated.rows_affected() == 0 {
            return Ok(None);
        }
        Self::get(pool, id).await
    }

    /// Fail every task a dead process left in flight.
    pub async fn recover_interrupted(
        pool: &SqlitePool,
        reason: &str,
        now: DateTime<Utc>,
    ) -> Result<Vec<AgentTask>, String> {
        let sql =
            format!("SELECT {TASK_COLUMNS} FROM agent_tasks WHERE state IN ('starting','running')");
        let rows: Vec<TaskRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .fetch_all(pool)
            .await
            .map_err(|error| format!("Could not read interrupted tasks: {error}"))?;
        let mut failed = Vec::new();
        for row in rows {
            let task = row.into_domain();
            let updated = sqlx::query(
                "UPDATE agent_tasks
                 SET state = 'failed', error = ?, completed_at = ?, updated_at = ?
                 WHERE id = ? AND state IN ('starting','running')",
            )
            .bind(reason)
            .bind(now.to_rfc3339())
            .bind(now.to_rfc3339())
            .bind(task.id.to_string())
            .execute(pool)
            .await
            .map_err(|error| format!("Could not recover the task: {error}"))?;
            if updated.rows_affected() == 1 {
                if let Some(mut task) = Self::get(pool, task.id).await? {
                    task.state = AgentTaskState::Failed;
                    task.error = Some(reason.to_string());
                    failed.push(task);
                }
            }
        }
        Ok(failed)
    }

    /// Number of tasks the host has claimed or is running.
    pub async fn active_count(pool: &SqlitePool) -> Result<u64, String> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM agent_tasks WHERE state IN ('starting','running')",
        )
        .fetch_one(pool)
        .await
        .map_err(|error| format!("Could not count active tasks: {error}"))?;
        Ok(count.max(0) as u64)
    }
}

enum DependencyStatus {
    Ready,
    Waiting,
    Failed(String),
}

async fn dependency_status(
    pool: &SqlitePool,
    task: &AgentTask,
) -> Result<DependencyStatus, String> {
    for dependency in &task.depends_on {
        let Some(dep) = AgentTaskQueries::get(pool, *dependency).await? else {
            return Ok(DependencyStatus::Failed(format!(
                "Dependency {dependency} no longer exists."
            )));
        };
        if dep.owner_bot_id != task.owner_bot_id {
            return Ok(DependencyStatus::Failed(format!(
                "Dependency {dependency} belongs to another agent."
            )));
        }
        match dep.state {
            AgentTaskState::Completed => {}
            AgentTaskState::Failed => {
                return Ok(DependencyStatus::Failed(format!(
                    "Dependency {dependency} failed."
                )))
            }
            AgentTaskState::Cancelled => {
                return Ok(DependencyStatus::Failed(format!(
                    "Dependency {dependency} was cancelled."
                )))
            }
            AgentTaskState::Queued | AgentTaskState::Starting | AgentTaskState::Running => {
                return Ok(DependencyStatus::Waiting)
            }
        }
    }
    Ok(DependencyStatus::Ready)
}

async fn fail_queued(
    pool: &SqlitePool,
    id: Uuid,
    reason: &str,
    now: DateTime<Utc>,
) -> Result<Option<AgentTask>, String> {
    let updated = sqlx::query(
        "UPDATE agent_tasks
         SET state = 'failed', error = ?, completed_at = ?, updated_at = ?
         WHERE id = ? AND state = 'queued'",
    )
    .bind(reason)
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .bind(id.to_string())
    .execute(pool)
    .await
    .map_err(|error| format!("Could not fail the dependent task: {error}"))?;
    if updated.rows_affected() == 0 {
        return Ok(None);
    }
    AgentTaskQueries::get(pool, id).await
}

async fn active_resources(pool: &SqlitePool) -> Result<Vec<String>, String> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT resources FROM agent_tasks WHERE state IN ('starting','running')")
            .fetch_all(pool)
            .await
            .map_err(|error| format!("Could not read active task resources: {error}"))?;
    let mut out = Vec::new();
    for (raw,) in rows {
        let resources: Vec<String> = serde_json::from_str(&raw).unwrap_or_default();
        for resource in resources {
            if !out.contains(&resource) {
                out.push(resource);
            }
        }
    }
    Ok(out)
}

#[derive(Debug, sqlx::FromRow)]
struct WaitRow {
    id: String,
    bot_id: String,
    run_id: String,
    thread_id: String,
    event_kind: String,
    filter: String,
    deadline: String,
    state: String,
    payload: Option<String>,
    created_at: String,
    fired_at: Option<String>,
    consumed_at: Option<String>,
    closed_at: Option<String>,
}

impl WaitRow {
    fn into_domain(self) -> AgentWait {
        AgentWait {
            id: Uuid::parse_str(&self.id).unwrap_or_default(),
            bot_id: Uuid::parse_str(&self.bot_id).unwrap_or_default(),
            run_id: Uuid::parse_str(&self.run_id).unwrap_or_default(),
            thread_id: Uuid::parse_str(&self.thread_id).unwrap_or_default(),
            event_kind: self.event_kind,
            filter: serde_json::from_str(&self.filter).unwrap_or_else(|_| serde_json::json!({})),
            deadline: parse_dt(&self.deadline),
            state: AgentWaitState::from_db(&self.state),
            payload: self
                .payload
                .as_deref()
                .and_then(|value| serde_json::from_str(value).ok()),
            created_at: parse_dt(&self.created_at),
            fired_at: parse_optional_dt(self.fired_at.as_deref()),
            consumed_at: parse_optional_dt(self.consumed_at.as_deref()),
            closed_at: parse_optional_dt(self.closed_at.as_deref()),
        }
    }
}

const WAIT_COLUMNS: &str = "id, bot_id, run_id, thread_id, event_kind, filter, deadline, \
     state, payload, created_at, fired_at, consumed_at, closed_at";

pub struct AgentWaitQueries;

impl AgentWaitQueries {
    pub async fn create(pool: &SqlitePool, new: NewAgentWait<'_>) -> Result<AgentWait, String> {
        let event_kind = agent_task::validate_event_kind(new.event_kind)?;
        let filter = agent_task::validate_event_filter(&new.filter)?;
        if event_kind == "timer" && filter.as_object().is_some_and(|map| !map.is_empty()) {
            return Err("A timer wait cannot carry a task filter.".to_string());
        }
        let id = Uuid::new_v4();
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO agent_waits
             (id, bot_id, run_id, thread_id, event_kind, filter, deadline, state, payload,
              created_at, fired_at, consumed_at, closed_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, 'waiting', NULL, ?, NULL, NULL, NULL)",
        )
        .bind(id.to_string())
        .bind(new.bot_id.to_string())
        .bind(new.run_id.to_string())
        .bind(new.thread_id.to_string())
        .bind(&event_kind)
        .bind(serde_json::to_string(&filter).map_err(|error| error.to_string())?)
        .bind(new.deadline.to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(pool)
        .await
        .map_err(|error| format!("Could not create the wait: {error}"))?;
        Self::get(pool, id)
            .await?
            .ok_or_else(|| "The created wait could not be read back.".to_string())
    }

    pub async fn get(pool: &SqlitePool, id: Uuid) -> Result<Option<AgentWait>, String> {
        let sql = format!("SELECT {WAIT_COLUMNS} FROM agent_waits WHERE id = ?");
        let row: Option<WaitRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(id.to_string())
            .fetch_optional(pool)
            .await
            .map_err(|error| format!("Could not read the wait: {error}"))?;
        Ok(row.map(WaitRow::into_domain))
    }

    pub async fn list_for_run(
        pool: &SqlitePool,
        run_id: Uuid,
        include_finished: bool,
    ) -> Result<Vec<AgentWait>, String> {
        let sql = if include_finished {
            format!(
                "SELECT {WAIT_COLUMNS} FROM agent_waits WHERE run_id = ? ORDER BY created_at DESC LIMIT 100"
            )
        } else {
            format!(
                "SELECT {WAIT_COLUMNS} FROM agent_waits WHERE run_id = ? \
                 AND state IN ('waiting','fired') ORDER BY created_at DESC LIMIT 100"
            )
        };
        let rows: Vec<WaitRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(run_id.to_string())
            .fetch_all(pool)
            .await
            .map_err(|error| format!("Could not list waits: {error}"))?;
        Ok(rows.into_iter().map(WaitRow::into_domain).collect())
    }

    /// Fire every waiting row for `owner_bot_id` whose exact filter matches.
    ///
    /// The `state = 'waiting'` predicate makes a duplicate event harmless: the
    /// second update affects zero rows, so a run is never woken twice by the
    /// same event or by two events racing.
    pub async fn fire_matching(
        pool: &SqlitePool,
        owner_bot_id: Uuid,
        event_kind: &str,
        event: &Value,
        now: DateTime<Utc>,
    ) -> Result<Vec<Uuid>, String> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT id, filter FROM agent_waits
             WHERE bot_id = ? AND event_kind = ? AND state = 'waiting'
             ORDER BY created_at ASC LIMIT 200",
        )
        .bind(owner_bot_id.to_string())
        .bind(event_kind)
        .fetch_all(pool)
        .await
        .map_err(|error| format!("Could not read waiting runs: {error}"))?;
        let payload = serde_json::to_string(event).map_err(|error| error.to_string())?;
        let mut fired = Vec::new();
        for (id, filter) in rows {
            let filter: Value =
                serde_json::from_str(&filter).unwrap_or_else(|_| serde_json::json!({}));
            if !agent_task::task_event_matches(&filter, event) {
                continue;
            }
            // A filter that names only a task is asking for its outcome or a
            // message, not the bookkeeping transition into `running`. Callers
            // that really want the start can ask for `state: "running"`
            // explicitly; the implicit filter must not consume the wait before
            // the result exists.
            let explicit_state = filter.get("state").and_then(Value::as_str);
            let transient_state = event.get("event").and_then(Value::as_str) == Some("state")
                && matches!(
                    event.get("state").and_then(Value::as_str),
                    Some("queued" | "starting" | "running")
                );
            if transient_state && explicit_state.is_none() {
                continue;
            }
            let updated = sqlx::query(
                "UPDATE agent_waits
                 SET state = 'fired', payload = ?, fired_at = ?
                 WHERE id = ? AND state = 'waiting'",
            )
            .bind(&payload)
            .bind(now.to_rfc3339())
            .bind(&id)
            .execute(pool)
            .await
            .map_err(|error| format!("Could not fire the wait: {error}"))?;
            if updated.rows_affected() == 1 {
                if let Ok(id) = Uuid::parse_str(&id) {
                    fired.push(id);
                }
            }
        }
        Ok(fired)
    }

    /// Consume a fired wait's payload exactly once.
    pub async fn claim_fired(
        pool: &SqlitePool,
        id: Uuid,
        run_id: Uuid,
    ) -> Result<Option<Value>, String> {
        let mut tx = pool
            .begin()
            .await
            .map_err(|error| format!("Could not open the wait transaction: {error}"))?;
        let row: Option<(Option<String>,)> = sqlx::query_as(
            "SELECT payload FROM agent_waits
             WHERE id = ? AND run_id = ? AND state = 'fired'",
        )
        .bind(id.to_string())
        .bind(run_id.to_string())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| format!("Could not read the fired wait: {error}"))?;
        let Some((payload,)) = row else {
            let _ = tx.rollback().await;
            return Ok(None);
        };
        let updated = sqlx::query(
            "UPDATE agent_waits
             SET state = 'consumed', consumed_at = ?
             WHERE id = ? AND run_id = ? AND state = 'fired'",
        )
        .bind(Utc::now().to_rfc3339())
        .bind(id.to_string())
        .bind(run_id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("Could not consume the wait: {error}"))?;
        if updated.rows_affected() != 1 {
            let _ = tx.rollback().await;
            return Ok(None);
        }
        tx.commit()
            .await
            .map_err(|error| format!("Could not commit the wait: {error}"))?;
        Ok(payload.and_then(|value| serde_json::from_str(&value).ok()))
    }

    /// Close a waiting or fired wait. Only the run that owns it may do this.
    pub async fn cancel(
        pool: &SqlitePool,
        id: Uuid,
        run_id: Uuid,
        reason: &str,
    ) -> Result<bool, String> {
        let payload = serde_json::json!({
            "event": "cancelled",
            "reason": reason,
        });
        let updated = sqlx::query(
            "UPDATE agent_waits
             SET state = 'cancelled', payload = ?, closed_at = ?
             WHERE id = ? AND run_id = ? AND state IN ('waiting','fired')",
        )
        .bind(serde_json::to_string(&payload).map_err(|error| error.to_string())?)
        .bind(Utc::now().to_rfc3339())
        .bind(id.to_string())
        .bind(run_id.to_string())
        .execute(pool)
        .await
        .map_err(|error| format!("Could not close the wait: {error}"))?;
        Ok(updated.rows_affected() == 1)
    }

    /// Expire one waiting task wait. Returns true when this call won.
    pub async fn expire(
        pool: &SqlitePool,
        id: Uuid,
        run_id: Uuid,
        reason: &str,
    ) -> Result<bool, String> {
        let payload = serde_json::json!({"event": "timeout", "reason": reason});
        let updated = sqlx::query(
            "UPDATE agent_waits
             SET state = 'expired', payload = ?, closed_at = ?
             WHERE id = ? AND run_id = ? AND state = 'waiting'",
        )
        .bind(serde_json::to_string(&payload).map_err(|error| error.to_string())?)
        .bind(Utc::now().to_rfc3339())
        .bind(id.to_string())
        .bind(run_id.to_string())
        .execute(pool)
        .await
        .map_err(|error| format!("Could not expire the wait: {error}"))?;
        Ok(updated.rows_affected() == 1)
    }

    /// Fire one due timer. Returns true when this call won.
    pub async fn fire_timer(pool: &SqlitePool, id: Uuid, run_id: Uuid) -> Result<bool, String> {
        let wait = Self::get(pool, id)
            .await?
            .ok_or_else(|| "That wait no longer exists.".to_string())?;
        if wait.run_id != run_id || wait.event_kind != "timer" {
            return Ok(false);
        }
        let payload = agent_task::timer_event(wait.deadline);
        let updated = sqlx::query(
            "UPDATE agent_waits
             SET state = 'fired', payload = ?, fired_at = ?
             WHERE id = ? AND run_id = ? AND state = 'waiting' AND event_kind = 'timer'",
        )
        .bind(serde_json::to_string(&payload).map_err(|error| error.to_string())?)
        .bind(Utc::now().to_rfc3339())
        .bind(id.to_string())
        .bind(run_id.to_string())
        .execute(pool)
        .await
        .map_err(|error| format!("Could not fire the timer: {error}"))?;
        Ok(updated.rows_affected() == 1)
    }

    /// Fire due timer waits. Returns the number fired.
    pub async fn fire_timers(pool: &SqlitePool, now: DateTime<Utc>) -> Result<u64, String> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT id, deadline FROM agent_waits
             WHERE state = 'waiting' AND event_kind = 'timer' AND deadline <= ?",
        )
        .bind(now.to_rfc3339())
        .fetch_all(pool)
        .await
        .map_err(|error| format!("Could not read due timers: {error}"))?;
        let mut fired = 0u64;
        for (id, deadline) in rows {
            let deadline = parse_dt(&deadline);
            let payload = agent_task::timer_event(deadline);
            let updated = sqlx::query(
                "UPDATE agent_waits
                 SET state = 'fired', payload = ?, fired_at = ?
                 WHERE id = ? AND state = 'waiting'",
            )
            .bind(serde_json::to_string(&payload).map_err(|error| error.to_string())?)
            .bind(now.to_rfc3339())
            .bind(&id)
            .execute(pool)
            .await
            .map_err(|error| format!("Could not fire the timer: {error}"))?;
            fired += updated.rows_affected();
        }
        Ok(fired)
    }

    /// Expire task waits whose deadline passed. Timers are handled separately.
    pub async fn expire_overdue(pool: &SqlitePool, now: DateTime<Utc>) -> Result<u64, String> {
        let payload = serde_json::json!({"event": "timeout"});
        let updated = sqlx::query(
            "UPDATE agent_waits
             SET state = 'expired', payload = ?, closed_at = ?
             WHERE state = 'waiting' AND event_kind != 'timer' AND deadline <= ?",
        )
        .bind(serde_json::to_string(&payload).map_err(|error| error.to_string())?)
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(pool)
        .await
        .map_err(|error| format!("Could not expire waits: {error}"))?;
        Ok(updated.rows_affected())
    }

    /// Expire everything left in flight by a dead process.
    pub async fn recover_interrupted(pool: &SqlitePool, reason: &str) -> Result<u64, String> {
        let payload = serde_json::json!({"event": "interrupted", "reason": reason});
        let updated = sqlx::query(
            "UPDATE agent_waits
             SET state = 'expired', payload = ?, closed_at = ?
             WHERE state IN ('waiting','fired')",
        )
        .bind(serde_json::to_string(&payload).map_err(|error| error.to_string())?)
        .bind(Utc::now().to_rfc3339())
        .execute(pool)
        .await
        .map_err(|error| format!("Could not recover waits: {error}"))?;
        Ok(updated.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ravenbot_core::agent_task::{normalize_resources, AgentTaskState};
    use sqlx::sqlite::SqlitePoolOptions;

    async fn db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        crate::migrations::run(&pool).await.expect("migrations");
        pool
    }

    async fn bots(pool: &SqlitePool) -> (Uuid, Uuid) {
        let owner = ravenbot_core::Bot::new("Owner", "starts tasks");
        let target = ravenbot_core::Bot::new("Target", "runs tasks");
        crate::queries::BotQueries::insert(pool, &owner)
            .await
            .expect("owner");
        crate::queries::BotQueries::insert(pool, &target)
            .await
            .expect("target");
        (owner.id, target.id)
    }

    fn new_task<'a>(
        owner: Uuid,
        target: Uuid,
        thread_id: Uuid,
        resources: &'a [String],
        depends_on: &'a [Uuid],
        title: &'a str,
    ) -> NewAgentTask<'a> {
        NewAgentTask {
            id: Uuid::new_v4(),
            owner_bot_id: owner,
            target_bot_id: target,
            owner_thread_id: Some(Uuid::new_v4()),
            owner_run_id: Some(Uuid::new_v4()),
            actor_id: None,
            actor_name: None,
            actor_role: None,
            actor_workspace_id: None,
            thread_id: Some(thread_id),
            office_id: None,
            title,
            instruction: "Do the work.",
            expected_result: "A result.",
            resources,
            depends_on,
            workspace: None,
            depth: 1,
            auto_allow: false,
            approval_policy: None,
        }
    }

    async fn queue(
        pool: &SqlitePool,
        owner: Uuid,
        target: Uuid,
        title: &str,
        resources: &[String],
        depends_on: &[Uuid],
    ) -> AgentTask {
        let thread = ravenbot_core::Thread::new_task(target, title);
        crate::queries::ThreadQueries::create(pool, &thread)
            .await
            .expect("thread");
        AgentTaskQueries::create(
            pool,
            new_task(owner, target, thread.id, resources, depends_on, title),
        )
        .await
        .expect("task")
    }

    async fn claim(pool: &SqlitePool) -> AgentTask {
        AgentTaskQueries::claim_next(pool, Utc::now())
            .await
            .expect("claim")
            .claimed
            .expect("a task")
    }

    #[tokio::test]
    async fn list_all_returns_tasks_across_owners_and_respects_the_cap() {
        let pool = db().await;
        let (owner, target) = bots(&pool).await;
        let first = queue(&pool, owner, target, "First", &[], &[]).await;
        let second = queue(&pool, target, target, "Second", &[], &[]).await;

        let all = AgentTaskQueries::list_all(&pool, 200)
            .await
            .expect("all tasks");
        assert_eq!(all.len(), 2);
        let ids: Vec<Uuid> = all.iter().map(|task| task.id).collect();
        assert!(ids.contains(&first.id));
        assert!(ids.contains(&second.id));

        let one = AgentTaskQueries::list_all(&pool, 1)
            .await
            .expect("one task");
        assert_eq!(one.len(), 1);
    }

    #[tokio::test]
    async fn actor_metadata_round_trips_with_a_task() {
        let pool = db().await;
        let (owner, target) = bots(&pool).await;
        let thread = ravenbot_core::Thread::new_task(target, "Actor task");
        crate::queries::ThreadQueries::create(&pool, &thread)
            .await
            .expect("thread");
        let task = AgentTaskQueries::create(
            &pool,
            NewAgentTask {
                id: Uuid::new_v4(),
                owner_bot_id: owner,
                target_bot_id: target,
                owner_thread_id: None,
                owner_run_id: None,
                actor_id: Some("chat-1"),
                actor_name: Some("Chat user"),
                actor_role: Some("chat"),
                actor_workspace_id: Some("workspace-1"),
                thread_id: Some(thread.id),
                office_id: None,
                title: "Actor task",
                instruction: "Do not become the owner.",
                expected_result: "",
                resources: &[],
                depends_on: &[],
                workspace: None,
                depth: 1,
                auto_allow: false,
                approval_policy: None,
            },
        )
        .await
        .expect("task");
        assert_eq!(task.actor_id.as_deref(), Some("chat-1"));
        assert_eq!(task.actor_role.as_deref(), Some("chat"));
        assert_eq!(task.actor_workspace_id.as_deref(), Some("workspace-1"));
        let loaded = AgentTaskQueries::get(&pool, task.id)
            .await
            .unwrap()
            .expect("loaded");
        assert_eq!(loaded.actor_name.as_deref(), Some("Chat user"));
    }

    #[tokio::test]
    async fn a_queued_task_is_claimed_once_and_moves_through_running_to_completed() {
        let pool = db().await;
        let (owner, target) = bots(&pool).await;
        let task = queue(&pool, owner, target, "Check the build", &[], &[]).await;
        assert_eq!(task.state, AgentTaskState::Queued);

        let claimed = claim(&pool).await;
        assert_eq!(claimed.id, task.id);
        assert_eq!(claimed.state, AgentTaskState::Starting);
        assert!(AgentTaskQueries::claim_next(&pool, Utc::now())
            .await
            .unwrap()
            .claimed
            .is_none());

        let run_id = Uuid::new_v4();
        let running = AgentTaskQueries::mark_running(
            &pool,
            task.id,
            task.thread_id.unwrap(),
            run_id,
            Utc::now(),
        )
        .await
        .unwrap()
        .expect("running");
        assert_eq!(running.state, AgentTaskState::Running);
        assert_eq!(running.run_id, Some(run_id));

        let done = AgentTaskQueries::finish(
            &pool,
            task.id,
            AgentTaskState::Completed,
            Some("build is green"),
            None,
            Utc::now(),
        )
        .await
        .unwrap()
        .expect("finished");
        assert_eq!(done.state, AgentTaskState::Completed);
        assert_eq!(done.result.as_deref(), Some("build is green"));
        assert!(AgentTaskQueries::finish(
            &pool,
            task.id,
            AgentTaskState::Failed,
            None,
            Some("late"),
            Utc::now(),
        )
        .await
        .unwrap()
        .is_none());
    }

    #[tokio::test]
    async fn dependencies_gate_a_task_and_a_failed_dependency_fails_it() {
        let pool = db().await;
        let (owner, target) = bots(&pool).await;
        let first = queue(&pool, owner, target, "First", &[], &[]).await;
        let second = queue(&pool, owner, target, "Second", &[], &[first.id]).await;
        let first_claim = claim(&pool).await;
        assert_eq!(first_claim.id, first.id);
        assert!(AgentTaskQueries::claim_next(&pool, Utc::now())
            .await
            .unwrap()
            .claimed
            .is_none());
        AgentTaskQueries::mark_running(
            &pool,
            first.id,
            first.thread_id.unwrap(),
            Uuid::new_v4(),
            Utc::now(),
        )
        .await
        .unwrap();
        AgentTaskQueries::finish(
            &pool,
            first.id,
            AgentTaskState::Completed,
            Some("ok"),
            None,
            Utc::now(),
        )
        .await
        .unwrap();
        let second_claim = claim(&pool).await;
        assert_eq!(second_claim.id, second.id);

        // A failed dependency is a terminal failure, not an eternal wait.
        let third = queue(&pool, owner, target, "Third", &[], &[second.id]).await;
        AgentTaskQueries::finish(
            &pool,
            second.id,
            AgentTaskState::Failed,
            None,
            Some("no"),
            Utc::now(),
        )
        .await
        .unwrap();
        let batch = AgentTaskQueries::claim_next(&pool, Utc::now())
            .await
            .unwrap();
        assert!(batch.claimed.is_none());
        assert_eq!(batch.failed.len(), 1);
        assert_eq!(batch.failed[0].id, third.id);
        assert_eq!(batch.failed[0].state, AgentTaskState::Failed);
        assert!(batch.failed[0].error.as_deref().unwrap().contains("failed"));
    }

    #[tokio::test]
    async fn a_resource_held_by_a_running_task_serializes_the_next_claim() {
        let pool = db().await;
        let (owner, target) = bots(&pool).await;
        let browser = vec!["browser".to_string()];
        let first = queue(&pool, owner, target, "Browser one", &browser, &[]).await;
        let second = queue(&pool, owner, target, "Browser two", &browser, &[]).await;
        let first_claim = claim(&pool).await;
        assert_eq!(first_claim.id, first.id);
        assert!(AgentTaskQueries::claim_next(&pool, Utc::now())
            .await
            .unwrap()
            .claimed
            .is_none());
        AgentTaskQueries::finish(
            &pool,
            first.id,
            AgentTaskState::Completed,
            None,
            None,
            Utc::now(),
        )
        .await
        .unwrap();
        let second_claim = claim(&pool).await;
        assert_eq!(second_claim.id, second.id);
    }

    #[tokio::test]
    async fn different_workspace_resources_do_not_share_a_lock() {
        let pool = db().await;
        let (owner, target) = bots(&pool).await;
        let one = normalize_resources(&["workspace:/tmp/one".to_string()]).unwrap();
        let two = normalize_resources(&["workspace:/tmp/two".to_string()]).unwrap();
        let first = queue(&pool, owner, target, "One", &one, &[]).await;
        let second = queue(&pool, owner, target, "Two", &two, &[]).await;
        assert_eq!(claim(&pool).await.id, first.id);
        AgentTaskQueries::finish(
            &pool,
            first.id,
            AgentTaskState::Completed,
            None,
            None,
            Utc::now(),
        )
        .await
        .unwrap();
        assert_eq!(claim(&pool).await.id, second.id);
    }

    #[tokio::test]
    async fn an_owner_can_close_a_task_but_another_agent_cannot() {
        let pool = db().await;
        let (owner, target) = bots(&pool).await;
        let task = queue(&pool, owner, target, "Close me", &[], &[]).await;
        assert!(
            AgentTaskQueries::cancel(&pool, task.id, Uuid::new_v4(), "bad actor", Utc::now())
                .await
                .unwrap()
                .is_none()
        );
        let closed = AgentTaskQueries::cancel(&pool, task.id, owner, "not needed", Utc::now())
            .await
            .unwrap()
            .expect("closed");
        assert_eq!(closed.state, AgentTaskState::Cancelled);
        assert!(closed.error.as_deref().unwrap().contains("not needed"));
    }

    #[tokio::test]
    async fn interrupted_tasks_become_failed_with_a_reason() {
        let pool = db().await;
        let (owner, target) = bots(&pool).await;
        let task = queue(&pool, owner, target, "Interrupt", &[], &[]).await;
        let _ = claim(&pool).await;
        let recovered = AgentTaskQueries::recover_interrupted(
            &pool,
            "app quit while the task was running",
            Utc::now(),
        )
        .await
        .expect("recover");
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].id, task.id);
        assert_eq!(recovered[0].state, AgentTaskState::Failed);
        assert!(recovered[0].error.as_deref().unwrap().contains("app quit"));
        assert!(AgentTaskQueries::claim_next(&pool, Utc::now())
            .await
            .unwrap()
            .claimed
            .is_none());
    }

    async fn make_wait(
        pool: &SqlitePool,
        bot_id: Uuid,
        event_kind: &str,
        filter: Value,
        timeout_ms: i64,
    ) -> AgentWait {
        AgentWaitQueries::create(
            pool,
            NewAgentWait {
                bot_id,
                run_id: Uuid::new_v4(),
                thread_id: Uuid::new_v4(),
                event_kind,
                filter,
                deadline: Utc::now() + chrono::Duration::milliseconds(timeout_ms),
            },
        )
        .await
        .expect("wait")
    }

    #[tokio::test]
    async fn a_matching_event_fires_a_wait_once_and_the_payload_is_consumed_once() {
        let pool = db().await;
        let (owner, _) = bots(&pool).await;
        let task_id = Uuid::new_v4();
        let wait = make_wait(
            &pool,
            owner,
            "task",
            serde_json::json!({"task_id": task_id.to_string(), "state": "completed"}),
            60_000,
        )
        .await;
        let mismatched = serde_json::json!({
            "event": "state", "task_id": task_id, "state": "running"
        });
        assert!(
            AgentWaitQueries::fire_matching(&pool, owner, "task", &mismatched, Utc::now())
                .await
                .unwrap()
                .is_empty()
        );
        let event = serde_json::json!({
            "event": "state", "task_id": task_id, "state": "completed", "result": "ok"
        });
        let fired = AgentWaitQueries::fire_matching(&pool, owner, "task", &event, Utc::now())
            .await
            .unwrap();
        assert_eq!(fired, vec![wait.id]);
        // A duplicate event must not fire it again.
        assert!(
            AgentWaitQueries::fire_matching(&pool, owner, "task", &event, Utc::now())
                .await
                .unwrap()
                .is_empty()
        );

        let payload = AgentWaitQueries::claim_fired(&pool, wait.id, wait.run_id)
            .await
            .unwrap()
            .expect("payload");
        assert_eq!(payload["result"], "ok");
        assert!(AgentWaitQueries::claim_fired(&pool, wait.id, wait.run_id)
            .await
            .unwrap()
            .is_none());
        let stored = AgentWaitQueries::get(&pool, wait.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.state, AgentWaitState::Consumed);
    }

    #[tokio::test]
    async fn a_task_only_filter_waits_for_the_outcome_not_the_running_transition() {
        let pool = db().await;
        let (owner, _) = bots(&pool).await;
        let task_id = Uuid::new_v4();
        let implicit = make_wait(
            &pool,
            owner,
            "task",
            serde_json::json!({"task_id": task_id.to_string()}),
            60_000,
        )
        .await;
        let explicit = make_wait(
            &pool,
            owner,
            "task",
            serde_json::json!({"task_id": task_id.to_string(), "state": "running"}),
            60_000,
        )
        .await;
        let running = serde_json::json!({
            "event": "state", "task_id": task_id, "state": "running"
        });
        let fired = AgentWaitQueries::fire_matching(&pool, owner, "task", &running, Utc::now())
            .await
            .unwrap();
        assert_eq!(fired, vec![explicit.id]);
        let completed = serde_json::json!({
            "event": "state", "task_id": task_id, "state": "completed", "result": "ok"
        });
        let fired = AgentWaitQueries::fire_matching(&pool, owner, "task", &completed, Utc::now())
            .await
            .unwrap();
        assert_eq!(fired, vec![implicit.id]);
    }

    #[tokio::test]
    async fn a_wait_only_fires_for_its_owner_and_filter() {
        let pool = db().await;
        let (owner, other) = bots(&pool).await;
        let task_id = Uuid::new_v4();
        let wait = make_wait(
            &pool,
            owner,
            "task",
            serde_json::json!({"task_id": task_id.to_string()}),
            60_000,
        )
        .await;
        let event = serde_json::json!({"event": "state", "task_id": task_id, "state": "completed"});
        assert!(
            AgentWaitQueries::fire_matching(&pool, other, "task", &event, Utc::now())
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            AgentWaitQueries::fire_matching(&pool, owner, "task", &event, Utc::now())
                .await
                .unwrap()
                .contains(&wait.id)
        );
    }

    #[tokio::test]
    async fn timers_fire_at_the_deadline_and_task_waits_expire() {
        let pool = db().await;
        let (owner, _) = bots(&pool).await;
        let timer = make_wait(&pool, owner, "timer", serde_json::json!({}), 10).await;
        let task_wait = make_wait(&pool, owner, "task", serde_json::json!({}), 10).await;
        let first_fired =
            AgentWaitQueries::fire_timers(&pool, Utc::now() + chrono::Duration::milliseconds(20))
                .await
                .unwrap();
        assert_eq!(first_fired, 1);
        let fired = AgentWaitQueries::get(&pool, timer.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fired.state, AgentWaitState::Fired);
        assert_eq!(fired.payload.as_ref().unwrap()["event"], "timer");
        let expired = AgentWaitQueries::expire_overdue(
            &pool,
            Utc::now() + chrono::Duration::milliseconds(20),
        )
        .await
        .unwrap();
        assert_eq!(expired, 1);
        let expired = AgentWaitQueries::get(&pool, task_wait.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(expired.state, AgentWaitState::Expired);
    }

    #[tokio::test]
    async fn a_wait_can_be_cancelled_only_by_its_run() {
        let pool = db().await;
        let (owner, _) = bots(&pool).await;
        let wait = make_wait(&pool, owner, "task", serde_json::json!({}), 60_000).await;
        assert!(
            !AgentWaitQueries::cancel(&pool, wait.id, Uuid::new_v4(), "no")
                .await
                .unwrap()
        );
        assert!(
            AgentWaitQueries::cancel(&pool, wait.id, wait.run_id, "not needed")
                .await
                .unwrap()
        );
        let stored = AgentWaitQueries::get(&pool, wait.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.state, AgentWaitState::Cancelled);
    }

    #[tokio::test]
    async fn interrupted_waits_are_expired_at_startup() {
        let pool = db().await;
        let (owner, _) = bots(&pool).await;
        let wait = make_wait(&pool, owner, "task", serde_json::json!({}), 60_000).await;
        let count = AgentWaitQueries::recover_interrupted(&pool, "app quit")
            .await
            .expect("recover");
        assert_eq!(count, 1);
        let stored = AgentWaitQueries::get(&pool, wait.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.state, AgentWaitState::Expired);
        assert_eq!(stored.payload.as_ref().unwrap()["reason"], "app quit");
    }
}
