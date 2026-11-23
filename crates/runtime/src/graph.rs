//! Task graph for parallel orchestration
//!
//! This module implements a directed acyclic graph (DAG) for orchestrating
//! multiple bots working on subtasks in parallel.

use ravenbot_core::ChecklistItem;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Unique identifier for a node in the task graph
pub type NodeId = Uuid;

/// State of a task node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeState {
    /// Not yet started
    Pending,
    /// Currently running (with run ID)
    Running(Uuid),
    /// Completed successfully with result
    Done(String),
    /// Failed with error message
    Failed(String),
    /// Skipped (dependency failed or not needed)
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionOperator {
    Contains,
    NotContains,
}

impl ConditionOperator {
    pub fn matches(self, output: &str, value: &str) -> bool {
        let output = output.to_lowercase();
        let value = value.to_lowercase();
        match self {
            Self::Contains => output.contains(&value),
            Self::NotContains => !output.contains(&value),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeCondition {
    /// The upstream node whose output is inspected.
    pub source: NodeId,
    pub operator: ConditionOperator,
    pub value: String,
}

/// Early-stop rule for a looping task. The task stops when its own answer
/// matches the operator/value pair; otherwise it runs up to its iteration cap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopStop {
    pub operator: ConditionOperator,
    pub value: String,
}

/// Explicit workflow node type. `Agent` keeps the original behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowNodeType {
    /// Entry marker: completes immediately and feeds its instruction forward.
    Trigger,
    /// A normal bot run with tools, memory, and approvals.
    #[default]
    Agent,
    /// One tool-free model completion on the assigned bot's provider.
    Llm,
    /// A human approval card. Approved continues; denied fails the node.
    Approval,
    /// Aggregates its upstream outputs into a final artifact.
    Output,
}

impl WorkflowNodeType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trigger => "trigger",
            Self::Agent => "agent",
            Self::Llm => "llm",
            Self::Approval => "approval",
            Self::Output => "output",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "trigger" => Some(Self::Trigger),
            "agent" | "" => Some(Self::Agent),
            "llm" => Some(Self::Llm),
            "approval" => Some(Self::Approval),
            "output" => Some(Self::Output),
            _ => None,
        }
    }
}

/// A named set of tasks that repeats as a unit. The terminal task publishes the
/// group; external dependents are not released until the group finishes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopGroup {
    pub name: String,
    pub members: Vec<NodeId>,
    pub terminal: NodeId,
    pub max_iterations: u32,
    pub iterations: u32,
    pub stop: Option<LoopStop>,
    pub finished: bool,
}

/// A node in the task graph - represents a task for a specific bot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskNode {
    /// Unique identifier
    pub id: NodeId,
    /// Bot that should execute this task
    pub bot_id: Uuid,
    /// Instruction for the bot
    pub instruction: String,
    /// Current state
    pub state: NodeState,
    /// Input data from dependencies (from scratchpad)
    pub input: Option<String>,
    /// Output data (stored in scratchpad when done)
    pub output: Option<String>,
    /// Optional branch condition evaluated against an upstream output.
    pub condition: Option<NodeCondition>,
    /// Explicit upstream nodes whose output is injected. None means every
    /// dependency; Some(empty) means no upstream output.
    pub data_from: Option<Vec<NodeId>>,
    /// How many times this node has started.
    pub attempts: u32,
    /// Total attempts allowed, including the first.
    pub max_attempts: u32,
    /// What this node does.
    #[serde(default)]
    pub node_type: WorkflowNodeType,
    /// Optional JSON Schema the final answer must satisfy.
    pub output_schema: Option<serde_json::Value>,
    /// Total successful iterations allowed (1 = no loop).
    pub max_iterations: u32,
    /// Successful iterations completed so far.
    pub iterations: u32,
    /// Optional early-stop rule checked against each answer.
    pub loop_stop: Option<LoopStop>,
    /// True while the node is waiting to run its next loop iteration. Such a
    /// start is not a retry, so it must not consume the retry budget.
    pub loop_pending: bool,
    /// Thread that owns this node's work. Reused across loop iterations and
    /// retries so one task reads as one conversation, not a pile of threads.
    pub thread_id: Option<Uuid>,
    /// True when this node was skipped because its branch condition was false.
    /// Such a skip is part of the plan, not a failed task.
    pub condition_skipped: bool,
}

/// Shared scratchpad for passing data between tasks
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Blackboard {
    /// Key-value store for sharing data
    pub data: HashMap<String, String>,
}

impl Blackboard {
    /// Create a new empty blackboard
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a value
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.data.insert(key.into(), value.into());
    }

    /// Get a value
    pub fn get(&self, key: &str) -> Option<&str> {
        self.data.get(key).map(|s| s.as_str())
    }

    /// Remove a value
    pub fn remove(&mut self, key: &str) -> Option<String> {
        self.data.remove(key)
    }

    /// Check if a key exists
    pub fn contains(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }
}

/// Directed task graph for parallel execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskGraph {
    /// Unique identifier for this graph execution
    pub id: Uuid,
    /// All nodes in the graph
    pub nodes: HashMap<NodeId, TaskNode>,
    /// Dependency edges: (from_node, to_node)
    /// An edge A -> B means B depends on A
    pub edges: Vec<(NodeId, NodeId)>,
    /// Shared scratchpad for passing data between tasks
    pub blackboard: Blackboard,
    /// Overall goal/instruction
    pub goal: String,
    /// Multi-task loop groups. Empty for ordinary DAGs.
    #[serde(default)]
    pub loop_groups: Vec<LoopGroup>,
}

