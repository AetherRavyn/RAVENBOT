//! Durable work ledger for graph execution.
//!
//! The graph is the scheduler and stays in memory (or in `office_graphs` as a
//! snapshot). This module mirrors every scheduler transition into the
//! `work_items` ledger so a human, a reviewer, or another agent can read what
//! happened without loading a graph.
//!
//! Ledger writes are best-effort: a lost ledger row must not stop the work it
//! describes. The graph snapshot is still the resume authority.

use crate::graph::{reports_incomplete_work, NodeState, TaskGraph, TaskNode, WorkflowNodeType};
use ravenbot_db::queries::{FileChangeQueries, NewWorkItem, WorkItem, WorkItemQueries};
use sqlx::SqlitePool;
use uuid::Uuid;

/// Lease length for a graph node. Long enough for a large model round, short
/// enough that a crashed owner's item becomes reclaimable.
const LEASE_SECS: u64 = 900;

#[derive(Clone)]
pub struct WorkLedger {
    pool: SqlitePool,
    office_id: Uuid,
}

impl WorkLedger {
    pub fn new(pool: SqlitePool, office_id: Uuid) -> Self {
        Self { pool, office_id }
    }

    /// Mirror every node and edge of a plan into the ledger.
    ///
    /// Safe to call on a resumed graph: seeding refreshes planning fields but
    /// never moves a done or running item backwards.
    pub async fn seed(&self, graph: &TaskGraph, cycle: u32) -> Result<(), sqlx::Error> {
        for id in graph.ordered_node_ids() {
            let Some(node) = graph.nodes.get(&id) else {
                continue;
            };
            let item = NewWorkItem {
                id: node.id,
                kind: "office_task".to_string(),
                parent_id: None,
                office_id: Some(self.office_id),
                thread_id: node.thread_id,
                run_id: match node.state {
                    NodeState::Running(run_id) => Some(run_id),
                    _ => None,
                },
                title: title_for(node),
                instruction: node.instruction.clone(),
                contract: serde_json::json!({}),
                status: status_for(&node.state).to_string(),
                owner_bot_id: Some(node.bot_id),
                attempts: i64::from(node.attempts),
                max_attempts: i64::from(node.max_attempts.max(1)),
                priority: 0,
                metadata: serde_json::json!({
                    "graph_id": graph.id.to_string(),
                    "cycle": cycle,
                    "node_type": node.node_type.as_str(),
                }),
            };
            WorkItemQueries::seed(&self.pool, &item).await?;
        }
        for (parent, child) in &graph.edges {
            WorkItemQueries::link(&self.pool, *parent, *child).await?;
        }
        Ok(())
    }

    /// Record that a node has started.
    pub async fn started(&self, node: &TaskNode, run_id: Uuid) {
        let id = node.id.to_string();
        if node.iterations > 0 {
            // A loop iteration is a continuation, not a new attempt. Claiming
            // again would consume the retry budget the first iteration used.
            if let Err(error) =
                WorkItemQueries::mark_running_again(&self.pool, &id, run_id, LEASE_SECS).await
            {
                tracing::debug!(%id, %error, "Could not mark a work item as iterating");
            }
            return;
        }

        match WorkItemQueries::claim(&self.pool, &id, run_id, "graph-executor", LEASE_SECS).await {
            Ok(true) => {}
            Ok(false) => {
                // A graph that predates the ledger has no row. Seed one from
                // the node so the run is still visible, without claiming.
                let Some(current) = WorkItemQueries::get(&self.pool, &id).await.ok().flatten()
                else {
                    let item = NewWorkItem {
                        id: node.id,
                        kind: "office_task".to_string(),
                        parent_id: None,
                        office_id: Some(self.office_id),
                        thread_id: node.thread_id,
                        run_id: Some(run_id),
                        title: title_for(node),
                        instruction: node.instruction.clone(),
                        contract: serde_json::json!({}),
                        status: "running".to_string(),
                        owner_bot_id: Some(node.bot_id),
                        attempts: 1,
                        max_attempts: i64::from(node.max_attempts.max(1)),
                        priority: 0,
                        metadata: serde_json::json!({ "seeded_on_start": true }),
                    };
                    if let Err(error) = WorkItemQueries::seed(&self.pool, &item).await {
                        tracing::debug!(%id, %error, "Could not seed a started work item");
                    }
                    return;
                };
                // A loop group resets completed members to pending, and a
                // rescheduled branch can briefly still read done. The graph is
                // authoritative about what is running now: reopen the item
                // without spending another attempt. A failed item is left
                // alone, because its attempt cap is the thing being enforced.
                if matches!(
                    current.status.as_str(),
                    "done" | "pending" | "skipped" | "blocked" | "review"
                ) {
                    if let Err(error) =
                        WorkItemQueries::mark_running_again(&self.pool, &id, run_id, LEASE_SECS)
                            .await
                    {
                        tracing::debug!(%id, %error, "Could not reopen a work item");
                    }
                } else {
                    tracing::debug!(
                        %id,
                        status = %current.status,
                        "Work item was not claimable; the graph remains authoritative"
                    );
                }
            }
            Err(error) => tracing::debug!(%id, %error, "Could not claim a work item"),
        }
    }

