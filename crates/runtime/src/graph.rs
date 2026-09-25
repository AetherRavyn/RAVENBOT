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
            },
        );
        id
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
                matches!(node.state, NodeState::Pending)
                    && self.dependencies_met(node.id)
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
                .filter(|n| matches!(n.state, NodeState::Pending) && self.has_failed_dependency(n.id))
                .map(|n| n.id)
                .collect();
            if to_skip.is_empty() {
                break;
            }
            for id in to_skip {
                if let Some(node) = self.nodes.get_mut(&id) {
                    node.state = NodeState::Skipped;
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
        let mut parts = Vec::new();
        for (from, to) in &self.edges {
            if *to != node_id {
                continue;
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
        let node = self.nodes.get_mut(&node_id)
            .ok_or_else(|| format!("Node not found: {}", node_id))?;
        node.state = NodeState::Running(run_id);
        Ok(())
    }

    /// Mark a node as done and store output in blackboard
    pub fn mark_done(&mut self, node_id: NodeId, output: String) -> Result<(), String> {
        let node = self.nodes.get_mut(&node_id)
            .ok_or_else(|| format!("Node not found: {}", node_id))?;
        node.state = NodeState::Done(output.clone());
        node.output = Some(output.clone());
        
        // Store in blackboard with node ID as key
        self.blackboard.set(node_id.to_string(), output);
        
        Ok(())
    }

    /// Mark a node as failed
    pub fn mark_failed(&mut self, node_id: NodeId, error: String) -> Result<(), String> {
        let node = self.nodes.get_mut(&node_id)
            .ok_or_else(|| format!("Node not found: {}", node_id))?;
        node.state = NodeState::Failed(error);
        Ok(())
    }

    /// Skip a node (e.g., because a dependency failed)
    pub fn mark_skipped(&mut self, node_id: NodeId) -> Result<(), String> {
        let node = self.nodes.get_mut(&node_id)
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
        self.nodes.values()
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
    pub fn to_checklist(&self) -> Vec<ChecklistItem> {
        // Sort nodes by dependency order (topological sort)
        let sorted = self.topological_sort();
        
        sorted.iter()
            .filter_map(|id| self.nodes.get(id))
            .map(|node| match &node.state {
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
        let mut queue: Vec<NodeId> = in_degree.iter()
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
        assert_eq!(graph.blackboard.get(&a.to_string()).unwrap(), "processed data");
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
        assert!(graph.is_complete(), "graph must complete after skip cascade");
        assert!(matches!(graph.nodes[&b].state, NodeState::Skipped));
        assert!(matches!(graph.nodes[&c].state, NodeState::Skipped));
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
