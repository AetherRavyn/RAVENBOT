//! Core sandbox implementation

use crate::resource_limits::ResourceLimits;
use crate::network_policy::NetworkPolicy;
use ravenbot_core::SandboxTier;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Configuration for a bot's sandbox
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Sandbox tier (OS-level, Docker, or Host)
    pub tier: SandboxTier,
    /// Resource limits
    pub resource_limits: ResourceLimits,
    /// Network policy
    pub network_policy: NetworkPolicy,
    /// Allowed paths in the filesystem
    pub allowed_paths: Vec<String>,
    /// Blocked paths (takes precedence over allowed)
    pub blocked_paths: Vec<String>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            tier: SandboxTier::OsLevel,
            resource_limits: ResourceLimits::default(),
            network_policy: NetworkPolicy::default(),
            allowed_paths: vec!["/tmp".to_string(), "./data".to_string()],
            blocked_paths: vec![
                "/etc/shadow".to_string(),
                "/etc/passwd".to_string(),
                "~/.ssh".to_string(),
            ],
        }
    }
}

/// Status of a sandbox
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SandboxStatus {
    /// Sandbox is created but not running
    Created,
    /// Sandbox is actively running
    Running,
    /// Sandbox is paused
    Paused,
    /// Sandbox has been terminated
    Terminated,
    /// Sandbox encountered an error
    Error(String),
}

/// A sandboxed execution environment for a bot
pub struct Sandbox {
    /// Unique identifier
    pub id: Uuid,
    /// Bot this sandbox belongs to
    pub bot_id: Uuid,
    /// Configuration
    pub config: SandboxConfig,
    /// Current status
    status: Arc<RwLock<SandboxStatus>>,
    /// Resource usage tracking
    usage: Arc<RwLock<ResourceUsage>>,
    /// Network connections
    connections: Arc<RwLock<Vec<NetworkConnection>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NetworkConnection {
    id: Uuid,
    url: String,
    established_at: chrono::DateTime<chrono::Utc>,
}

use crate::resource_limits::ResourceUsage;

impl Sandbox {
    /// Create a new sandbox for a bot
    pub fn new(bot_id: Uuid, config: SandboxConfig) -> Self {
        Self {
            id: Uuid::new_v4(),
            bot_id,
            config,
            status: Arc::new(RwLock::new(SandboxStatus::Created)),
            usage: Arc::new(RwLock::new(ResourceUsage::default())),
            connections: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Start the sandbox
    pub async fn start(&self) -> Result<(), String> {
        let mut status = self.status.write().await;
        *status = SandboxStatus::Running;
        tracing::info!(sandbox_id = %self.id, "Sandbox started");
        Ok(())
    }

    /// Stop the sandbox
    pub async fn stop(&self) -> Result<(), String> {
        let mut status = self.status.write().await;
        *status = SandboxStatus::Terminated;
        tracing::info!(sandbox_id = %self.id, "Sandbox stopped");
        Ok(())
    }

    /// Pause the sandbox
    pub async fn pause(&self) -> Result<(), String> {
        let mut status = self.status.write().await;
        *status = SandboxStatus::Paused;
        Ok(())
    }

    /// Resume the sandbox
    pub async fn resume(&self) -> Result<(), String> {
        let mut status = self.status.write().await;
        *status = SandboxStatus::Running;
        Ok(())
    }

    /// Get current status
    pub async fn status(&self) -> SandboxStatus {
        self.status.read().await.clone()
    }

    /// Get resource usage
    pub async fn usage(&self) -> ResourceUsage {
        self.usage.read().await.clone()
    }

    /// Check if resource limits are exceeded
    pub async fn is_over_limit(&self) -> bool {
        let usage = self.usage.read().await;
        let limits = &self.config.resource_limits;
        
        usage.cpu_percent > limits.max_cpu_percent
            || usage.memory_mb > limits.max_memory_mb
            || usage.disk_read_mb > limits.max_disk_read_mb
            || usage.disk_write_mb > limits.max_disk_write_mb
    }

    /// Record network connection attempt
    pub async fn check_network(&self, url: &str) -> bool {
        if !self.config.network_policy.is_allowed(url) {
            tracing::warn!(
                sandbox_id = %self.id,
                url = url,
                "Network access denied by policy"
            );
            return false;
        }

        let mut connections = self.connections.write().await;
        connections.push(NetworkConnection {
            id: Uuid::new_v4(),
            url: url.to_string(),
            established_at: chrono::Utc::now(),
        });

        true
    }

    /// Check if a file path is allowed
    pub fn check_file_access(&self, path: &str, _write: bool) -> bool {
        // Check blocked paths first
        for blocked in &self.config.blocked_paths {
            if path.starts_with(blocked) {
                return false;
            }
        }

        // Check allowed paths
        for allowed in &self.config.allowed_paths {
            if path.starts_with(allowed) {
                return true;
            }
        }

        // Default: deny access
        false
    }
}

/// Manages all active sandboxes
pub struct SandboxManager {
    sandboxes: HashMap<Uuid, Arc<Sandbox>>,
}

impl SandboxManager {
    pub fn new() -> Self {
        Self {
            sandboxes: HashMap::new(),
        }
    }

    /// Create a new sandbox for a bot
    pub async fn create(&mut self, bot_id: Uuid, config: SandboxConfig) -> Arc<Sandbox> {
        let sandbox = Arc::new(Sandbox::new(bot_id, config));
        self.sandboxes.insert(sandbox.id, sandbox.clone());
        sandbox
    }

    /// Get a sandbox by ID
    pub fn get(&self, id: &Uuid) -> Option<Arc<Sandbox>> {
        self.sandboxes.get(id).cloned()
    }

    /// Get sandbox for a bot
    pub fn get_for_bot(&self, bot_id: &Uuid) -> Option<Arc<Sandbox>> {
        self.sandboxes.values().find(|s| s.bot_id == *bot_id).cloned()
    }

    /// Stop all sandboxes (kill switch)
    pub async fn stop_all(&self) {
        for sandbox in self.sandboxes.values() {
            let _ = sandbox.stop().await;
        }
        tracing::warn!("All sandboxes stopped (kill switch activated)");
    }
}

impl Default for SandboxManager {
    fn default() -> Self {
        Self::new()
    }
}