/// Does a finished node's own report say that its task is incomplete?
///
/// A node can exit cleanly after producing a progress report instead of the
/// artifact it was asked for. The office must not treat that report as a
/// completed milestone: the next cycle would build on work that was never
/// filed. This intentionally matches only an explicit status line or a small
/// set of unambiguous phrases, so normal prose is not misclassified.
pub fn reports_incomplete_work(text: &str) -> bool {
    for raw in text.lines().take(60) {
        let line = raw
            .trim()
            .trim_start_matches(|c: char| matches!(c, '#' | '*' | '-' | '>' | ' '))
            .trim();
        if line.is_empty() {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if let Some(status) = lower.strip_prefix("status:") {
            let status = status.trim_start_matches(|c: char| matches!(c, '*' | '_' | ' '));
            if status.starts_with("incomplete")
                || status.starts_with("partial")
                || status.starts_with("failed")
                || status.starts_with("blocked")
                || status.starts_with("not filed")
            {
                return true;
            }
        }
        if lower.contains("did not finish the task")
            || lower.contains("did not complete the task")
            || lower.contains("no file was created or modified")
            || lower.contains("treat it as unfiled")
        {
            return true;
        }
    }
    false
}

impl TaskGraph {
    /// Create a new empty graph
    pub fn new(goal: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            nodes: HashMap::new(),
            edges: Vec::new(),
            blackboard: Blackboard::new(),
            goal: goal.into(),
            loop_groups: Vec::new(),
        }
    }

    /// Add a node to the graph
    pub fn add_node(&mut self, bot_id: Uuid, instruction: impl Into<String>) -> NodeId {
        let id = Uuid::new_v4();
        self.nodes.insert(
            id,
            TaskNode {
                id,
                bot_id,
                instruction: instruction.into(),
                state: NodeState::Pending,
                input: None,
                output: None,
                condition: None,
                data_from: None,
                attempts: 0,
                max_attempts: 1,
                node_type: WorkflowNodeType::Agent,
                output_schema: None,
                max_iterations: 1,
                iterations: 0,
                loop_stop: None,
                loop_pending: false,
                thread_id: None,
                condition_skipped: false,
            },
        );
        id
    }

    pub fn set_node_type(
        &mut self,
        node_id: NodeId,
        node_type: WorkflowNodeType,
    ) -> Result<(), String> {
        let node = self
            .nodes
            .get_mut(&node_id)
            .ok_or_else(|| format!("Node not found: {node_id}"))?;
        node.node_type = node_type;
        Ok(())
    }

    /// Require this node's final answer to be JSON matching a schema.
    pub fn set_output_schema(
        &mut self,
        node_id: NodeId,
        schema: serde_json::Value,
    ) -> Result<(), String> {
        let node = self
            .nodes
            .get_mut(&node_id)
            .ok_or_else(|| format!("Node not found: {node_id}"))?;
        node.output_schema = Some(schema);
        Ok(())
    }

    /// Run this node repeatedly, with an optional early stop. `iterations` is
    /// the total number of successful runs (1 disables the loop). The node only
    /// publishes an output when it finishes, so dependents never see a partial
    /// iteration.
    pub fn set_loop(
        &mut self,
        node_id: NodeId,
        iterations: u32,
        stop: Option<LoopStop>,
    ) -> Result<(), String> {
        if !(1..=10).contains(&iterations) {
            return Err("A workflow loop must run between 1 and 10 times.".to_string());
        }
        let node = self
            .nodes
            .get_mut(&node_id)
            .ok_or_else(|| format!("Node not found: {node_id}"))?;
        node.max_iterations = iterations;
        node.loop_stop = if iterations > 1 { stop } else { None };
        Ok(())
    }

    /// Record one successful iteration. Returns the next iteration number when
    /// the node should run again, or `None` when this answer is final.
    pub fn mark_loop_iteration(
        &mut self,
        node_id: NodeId,
        output: &str,
    ) -> Result<Option<u32>, String> {
        let node = self
            .nodes
            .get_mut(&node_id)
            .ok_or_else(|| format!("Node not found: {node_id}"))?;
        node.iterations = node.iterations.saturating_add(1);
        let stop = node
            .loop_stop
            .as_ref()
            .is_some_and(|rule| rule.operator.matches(output, &rule.value));
        if stop || node.iterations >= node.max_iterations {
            return Ok(None);
        }
        node.output = None;
        node.state = NodeState::Pending;
        node.loop_pending = true;
        Ok(Some(node.iterations + 1))
    }

    /// Register a multi-task loop group. The terminal task must be a member.
    /// A single-iteration group is a no-op and is not stored.
    pub fn add_loop_group(
        &mut self,
        name: &str,
        members: Vec<NodeId>,
        terminal: NodeId,
        max_iterations: u32,
        stop: Option<LoopStop>,
    ) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 64 {
            return Err("A loop group needs a name of at most 64 characters.".to_string());
        }
        if !(1..=10).contains(&max_iterations) {
            return Err("A loop group must run between 1 and 10 times.".to_string());
        }
        if members.is_empty() {
            return Err("A loop group needs at least one task.".to_string());
        }
        if !members.contains(&terminal) {
            return Err("The loop group terminal must be one of its tasks.".to_string());
        }
        if self.loop_groups.iter().any(|group| group.name == name) {
            return Err(format!("Loop group '{name}' is defined more than once."));
        }
        if members.iter().any(|id| !self.nodes.contains_key(id)) {
            return Err("A loop group references an unknown task.".to_string());
        }
        if max_iterations == 1 {
            return Ok(());
        }
        self.loop_groups.push(LoopGroup {
            name: name.to_string(),
            members,
            terminal,
            max_iterations,
            iterations: 1,
            stop,
            finished: false,
        });
        Ok(())
    }

    /// Advance groups whose members have all completed. Returns the nodes reset
    /// for another iteration, paired with the new iteration number.
    pub fn advance_loop_groups(&mut self) -> Vec<(NodeId, u32)> {
        let mut reset = Vec::new();
        for group in &mut self.loop_groups {
            if group.finished {
                continue;
            }
            if !group.members.iter().all(|id| {
                self.nodes
                    .get(id)
                    .is_some_and(|node| matches!(node.state, NodeState::Done(_)))
            }) {
                continue;
            }
            let stop = self
                .nodes
                .get(&group.terminal)
                .and_then(|node| node.output.as_deref())
                .is_some_and(|output| {
                    group
                        .stop
                        .as_ref()
                        .is_some_and(|rule| rule.operator.matches(output, &rule.value))
                });
            if stop || group.iterations >= group.max_iterations {
                group.finished = true;
                continue;
            }
            group.iterations += 1;
            for id in &group.members {
                if let Some(node) = self.nodes.get_mut(id) {
                    node.state = NodeState::Pending;
                    node.output = None;
                    node.attempts = 0;
                    node.iterations = 0;
                    node.loop_pending = false;
                    node.condition_skipped = false;
                }
                self.blackboard.data.remove(&id.to_string());
                reset.push((*id, group.iterations));
            }
        }
        reset
    }

    /// Set how many additional attempts a failed node gets.
    pub fn set_retries(&mut self, node_id: NodeId, retries: u32) -> Result<(), String> {
        let node = self
            .nodes
            .get_mut(&node_id)
            .ok_or_else(|| format!("Node not found: {node_id}"))?;
        node.max_attempts = retries.saturating_add(1);
        Ok(())
    }

    /// Reset failed nodes that still have attempts left. Call this before the
    /// dependency skip cascade so a retry can still rescue its dependents.
    pub fn retry_failed_nodes(&mut self) -> Vec<NodeId> {
        let retryable: Vec<NodeId> = self
            .nodes
            .values()
            .filter(|node| {
                let NodeState::Failed(error) = &node.state else {
                    return false;
                };
                node.attempts < node.max_attempts && !is_non_retryable_failure(error)
            })
            .map(|node| node.id)
            .collect();
        for node_id in &retryable {
            if let Some(node) = self.nodes.get_mut(node_id) {
                node.state = NodeState::Pending;
            }
        }
        if retryable.is_empty() {
            return retryable;
        }
        // A failed node's dependents were skipped when it failed. Reset the
        // ones that were only skipped because of a now-retrying ancestor, so
        // a successful retry can still finish the branch. Nodes skipped for
        // another reason keep their skipped state.
        let mut candidates: std::collections::HashSet<NodeId> = std::collections::HashSet::new();
        let mut frontier: Vec<NodeId> = retryable.clone();
        while let Some(id) = frontier.pop() {
            for (from, to) in &self.edges {
                if *from == id && candidates.insert(*to) {
                    frontier.push(*to);
                }
            }
        }
        loop {
            let mut restored = false;
            for id in candidates.iter().copied() {
                let Some(node) = self.nodes.get(&id) else {
                    continue;
                };
                if !matches!(node.state, NodeState::Skipped) {
                    continue;
                }
                let viable = self
                    .edges
                    .iter()
                    .filter(|(_, to)| *to == id)
                    .all(|(from, _)| {
                        self.nodes.get(from).is_some_and(|dependency| {
                            matches!(
                                dependency.state,
                                NodeState::Done(_) | NodeState::Pending | NodeState::Running(_)
                            )
                        })
                    });
                if viable {
                    if let Some(node) = self.nodes.get_mut(&id) {
                        node.state = NodeState::Pending;
                    }
                    restored = true;
                }
            }
            if !restored {
                break;
            }
        }
        retryable
    }

    /// Restrict which dependency outputs are injected into this node.
    pub fn set_data_from(&mut self, node_id: NodeId, sources: Vec<NodeId>) -> Result<(), String> {
        let node = self
            .nodes
            .get_mut(&node_id)
            .ok_or_else(|| format!("Node not found: {node_id}"))?;
        node.data_from = Some(sources);
        Ok(())
    }

    /// Attach a branch condition to a node after it has been added.
    pub fn set_condition(
        &mut self,
        node_id: NodeId,
        condition: NodeCondition,
    ) -> Result<(), String> {
        let node = self
            .nodes
            .get_mut(&node_id)
            .ok_or_else(|| format!("Node not found: {node_id}"))?;
        node.condition = Some(condition);
        Ok(())
    }

    /// Skip pending nodes whose branch condition is false. Returns the nodes
    /// that were just skipped. Conditions are only evaluated once their source
    /// is done; a failed source is handled by the normal skip cascade.
    pub fn apply_conditions(&mut self) -> Vec<NodeId> {
        let mut skipped = Vec::new();
        loop {
            let to_skip: Vec<NodeId> = self
                .nodes
                .values()
                .filter(|node| matches!(node.state, NodeState::Pending))
                .filter(|node| self.dependencies_met(node.id))
                .filter_map(|node| {
                    let condition = node.condition.as_ref()?;
                    let source = self.nodes.get(&condition.source)?;
                    let NodeState::Done(output) = &source.state else {
                        return None;
                    };
                    if condition.operator.matches(output, &condition.value) {
                        None
                    } else {
                        Some(node.id)
                    }
                })
                .collect();
            if to_skip.is_empty() {
                break;
            }
            for node_id in to_skip {
                if self.mark_skipped(node_id).is_ok() {
                    if let Some(node) = self.nodes.get_mut(&node_id) {
                        node.condition_skipped = true;
                    }
                    skipped.push(node_id);
                }
            }
        }
        skipped
    }

    /// Add a node with input from a key in the blackboard
    pub fn add_node_with_input(
        &mut self,
        bot_id: Uuid,
        instruction: impl Into<String>,
        input_key: &str,
    ) -> NodeId {
        let id = self.add_node(bot_id, instruction);
        if let Some(node) = self.nodes.get_mut(&id) {
            node.input = self.blackboard.get(input_key).map(|s| s.to_string());
        }
        id
    }

    /// Add a dependency edge: `to` depends on `from`
    pub fn add_edge(&mut self, from: NodeId, to: NodeId) {
        self.edges.push((from, to));
    }

    /// Get all nodes that are ready to run (all dependencies satisfied)
    pub fn ready_nodes(&self) -> Vec<&TaskNode> {
        self.nodes
            .values()
            .filter(|node| {
                matches!(node.state, NodeState::Pending) && self.dependencies_met(node.id)
            })
            .collect()
    }

    /// Check if all dependencies for a node are met
    fn dependencies_met(&self, node_id: NodeId) -> bool {
        self.edges
            .iter()
            .filter(|(_, to)| *to == node_id)
            .all(|(from, _)| {
                self.nodes
                    .get(from)
                    .map(|node| matches!(node.state, NodeState::Done(_)))
                    .unwrap_or(true)
            })
    }

    /// Whether any dependency of this node has failed or been skipped, which
    /// means the node can never run and should be skipped itself.
    fn has_failed_dependency(&self, node_id: NodeId) -> bool {
        self.edges
            .iter()
            .filter(|(_, to)| *to == node_id)
            .any(|(from, _)| {
                self.nodes
                    .get(from)
                    .map(|node| matches!(node.state, NodeState::Failed(_) | NodeState::Skipped))
                    .unwrap_or(false)
            })
    }

    /// Whether a skip cascades only from branches the plan chose not to take.
    /// A dependency that failed makes the skip a real incomplete task.
    fn skip_is_expected(&self, node_id: NodeId) -> bool {
        let mut saw_blocking = false;
        for (from, to) in &self.edges {
            if *to != node_id {
                continue;
            }
            let Some(dependency) = self.nodes.get(from) else {
                continue;
            };
            if !matches!(dependency.state, NodeState::Failed(_) | NodeState::Skipped) {
                continue;
            }
            saw_blocking = true;
            if !(matches!(dependency.state, NodeState::Skipped) && dependency.condition_skipped) {
                return false;
            }
        }
        saw_blocking
    }

    /// Cascade skips: any pending node whose dependency failed or was skipped
    /// is marked `Skipped`. Returns how many nodes were skipped. This lets a
    /// graph with a failed branch still reach completion instead of leaving
    /// dependents pending forever.
    pub fn propagate_skips(&mut self) -> usize {
        let mut skipped = 0;
        loop {
            let to_skip: Vec<NodeId> = self
                .nodes
                .values()
                .filter(|n| {
                    matches!(n.state, NodeState::Pending) && self.has_failed_dependency(n.id)
                })
                .map(|n| n.id)
                .collect();
            if to_skip.is_empty() {
                break;
            }
            for id in to_skip {
                let expected = self.skip_is_expected(id);
                if let Some(node) = self.nodes.get_mut(&id) {
                    node.state = NodeState::Skipped;
                    node.condition_skipped = expected;
                    skipped += 1;
                }
            }
        }
        skipped
    }

    /// Build the input text for a node from its dependencies' outputs, so a
    /// dependent task actually receives the upstream work. Returns `None` when
    /// the node has no satisfied dependencies.
    pub fn input_for(&self, node_id: NodeId) -> Option<String> {
        let allowed = self
            .nodes
            .get(&node_id)
            .and_then(|node| node.data_from.as_ref());
        let mut parts = Vec::new();
        for (from, to) in &self.edges {
            if *to != node_id {
                continue;
            }
            if let Some(allowed) = allowed {
                if !allowed.contains(from) {
                    continue;
                }
            }
            if let Some(dep) = self.nodes.get(from) {
                if let Some(output) = &dep.output {
                    parts.push(format!("### {}\n{}", dep.instruction, output.trim()));
                }
            }
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("\n\n"))
        }
    }

    /// Mark a node as running
    pub fn mark_running(&mut self, node_id: NodeId, run_id: Uuid) -> Result<(), String> {
        let node = self
            .nodes
            .get_mut(&node_id)
            .ok_or_else(|| format!("Node not found: {}", node_id))?;
        if std::mem::take(&mut node.loop_pending) {
            // A loop iteration is a successful continuation, not a retry.
        } else {
            node.attempts = node.attempts.saturating_add(1);
        }
        // If a condition-skipped branch was restored by a retry, it is a live
        // task again; a later dependency skip must count as a real skip.
        node.condition_skipped = false;
        node.state = NodeState::Running(run_id);
        Ok(())
    }

    /// Running nodes and the run id each was executing when the graph was
    /// snapshotted. Startup uses this to load the run checkpoints before
    /// `prepare_resume` resets the nodes to pending.
    pub fn running_node_runs(&self) -> Vec<(NodeId, Uuid)> {
        self.nodes
            .values()
            .filter_map(|node| match node.state {
                NodeState::Running(run_id) => Some((node.id, run_id)),
                _ => None,
            })
            .collect()
    }

    /// Prepare a graph that was interrupted mid-run for re-execution.
    ///
    /// Completed, skipped, and failed nodes keep their state. A node that was
    /// `Running` when the process stopped is reset to `Pending`; its thread id
    /// is returned so the caller can add a "resume from the workspace" note to
    /// that thread. The graph executor will schedule only the pending nodes,
    /// so completed work is never replayed.
    pub fn prepare_resume(&mut self) -> Vec<Uuid> {
        let mut threads = Vec::new();
        for node in self.nodes.values_mut() {
            if matches!(node.state, NodeState::Running(_)) {
                if let Some(thread) = node.thread_id {
                    threads.push(thread);
                }
                node.state = NodeState::Pending;
                node.loop_pending = false;
            }
        }
        threads
    }

    /// Mark a node as done and store output in blackboard
    pub fn mark_done(&mut self, node_id: NodeId, output: String) -> Result<(), String> {
        let node = self
            .nodes
            .get_mut(&node_id)
            .ok_or_else(|| format!("Node not found: {}", node_id))?;
        node.state = NodeState::Done(output.clone());
        node.output = Some(output.clone());

        // Store in blackboard with node ID as key
        self.blackboard.set(node_id.to_string(), output);

        Ok(())
    }

    /// Mark a node as failed
    pub fn mark_failed(&mut self, node_id: NodeId, error: String) -> Result<(), String> {
        let node = self
            .nodes
            .get_mut(&node_id)
            .ok_or_else(|| format!("Node not found: {}", node_id))?;
        node.state = NodeState::Failed(error);
        Ok(())
    }

    /// Skip a node (e.g., because a dependency failed)
    pub fn mark_skipped(&mut self, node_id: NodeId) -> Result<(), String> {
        let node = self
            .nodes
            .get_mut(&node_id)
            .ok_or_else(|| format!("Node not found: {}", node_id))?;
        node.state = NodeState::Skipped;
        Ok(())
    }

    /// Get all nodes
    pub fn all_nodes(&self) -> Vec<&TaskNode> {
        self.nodes.values().collect()
    }

    /// Get all running nodes
    pub fn running_nodes(&self) -> Vec<&TaskNode> {
        self.nodes
            .values()
            .filter(|node| matches!(node.state, NodeState::Running(_)))
            .collect()
    }

    /// Check if the graph is complete (all nodes done or failed/skipped)
    pub fn is_complete(&self) -> bool {
        self.nodes.values().all(|node| {
            matches!(
                node.state,
                NodeState::Done(_) | NodeState::Failed(_) | NodeState::Skipped
            )
        })
    }

    /// Check for a true deadlock: not complete, nothing ready, and nothing
    /// running — i.e. pending nodes are permanently blocked (a cycle, or a
    /// dependency that can never be satisfied). Previously this required a
    /// *running* node, so a real deadlock reported `false` and the executor
    /// spun forever.
    pub fn has_deadlock(&self) -> bool {
        !self.is_complete() && self.ready_nodes().is_empty() && self.running_nodes().is_empty()
    }

    /// Get the count of each state
    pub fn state_counts(&self) -> HashMap<String, usize> {
        let mut counts = HashMap::new();
        for node in self.nodes.values() {
            let state = match &node.state {
                NodeState::Pending => "pending",
                NodeState::Running(_) => "running",
                NodeState::Done(_) => "done",
                NodeState::Failed(_) => "failed",
                NodeState::Skipped => "skipped",
            };
            *counts.entry(state.to_string()).or_insert(0) += 1;
        }
        counts
    }

    /// Generate checklist items from nodes
    /// Nodes that genuinely failed or were blocked by a failed dependency.
    /// Branch-condition skips are excluded: choosing not to run a branch is a
    /// successful plan outcome, not a failed run.
    pub fn incomplete_nodes(&self) -> Vec<(NodeId, Uuid, String, String)> {
        self.topological_sort()
            .iter()
            .filter_map(|id| {
                let node = self.nodes.get(id)?;
                match &node.state {
                    NodeState::Failed(error) => Some((
                        node.id,
                        node.bot_id,
                        node.instruction.clone(),
                        error.clone(),
                    )),
                    NodeState::Done(result)
                        if matches!(
                            node.node_type,
                            WorkflowNodeType::Agent | WorkflowNodeType::Llm
                        ) && reports_incomplete_work(result) =>
                    {
                        Some((
                            node.id,
                            node.bot_id,
                            node.instruction.clone(),
                            "the node reported its own task as incomplete".to_string(),
                        ))
                    }
                    NodeState::Skipped if !node.condition_skipped => Some((
                        node.id,
                        node.bot_id,
                        node.instruction.clone(),
                        "skipped because a dependency did not complete".to_string(),
                    )),
                    _ => None,
                }
            })
            .collect()
    }

    pub fn to_checklist(&self) -> Vec<ChecklistItem> {
        // Sort nodes by dependency order (topological sort)
        let sorted = self.topological_sort();

        sorted
            .iter()
            .filter_map(|id| self.nodes.get(id))
            .map(|node| match &node.state {
                NodeState::Done(result)
                    if matches!(
                        node.node_type,
                        WorkflowNodeType::Agent | WorkflowNodeType::Llm
                    ) && reports_incomplete_work(result) =>
                {
                    ChecklistItem {
                        label: node.instruction.clone(),
                        status: ravenbot_core::ChecklistStatus::Failed,
                        result: Some(
                            "the node reported its own task as incomplete; its report is not a completed artifact"
                                .to_string(),
                        ),
                        thread_id: None,
                        bot_id: Some(node.bot_id),
                    }
                }
                NodeState::Done(result) => ChecklistItem {
                    label: node.instruction.clone(),
                    status: ravenbot_core::ChecklistStatus::Completed,
                    result: Some(result.clone()),
                    thread_id: None,
                    bot_id: Some(node.bot_id),
                },
                NodeState::Failed(error) => ChecklistItem {
                    label: node.instruction.clone(),
                    status: ravenbot_core::ChecklistStatus::Failed,
                    result: Some(error.clone()),
                    thread_id: None,
                    bot_id: Some(node.bot_id),
                },
                NodeState::Skipped => ChecklistItem {
                    label: node.instruction.clone(),
                    status: ravenbot_core::ChecklistStatus::Skipped,
                    result: None,
                    thread_id: None,
                    bot_id: Some(node.bot_id),
                },
                NodeState::Running(_) => ChecklistItem {
                    label: node.instruction.clone(),
                    status: ravenbot_core::ChecklistStatus::InProgress,
                    result: None,
                    thread_id: None,
                    bot_id: Some(node.bot_id),
                },
                NodeState::Pending => ChecklistItem {
                    label: node.instruction.clone(),
                    status: ravenbot_core::ChecklistStatus::Pending,
                    result: None,
                    thread_id: None,
                    bot_id: Some(node.bot_id),
                },
            })
            .collect()
    }

    /// Topological sort of nodes (Kahn's algorithm)
    /// Node ids in dependency order (for posting room replies as a
    /// readable conversation rather than completion order).
    pub fn ordered_node_ids(&self) -> Vec<NodeId> {
        self.topological_sort()
    }

    fn topological_sort(&self) -> Vec<NodeId> {
        let mut in_degree: HashMap<NodeId, usize> = HashMap::new();
        let mut adjacency: HashMap<NodeId, Vec<NodeId>> = HashMap::new();

        // Initialize
        for id in self.nodes.keys() {
            in_degree.entry(*id).or_insert(0);
            adjacency.entry(*id).or_default();
        }

        // Build adjacency and in-degree
        for (from, to) in &self.edges {
            adjacency.entry(*from).or_default().push(*to);
            *in_degree.entry(*to).or_insert(0) += 1;
        }

        // Start with nodes that have no dependencies
        let mut queue: Vec<NodeId> = in_degree
            .iter()
            .filter(|(_, &degree)| degree == 0)
            .map(|(&id, _)| id)
            .collect();
        queue.sort(); // Deterministic order

        let mut sorted = Vec::new();

        while !queue.is_empty() {
            let node_id = queue.remove(0);
            sorted.push(node_id);

            if let Some(neighbors) = adjacency.get(&node_id) {
                for &neighbor in neighbors {
                    if let Some(degree) = in_degree.get_mut(&neighbor) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push(neighbor);
                            queue.sort(); // Keep deterministic
                        }
                    }
                }
            }
        }

        sorted
    }
}