    /// Mirror a node's current terminal or scheduled state.
    pub async fn sync(&self, node: &TaskNode) {
        let id = node.id.to_string();
        if let Some(thread_id) = node.thread_id {
            if let Err(error) = WorkItemQueries::attach_thread(&self.pool, &id, thread_id).await {
                tracing::debug!(%id, %error, "Could not attach a thread to a work item");
            }
        }
        let run_id = WorkItemQueries::get(&self.pool, &id)
            .await
            .ok()
            .flatten()
            .and_then(|item| item.run_id)
            .and_then(|value| Uuid::parse_str(&value).ok());

        let result = match &node.state {
            NodeState::Pending => {
                WorkItemQueries::mark_pending(&self.pool, &id, "graph rescheduled the task").await
            }
            NodeState::Running(_) => Ok(false),
            NodeState::Done(text)
                if matches!(
                    node.node_type,
                    WorkflowNodeType::Agent | WorkflowNodeType::Llm
                ) && reports_incomplete_work(text) =>
            {
                WorkItemQueries::mark_failed(
                    &self.pool,
                    &id,
                    run_id,
                    "The node reported its own task as incomplete; its report is a progress note.",
                )
                .await
            }
            NodeState::Done(text) => {
                let changed = WorkItemQueries::mark_done(&self.pool, &id, run_id, text).await;
                if matches!(changed, Ok(true)) {
                    if let Some(run_id) = run_id {
                        self.attach_artifacts(&id, run_id).await;
                    }
                }
                changed
            }
            NodeState::Failed(error) => {
                WorkItemQueries::mark_failed(&self.pool, &id, run_id, error).await
            }
            NodeState::Skipped => {
                WorkItemQueries::mark_skipped(
                    &self.pool,
                    &id,
                    "dependency failed or branch condition was false",
                )
                .await
            }
        };
        if let Err(error) = result {
            tracing::debug!(%id, %error, "Could not sync a work item");
        }
    }

    /// Turn the files a run actually changed into durable artifact rows.
    /// `file_changes` is already written by the runtime at tool-call time, so
    /// this needs no extra model output and cannot be inflated by prose.
    async fn attach_artifacts(&self, work_item_id: &str, run_id: Uuid) {
        match FileChangeQueries::paths_for_run(&self.pool, run_id).await {
            Ok(paths) => {
                for path in paths {
                    if let Err(error) =
                        WorkItemQueries::add_artifact_once(&self.pool, work_item_id, &path).await
                    {
                        tracing::debug!(%work_item_id, %path, %error, "Could not attach artifact");
                    }
                }
            }
            Err(error) => {
                tracing::debug!(%work_item_id, %run_id, %error, "Could not read changed files");
            }
        }
    }

    /// Reconcile every node after the graph stops, including skipped
    /// dependents that no individual node transition touched.
    pub async fn reconcile(&self, graph: &TaskGraph) {
        for id in graph.ordered_node_ids() {
            if let Some(node) = graph.nodes.get(&id) {
                self.sync(node).await;
            }
        }
    }
}

/// Markers around the generated block. Everything outside them belongs to the
/// agents and is never rewritten.
pub const STATUS_BEGIN: &str = "<!-- ravenbot:work-ledger:begin -->";
pub const STATUS_END: &str = "<!-- ravenbot:work-ledger:end -->";

/// Render the ledger as the table that belongs in `Shared/STATUS.md`.
pub fn status_projection(items: &[WorkItem]) -> String {
    let mut out = String::new();
    out.push_str(STATUS_BEGIN);
    out.push('\n');
    out.push_str("## Work ledger\n\n");
    out.push_str(
        "_Generated from the durable task record. Agent notes outside the markers are preserved; this block is replaced after each run._\n\n",
    );
    out.push_str("| Status | Task | Attempts | Result |\n|---|---|---|---|\n");
    for item in items {
        let detail = item
            .error
            .as_deref()
            .or(item.result.as_deref())
            .unwrap_or("");
        out.push_str(&format!(
            "| {} | {} | {}/{} | {} |\n",
            escape_table(&item.status),
            escape_table(&item.title),
            item.attempts,
            item.max_attempts,
            escape_table(&truncate(detail, 120)),
        ));
    }
    out.push_str(STATUS_END);
    out.push('\n');
    out
}

/// Replace the generated block while preserving everything an agent wrote
/// around it. Running twice is a no-op.
pub fn merge_status_projection(existing: &str, projection: &str) -> String {
    if let (Some(start), Some(end)) = (existing.find(STATUS_BEGIN), existing.find(STATUS_END)) {
        if end > start {
            let mut out = String::with_capacity(existing.len() + projection.len());
            out.push_str(&existing[..start]);
            out.push_str(projection.trim_end());
            out.push_str(&existing[end + STATUS_END.len()..]);
            return out;
        }
    }
    if existing.trim().is_empty() {
        return format!("{}\n", projection.trim_end());
    }
    format!("{}\n\n{}\n", existing.trim_end(), projection.trim_end())
}

fn truncate(text: &str, max: usize) -> String {
    let mut out: String = text.chars().take(max).collect();
    if text.chars().count() > max {
        out.push('…');
    }
    out
}

fn escape_table(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('\n', " ")
}

fn status_for(state: &NodeState) -> &'static str {
    match state {
        NodeState::Pending => "pending",
        NodeState::Running(_) => "running",
        NodeState::Done(_) => "done",
        NodeState::Failed(_) => "failed",
        NodeState::Skipped => "skipped",
    }
}