impl Default for TaskGraph {
    fn default() -> Self {
        Self::new("")
    }
}

/// Cancellation, kill switch, budget, denial, and configuration failures are
/// not transient. Retrying them would fight the user or repeat a permanent
/// error up to four times.
fn is_non_retryable_failure(error: &str) -> bool {
    let error = error.to_lowercase();
    error.contains("cancel")
        || error.contains("kill switch")
        || error.contains("budget exceeded")
        || error.contains("denied by user")
        || crate::is_config_error(&error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_graph_is_complete() {
        let graph = TaskGraph::new("test");
        assert!(graph.is_complete());
    }

    #[test]
    fn test_single_node_graph() {
        let mut graph = TaskGraph::new("test");
        let bot_id = Uuid::new_v4();
        let node_id = graph.add_node(bot_id, "Do something");

        assert!(!graph.is_complete());
        assert_eq!(graph.ready_nodes().len(), 1);
        assert_eq!(graph.ready_nodes()[0].id, node_id);
    }

    #[test]
    fn test_dependency_chain() {
        let mut graph = TaskGraph::new("test");
        let bot_id = Uuid::new_v4();

        let a = graph.add_node(bot_id, "Task A");
        let b = graph.add_node(bot_id, "Task B");
        let c = graph.add_node(bot_id, "Task C");

        graph.add_edge(a, b);
        graph.add_edge(b, c);

        // Only A should be ready initially
        let ready = graph.ready_nodes();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, a);
    }

    #[test]
    fn test_parallel_execution() {
        let mut graph = TaskGraph::new("test");
        let bot_id = Uuid::new_v4();

        let a = graph.add_node(bot_id, "Task A");
        let b = graph.add_node(bot_id, "Task B");
        let c = graph.add_node(bot_id, "Task C");

        // C depends on both A and B
        graph.add_edge(a, c);
        graph.add_edge(b, c);

        // Both A and B should be ready initially
        let ready = graph.ready_nodes();
        assert_eq!(ready.len(), 2);
    }

    #[test]
    fn test_mark_done_releases_dependents() {
        let mut graph = TaskGraph::new("test");
        let bot_id = Uuid::new_v4();

        let a = graph.add_node(bot_id, "Task A");
        let b = graph.add_node(bot_id, "Task B");

        graph.add_edge(a, b);

        // Initially only A is ready
        assert_eq!(graph.ready_nodes().len(), 1);

        // Mark A as done
        graph.mark_done(a, "Result A".to_string()).unwrap();

        // Now B should be ready
        assert_eq!(graph.ready_nodes().len(), 1);
        assert_eq!(graph.ready_nodes()[0].id, b);
    }

    #[test]
    fn input_ports_limit_injected_dependency_outputs() {
        let mut graph = TaskGraph::new("test");
        let bot_id = Uuid::new_v4();
        let a = graph.add_node(bot_id, "A");
        let b = graph.add_node(bot_id, "B");
        let c = graph.add_node(bot_id, "C");
        graph.add_edge(a, c);
        graph.add_edge(b, c);
        graph.mark_done(a, "output A".to_string()).unwrap();
        graph.mark_done(b, "output B".to_string()).unwrap();

        let all = graph.input_for(c).expect("all dependencies");
        assert!(all.contains("output A"));
        assert!(all.contains("output B"));

        graph.set_data_from(c, vec![a]).unwrap();
        let only_a = graph.input_for(c).expect("one input");
        assert!(only_a.contains("output A"));
        assert!(!only_a.contains("output B"));

        graph.set_data_from(c, vec![]).unwrap();
        assert!(graph.input_for(c).is_none());
    }

    #[test]
    fn test_blackboard_sharing() {
        let mut graph = TaskGraph::new("test");
        let bot_id = Uuid::new_v4();

        // Set initial data
        graph.blackboard.set("input", "initial data");

        let a = graph.add_node_with_input(bot_id, "Process input", "input");

        // A can read from blackboard
        assert_eq!(graph.nodes[&a].input.as_deref(), Some("initial data"));

        // Mark A as done with output
        graph.mark_done(a, "processed data".to_string()).unwrap();

        // Output should be in blackboard
        assert_eq!(
            graph.blackboard.get(&a.to_string()).unwrap(),
            "processed data"
        );
    }

    #[test]
    fn test_failed_dependency_propagates_skip() {
        let mut graph = TaskGraph::new("test");
        let bot_id = Uuid::new_v4();
        let a = graph.add_node(bot_id, "A");
        let b = graph.add_node(bot_id, "B");
        let c = graph.add_node(bot_id, "C");
        graph.add_edge(a, b);
        graph.add_edge(b, c);

        // A fails → B (depends on A) and then C (depends on B) must skip.
        graph.mark_failed(a, "boom".to_string()).unwrap();
        assert_eq!(graph.propagate_skips(), 2);
        assert!(
            graph.is_complete(),
            "graph must complete after skip cascade"
        );
        assert!(matches!(graph.nodes[&b].state, NodeState::Skipped));
        assert!(matches!(graph.nodes[&c].state, NodeState::Skipped));
    }

    fn condition(source: NodeId, operator: ConditionOperator, value: &str) -> NodeCondition {
        NodeCondition {
            source,
            operator,
            value: value.to_string(),
        }
    }

    #[test]
    fn condition_matches_case_insensitively() {
        assert!(ConditionOperator::Contains.matches("Approved by Finance", "approved"));
        assert!(!ConditionOperator::Contains.matches("Rejected", "approved"));
        assert!(ConditionOperator::NotContains.matches("Rejected", "approved"));
        assert!(!ConditionOperator::NotContains.matches("Approved", "approved"));
    }

    #[test]
    fn true_condition_releases_the_dependent() {
        let mut graph = TaskGraph::new("test");
        let bot_id = Uuid::new_v4();
        let a = graph.add_node(bot_id, "Review");
        let b = graph.add_node(bot_id, "Ship");
        graph.add_edge(a, b);
        graph
            .set_condition(b, condition(a, ConditionOperator::Contains, "approved"))
            .unwrap();

        graph
            .mark_done(a, "Approved by finance".to_string())
            .unwrap();
        assert!(graph.apply_conditions().is_empty());
        let ready = graph.ready_nodes();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, b);
    }

    #[test]
    fn false_condition_skips_the_node_and_cascades() {
        let mut graph = TaskGraph::new("test");
        let bot_id = Uuid::new_v4();
        let a = graph.add_node(bot_id, "Review");
        let b = graph.add_node(bot_id, "Ship");
        let c = graph.add_node(bot_id, "Announce");
        graph.add_edge(a, b);
        graph.add_edge(b, c);
        graph
            .set_condition(b, condition(a, ConditionOperator::Contains, "approved"))
            .unwrap();

        graph.mark_done(a, "Rejected".to_string()).unwrap();
        assert_eq!(graph.apply_conditions(), vec![b]);
        assert_eq!(graph.propagate_skips(), 1);
        assert!(graph.is_complete());
        assert!(matches!(graph.nodes[&b].state, NodeState::Skipped));
        assert!(matches!(graph.nodes[&c].state, NodeState::Skipped));
    }

    #[test]
    fn condition_waits_until_its_source_is_done() {
        let mut graph = TaskGraph::new("test");
        let bot_id = Uuid::new_v4();
        let a = graph.add_node(bot_id, "Review");
        let b = graph.add_node(bot_id, "Ship");
        graph.add_edge(a, b);
        graph
            .set_condition(b, condition(a, ConditionOperator::NotContains, "approved"))
            .unwrap();
        graph.mark_running(a, Uuid::new_v4()).unwrap();
        assert!(graph.apply_conditions().is_empty());
        assert!(!matches!(graph.nodes[&b].state, NodeState::Skipped));
    }

    #[test]
    fn failed_nodes_retry_until_attempts_are_exhausted() {
        let mut graph = TaskGraph::new("test");
        let bot_id = Uuid::new_v4();
        let node = graph.add_node(bot_id, "Flaky task");
        graph.set_retries(node, 1).unwrap();

        graph.mark_running(node, Uuid::new_v4()).unwrap();
        assert_eq!(graph.nodes[&node].attempts, 1);
        graph.mark_failed(node, "transient".to_string()).unwrap();
        assert_eq!(graph.retry_failed_nodes(), vec![node]);
        assert!(matches!(graph.nodes[&node].state, NodeState::Pending));
        assert_eq!(graph.ready_nodes().len(), 1);

        graph.mark_running(node, Uuid::new_v4()).unwrap();
        assert_eq!(graph.nodes[&node].attempts, 2);
        graph.mark_failed(node, "still broken".to_string()).unwrap();
        assert!(graph.retry_failed_nodes().is_empty());
        assert!(matches!(graph.nodes[&node].state, NodeState::Failed(_)));
    }

    #[test]
    fn condition_branch_skips_are_not_run_failures() {
        let mut graph = TaskGraph::new("test");
        let source = graph.add_node(Uuid::new_v4(), "Review");
        let branch = graph.add_node(Uuid::new_v4(), "Publish only if approved");
        let descendant = graph.add_node(Uuid::new_v4(), "Notify subscribers");
        graph.add_edge(source, branch);
        graph.add_edge(branch, descendant);
        graph
            .set_condition(
                branch,
                condition(source, ConditionOperator::Contains, "approved"),
            )
            .unwrap();
        graph.mark_running(source, Uuid::new_v4()).unwrap();
        graph.mark_done(source, "rejected".to_string()).unwrap();
        assert_eq!(graph.apply_conditions(), vec![branch]);
        assert_eq!(graph.propagate_skips(), 1);
        assert!(graph.incomplete_nodes().is_empty());
        assert!(graph.nodes[&descendant].condition_skipped);
    }

    #[test]
    fn an_explicit_incomplete_status_is_not_a_completed_node() {
        assert!(reports_incomplete_work(
            "## Status: incomplete — the milestone was decided, but not filed"
        ));
        assert!(reports_incomplete_work(
            "STATUS: **incomplete**\n\nNo artifact was produced."
        ));
        assert!(reports_incomplete_work(
            "The node produced a progress report but did not finish the task."
        ));
        assert!(reports_incomplete_work(
            "No file was created or modified. Treat it as unfiled."
        ));
        assert!(!reports_incomplete_work(
            "## Completed — the tests pass and the artifact is filed."
        ));
        assert!(!reports_incomplete_work(
            "Status: done. All criteria verified."
        ));
    }

    #[test]
    fn dependency_skips_are_run_failures() {
        let mut graph = TaskGraph::new("test");
        let first = graph.add_node(Uuid::new_v4(), "First");
        let second = graph.add_node(Uuid::new_v4(), "Second");
        graph.add_edge(first, second);
        graph.mark_running(first, Uuid::new_v4()).unwrap();
        graph
            .mark_failed(first, "provider error".to_string())
            .unwrap();
        graph.propagate_skips();
        let incomplete = graph.incomplete_nodes();
        assert_eq!(incomplete.len(), 2);
        assert!(incomplete
            .iter()
            .any(|(_, _, _, detail)| detail == "provider error"));
        assert!(incomplete
            .iter()
            .any(|(_, _, _, detail)| detail.contains("dependency did not complete")));
    }

    #[test]
    fn a_loop_group_repeats_as_a_unit_and_holds_external_dependents() {
        let mut graph = TaskGraph::new("test");
        let a = graph.add_node(Uuid::new_v4(), "A");
        let b = graph.add_node(Uuid::new_v4(), "B");
        let c = graph.add_node(Uuid::new_v4(), "After the group");
        graph.add_edge(a, b);
        graph.add_edge(b, c);
        graph
            .add_loop_group("refine", vec![a, b], b, 2, None)
            .unwrap();

        graph.mark_running(a, Uuid::new_v4()).unwrap();
        graph.mark_done(a, "a1".to_string()).unwrap();
        graph.mark_running(b, Uuid::new_v4()).unwrap();
        graph.mark_done(b, "b1".to_string()).unwrap();

        let reset = graph.advance_loop_groups();
        assert_eq!(reset.len(), 2);
        assert!(matches!(graph.nodes[&a].state, NodeState::Pending));
        assert!(matches!(graph.nodes[&b].state, NodeState::Pending));
        assert!(graph.blackboard.get(&b.to_string()).is_none());
        assert!(!graph.ready_nodes().iter().any(|node| node.id == c));

        graph.mark_running(a, Uuid::new_v4()).unwrap();
        graph.mark_done(a, "a2".to_string()).unwrap();
        graph.mark_running(b, Uuid::new_v4()).unwrap();
        graph.mark_done(b, "b2".to_string()).unwrap();
        assert!(graph.advance_loop_groups().is_empty());
        assert!(matches!(graph.nodes[&a].state, NodeState::Done(_)));
        assert!(matches!(graph.nodes[&b].state, NodeState::Done(_)));
        assert!(graph.ready_nodes().iter().any(|node| node.id == c));
    }

    #[test]
    fn a_loop_group_stops_early_on_the_terminal_answer() {
        let mut graph = TaskGraph::new("test");
        let a = graph.add_node(Uuid::new_v4(), "A");
        let b = graph.add_node(Uuid::new_v4(), "B");
        graph.add_edge(a, b);
        graph
            .add_loop_group(
                "refine",
                vec![a, b],
                b,
                5,
                Some(LoopStop {
                    operator: ConditionOperator::Contains,
                    value: "APPROVED".to_string(),
                }),
            )
            .unwrap();

        graph.mark_running(a, Uuid::new_v4()).unwrap();
        graph.mark_done(a, "draft".to_string()).unwrap();
        graph.mark_running(b, Uuid::new_v4()).unwrap();
        graph.mark_done(b, "APPROVED".to_string()).unwrap();
        assert!(graph.advance_loop_groups().is_empty());
        assert!(matches!(graph.nodes[&a].state, NodeState::Done(_)));
        assert!(matches!(graph.nodes[&b].state, NodeState::Done(_)));
    }

    #[test]
    fn loop_group_definitions_are_validated() {
        let mut graph = TaskGraph::new("test");
        let a = graph.add_node(Uuid::new_v4(), "A");
        let b = graph.add_node(Uuid::new_v4(), "B");
        assert!(graph.add_loop_group("", vec![a], a, 2, None).is_err());
        assert!(graph.add_loop_group("g", vec![], a, 2, None).is_err());
        assert!(graph.add_loop_group("g", vec![a], b, 2, None).is_err());
        assert!(graph.add_loop_group("g", vec![a], a, 0, None).is_err());
        assert!(graph.add_loop_group("g", vec![a], a, 11, None).is_err());
        assert!(graph.add_loop_group("g", vec![a], a, 2, None).is_ok());
        assert!(graph.add_loop_group("g", vec![b], b, 2, None).is_err());
    }

    #[test]
    fn loop_stops_when_the_answer_matches_and_publishes_once() {
        let mut graph = TaskGraph::new("test");
        let node = graph.add_node(Uuid::new_v4(), "Refine");
        graph
            .set_loop(
                node,
                3,
                Some(LoopStop {
                    operator: ConditionOperator::Contains,
                    value: "APPROVED".to_string(),
                }),
            )
            .unwrap();

        graph.mark_running(node, Uuid::new_v4()).unwrap();
        assert_eq!(
            graph.mark_loop_iteration(node, "NEEDS WORK").unwrap(),
            Some(2)
        );
        assert!(matches!(graph.nodes[&node].state, NodeState::Pending));
        assert!(graph.ready_nodes().iter().any(|n| n.id == node));

        graph.mark_running(node, Uuid::new_v4()).unwrap();
        assert_eq!(graph.mark_loop_iteration(node, "APPROVED").unwrap(), None);
        graph.mark_done(node, "APPROVED".to_string()).unwrap();
        assert_eq!(graph.blackboard.get(&node.to_string()), Some("APPROVED"));
        assert_eq!(graph.nodes[&node].iterations, 2);
        // Loop starts are continuations, not retries.
        assert_eq!(graph.nodes[&node].attempts, 1);
    }

    #[test]
    fn loop_runs_to_its_cap_without_an_early_stop() {
        let mut graph = TaskGraph::new("test");
        let node = graph.add_node(Uuid::new_v4(), "Iterate");
        graph.set_loop(node, 3, None).unwrap();

        graph.mark_running(node, Uuid::new_v4()).unwrap();
        assert_eq!(graph.mark_loop_iteration(node, "one").unwrap(), Some(2));
        graph.mark_running(node, Uuid::new_v4()).unwrap();
        assert_eq!(graph.mark_loop_iteration(node, "two").unwrap(), Some(3));
        graph.mark_running(node, Uuid::new_v4()).unwrap();
        assert_eq!(graph.mark_loop_iteration(node, "three").unwrap(), None);
        assert_eq!(graph.nodes[&node].iterations, 3);
        assert_eq!(graph.nodes[&node].attempts, 1);
    }

    #[test]
    fn loop_bounds_are_validated() {
        let mut graph = TaskGraph::new("test");
        let node = graph.add_node(Uuid::new_v4(), "Iterate");
        assert!(graph.set_loop(node, 0, None).is_err());
        assert!(graph.set_loop(node, 11, None).is_err());
        assert!(graph.set_loop(node, 10, None).is_ok());
        assert!(!graph.nodes[&node].loop_pending);
    }

    #[test]
    fn retrying_a_node_restores_its_skipped_dependents() {
        let mut graph = TaskGraph::new("test");
        let bot_id = Uuid::new_v4();
        let first = graph.add_node(bot_id, "First");
        let second = graph.add_node(bot_id, "Second");
        let third = graph.add_node(bot_id, "Third");
        graph.add_edge(first, second);
        graph.add_edge(second, third);
        graph.set_retries(first, 1).unwrap();

        graph.mark_running(first, Uuid::new_v4()).unwrap();
        graph.mark_failed(first, "transient".to_string()).unwrap();
        graph.mark_skipped(second).unwrap();
        graph.mark_skipped(third).unwrap();

        assert_eq!(graph.retry_failed_nodes(), vec![first]);
        assert!(matches!(graph.nodes[&first].state, NodeState::Pending));
        assert!(matches!(graph.nodes[&second].state, NodeState::Pending));
        assert!(matches!(graph.nodes[&third].state, NodeState::Pending));
    }

    #[test]
    fn retrying_keeps_dependents_skipped_for_another_failure() {
        let mut graph = TaskGraph::new("test");
        let bot_id = Uuid::new_v4();
        let retried = graph.add_node(bot_id, "Retried");
        let broken = graph.add_node(bot_id, "Broken");
        let dependent = graph.add_node(bot_id, "Dependent");
        graph.add_edge(retried, dependent);
        graph.add_edge(broken, dependent);
        graph.set_retries(retried, 1).unwrap();

        graph.mark_running(retried, Uuid::new_v4()).unwrap();
        graph.mark_failed(retried, "transient".to_string()).unwrap();
        graph.mark_running(broken, Uuid::new_v4()).unwrap();
        graph.mark_failed(broken, "permanent".to_string()).unwrap();
        graph.mark_skipped(dependent).unwrap();

        assert_eq!(graph.retry_failed_nodes(), vec![retried]);
        assert!(matches!(graph.nodes[&dependent].state, NodeState::Skipped));
    }

    #[test]
    fn cancellation_and_permanent_failures_are_not_retried() {
        for message in [
            "Cancelled: User cancelled",
            "Kill switch active: paused",
            "Budget exceeded: monthly limit",
            "API key is invalid",
        ] {
            let mut graph = TaskGraph::new("test");
            let node = graph.add_node(Uuid::new_v4(), "Do not retry");
            graph.set_retries(node, 3).unwrap();
            graph.mark_running(node, Uuid::new_v4()).unwrap();
            graph.mark_failed(node, message.to_string()).unwrap();
            assert!(
                graph.retry_failed_nodes().is_empty(),
                "{message} must not be retried"
            );
        }
    }

    #[test]
    fn retries_are_off_by_default() {
        let mut graph = TaskGraph::new("test");
        let bot_id = Uuid::new_v4();
        let node = graph.add_node(bot_id, "One shot");
        graph.mark_running(node, Uuid::new_v4()).unwrap();
        graph.mark_failed(node, "boom".to_string()).unwrap();
        assert!(graph.retry_failed_nodes().is_empty());
    }

    #[test]
    fn test_true_deadlock_is_detected() {
        let mut graph = TaskGraph::new("test");
        let bot_id = Uuid::new_v4();
        let a = graph.add_node(bot_id, "A");
        let b = graph.add_node(bot_id, "B");
        // Create a cycle A <-> B: neither can ever be ready.
        graph.add_edge(a, b);
        graph.add_edge(b, a);

        assert!(graph.ready_nodes().is_empty());
        assert!(graph.running_nodes().is_empty());
        assert!(!graph.is_complete());
        assert!(graph.has_deadlock(), "a blocked cycle must be detected");
    }

    #[test]
    fn test_running_graph_is_not_a_deadlock() {
        let mut graph = TaskGraph::new("test");
        let bot_id = Uuid::new_v4();
        let a = graph.add_node(bot_id, "A");
        graph.mark_running(a, Uuid::new_v4()).unwrap();
        // Nothing ready, nothing pending-run, but a task is running: not a deadlock.
        assert!(!graph.has_deadlock());
    }

    #[test]
    fn resume_keeps_completed_work_and_reschedules_the_interrupted_node() {
        let mut graph = TaskGraph::new("goal");
        let bot = Uuid::new_v4();
        let a = graph.add_node(bot, "A");
        let b = graph.add_node(bot, "B");
        let c = graph.add_node(bot, "C");
        graph.add_edge(a, b);
        graph.add_edge(b, c);

        graph.mark_running(a, Uuid::new_v4()).unwrap();
        graph.mark_done(a, "A done".to_string()).unwrap();
        let interrupted_thread = Uuid::new_v4();
        let interrupted_run = Uuid::new_v4();
        graph.nodes.get_mut(&b).unwrap().thread_id = Some(interrupted_thread);
        graph.mark_running(b, interrupted_run).unwrap();

        // The persisted form is what the office graph table stores.
        let json = serde_json::to_string(&graph).expect("serialize graph");
        let mut loaded: TaskGraph = serde_json::from_str(&json).expect("deserialize graph");
        assert_eq!(loaded.running_node_runs(), vec![(b, interrupted_run)]);
        let resumed_threads = loaded.prepare_resume();

        assert_eq!(resumed_threads, vec![interrupted_thread]);
        assert!(matches!(loaded.nodes[&a].state, NodeState::Done(_)));
        assert_eq!(loaded.nodes[&a].output.as_deref(), Some("A done"));
        assert!(matches!(loaded.nodes[&b].state, NodeState::Pending));
        assert!(matches!(loaded.nodes[&c].state, NodeState::Pending));
        let ready: Vec<Uuid> = loaded.ready_nodes().iter().map(|node| node.id).collect();
        assert_eq!(ready, vec![b], "only the interrupted node is ready");
    }

    #[test]
    fn test_topological_sort() {
        let mut graph = TaskGraph::new("test");
        let bot_id = Uuid::new_v4();

        let a = graph.add_node(bot_id, "A");
        let b = graph.add_node(bot_id, "B");
        let c = graph.add_node(bot_id, "C");

        graph.add_edge(a, b);
        graph.add_edge(a, c);

        let sorted = graph.topological_sort();

        // A should come before B and C
        let a_pos = sorted.iter().position(|&id| id == a).unwrap();
        let b_pos = sorted.iter().position(|&id| id == b).unwrap();
        let c_pos = sorted.iter().position(|&id| id == c).unwrap();

        assert!(a_pos < b_pos);
        assert!(a_pos < c_pos);
    }
}