fn title_for(node: &TaskNode) -> String {
    // Office instructions are prompts, not labels:
    //   [QA Engineer — Testing & automation]
    //   Client request: ...
    //   Office goal:
    //   ...multi-line goal...
    //
    //   Read OFFICE.md ...
    // The durable ledger needs the real task line, not the role header or the
    // first line of the mission. Prefer an explicit `Your task:` section, then
    // skip the role/request/goal preamble and take the first actionable line.
    let instruction = node.instruction.as_str();
    let task = instruction
        .rsplit("\n\nYour task: ")
        .next()
        .unwrap_or(instruction);
    let mut in_goal = false;
    let mut selected: Option<&str> = None;
    for line in task.lines().map(str::trim) {
        if line.is_empty() {
            in_goal = false;
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if line.starts_with('[') && line.ends_with(']') {
            continue;
        }
        if lower.starts_with("client request:") || lower.starts_with("office goal:") {
            in_goal = lower.starts_with("office goal:");
            continue;
        }
        if in_goal {
            continue;
        }
        if lower.starts_with("current board:")
            || lower.starts_with("this cycle:")
            || lower.starts_with("durable project backlog:")
        {
            break;
        }
        selected = Some(line);
        break;
    }
    let first_line = selected.unwrap_or("Task");
    let mut title: String = first_line.chars().take(120).collect();
    if first_line.chars().count() > 120 {
        title.push('…');
    }
    match node.node_type {
        WorkflowNodeType::Agent => title,
        other => format!("[{:?}] {title}", other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ravenbot_db::queries::WorkItemQueries;

    #[tokio::test]
    async fn seeding_mirrors_nodes_edges_and_metadata_without_duplicating() {
        let path = std::env::temp_dir().join(format!("rb-ledger-seed-{}.db", Uuid::new_v4()));
        let db = ravenbot_db::Database::new(&path).await.expect("temp db");
        let office = Uuid::new_v4();
        let mut graph = TaskGraph::new("Build the dashboard");
        let first = graph.add_node(Uuid::new_v4(), "Scaffold the page");
        let second = graph.add_node(Uuid::new_v4(), "Implement the charts");
        graph.add_edge(first, second);

        let ledger = WorkLedger::new(db.pool().clone(), office);
        ledger.seed(&graph, 1).await.expect("seed");
        ledger.seed(&graph, 1).await.expect("reseed");

        let items = WorkItemQueries::list_by_office(db.pool(), office, 10)
            .await
            .expect("list");
        assert_eq!(items.len(), 2, "reseeding must not duplicate tasks");
        let scaffold = items
            .iter()
            .find(|item| item.id == first.to_string())
            .expect("scaffold item");
        assert_eq!(scaffold.title, "Scaffold the page");
        assert_eq!(scaffold.metadata["cycle"], 1);
        assert_eq!(scaffold.metadata["node_type"], "agent");

        let linked: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM work_links WHERE parent_id = ? AND child_id = ?",
        )
        .bind(first.to_string())
        .bind(second.to_string())
        .fetch_one(db.pool())
        .await
        .expect("link count");
        assert_eq!(linked, 1);

        WorkItemQueries::mark_done(db.pool(), &first.to_string(), None, "shipped")
            .await
            .expect("done");
        ledger.seed(&graph, 2).await.expect("reseed after done");
        let stored = WorkItemQueries::get(db.pool(), &first.to_string())
            .await
            .expect("get")
            .expect("row");
        assert_eq!(stored.status, "done");
        assert_eq!(stored.result.as_deref(), Some("shipped"));

        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn a_rescheduled_completed_member_reopens_without_spending_an_attempt() {
        let path = std::env::temp_dir().join(format!("rb-ledger-loop-{}.db", Uuid::new_v4()));
        let db = ravenbot_db::Database::new(&path).await.expect("temp db");
        let office = Uuid::new_v4();
        let mut graph = TaskGraph::new("Loop the review");
        let node_id = graph.add_node(Uuid::new_v4(), "Draft");
        let ledger = WorkLedger::new(db.pool().clone(), office);
        ledger.seed(&graph, 1).await.expect("seed");
        WorkItemQueries::mark_done(db.pool(), &node_id.to_string(), None, "draft one")
            .await
            .expect("done");

        let mut node = graph.nodes.get(&node_id).cloned().expect("node");
        node.state = NodeState::Running(Uuid::new_v4());
        node.iterations = 0; // loop-group reset, not a per-node loop iteration
        ledger.started(&node, Uuid::new_v4()).await;

        let stored = WorkItemQueries::get(db.pool(), &node_id.to_string())
            .await
            .expect("get")
            .expect("row");
        assert_eq!(stored.status, "running");
        assert_eq!(stored.attempts, 0, "reopening is not a retry");

        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn a_done_node_that_reports_incomplete_is_not_projected_as_done() {
        let path = std::env::temp_dir().join(format!("rb-ledger-incomplete-{}.db", Uuid::new_v4()));
        let db = ravenbot_db::Database::new(&path).await.expect("temp db");
        let office = Uuid::new_v4();
        let mut graph = TaskGraph::new("Build the dashboard");
        let node_id = graph.add_node(Uuid::new_v4(), "Write the status report");
        let ledger = WorkLedger::new(db.pool().clone(), office);
        ledger.seed(&graph, 1).await.expect("seed");

        graph
            .mark_running(node_id, Uuid::new_v4())
            .expect("running");
        graph
            .mark_done(
                node_id,
                "## Status: incomplete — the milestone was decided, but not filed".to_string(),
            )
            .expect("done");
        let node = graph.nodes.get(&node_id).expect("node").clone();
        ledger.sync(&node).await;

        let stored = WorkItemQueries::get(db.pool(), &node_id.to_string())
            .await
            .expect("get")
            .expect("row");
        assert_eq!(stored.status, "failed");
        assert!(
            stored
                .error
                .as_deref()
                .is_some_and(|error| error.contains("reported its own task as incomplete")),
            "{:?}",
            stored.error
        );

        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn the_status_projection_is_a_bounded_table_and_keeps_agent_notes() {
        let path = std::env::temp_dir().join(format!("rb-ledger-status-{}.db", Uuid::new_v4()));
        let db = ravenbot_db::Database::new(&path).await.expect("temp db");
        let office = Uuid::new_v4();
        let mut graph = TaskGraph::new("Build the dashboard");
        let first = graph.add_node(Uuid::new_v4(), "Scaffold | shell\npage");
        let second = graph.add_node(Uuid::new_v4(), "Render charts");
        let ledger = WorkLedger::new(db.pool().clone(), office);
        ledger.seed(&graph, 1).await.expect("seed");
        WorkItemQueries::mark_done(db.pool(), &first.to_string(), None, "created index.html")
            .await
            .expect("done");
        WorkItemQueries::mark_failed(db.pool(), &second.to_string(), None, "provider | timeout")
            .await
            .expect("failed");

        let items = WorkItemQueries::list_by_office(db.pool(), office, 500)
            .await
            .expect("items");
        let projection = status_projection(&items);
        assert!(
            projection.contains("| done | Scaffold \\| shell |"),
            "{projection}"
        );
        assert!(
            projection.contains("| failed | Render charts |"),
            "{projection}"
        );
        assert!(projection.contains("provider \\| timeout"), "{projection}");

        let existing = "# Office status\n\nAgent note: keep this.\n";
        let merged = merge_status_projection(existing, &projection);
        assert!(merged.starts_with("# Office status"));
        assert!(merged.contains("Agent note: keep this."));
        assert_eq!(merged.matches(STATUS_BEGIN).count(), 1);
        assert_eq!(merged.matches(STATUS_END).count(), 1);

        let replaced = merge_status_projection(
            &merged,
            "<!-- ravenbot:work-ledger:begin -->\nnew\n<!-- ravenbot:work-ledger:end -->\n",
        );
        assert!(replaced.contains("Agent note: keep this."));
        assert!(replaced.contains("new"));
        assert!(!replaced.contains("Render charts"));

        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn an_office_prompt_title_uses_the_task_not_the_role_header() {
        let mut graph = TaskGraph::new("goal");
        let node_id = graph.add_node(
            Uuid::new_v4(),
            "[QA Engineer — Testing & automation]\n\
             Client request: [Office continuation — cycle 4]\n\
             Office goal:\n\
             Ship the dashboard.\n\
             Keep it offline.\n\n\
             Verify the release and write the acceptance report.\n\
             Do not edit production code.",
        );
        let node = graph.nodes.get(&node_id).expect("node");
        let title = title_for(node);
        assert_eq!(title, "Verify the release and write the acceptance report.");
        assert!(!title.contains("QA Engineer"), "{title}");
        assert!(!title.contains("Office goal"), "{title}");
    }

    #[test]
    fn titles_are_one_line_and_marked_for_non_agent_nodes() {
        let mut graph = TaskGraph::new("goal");
        let node_id = graph.add_node(Uuid::new_v4(), "\n  First useful line\nsecond line");
        let node = graph.nodes.get(&node_id).expect("node");
        assert_eq!(title_for(node), "First useful line");

        let mut graph = TaskGraph::new("goal");
        let node_id = graph.add_node(Uuid::new_v4(), "Aggregate");
        graph
            .set_node_type(node_id, WorkflowNodeType::Output)
            .expect("type");
        let node = graph.nodes.get(&node_id).expect("node");
        assert!(title_for(node).starts_with("[Output]"));
    }
}
