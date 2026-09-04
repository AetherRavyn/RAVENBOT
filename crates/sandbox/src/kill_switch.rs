//! Global kill switch for all bots

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// State of the kill switch
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum KillSwitchState {
    /// Normal operation
    Active,
    /// Kill switch triggered - all bots paused
    Triggered,
}

/// Global kill switch that can pause all bots immediately
pub struct KillSwitch {
    state: Arc<RwLock<KillSwitchState>>,
    reason: Arc<RwLock<Option<String>>>,
    triggered_at: Arc<RwLock<Option<chrono::DateTime<chrono::Utc>>>>,
}

impl KillSwitch {
    /// Create a new kill switch in active state
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(KillSwitchState::Active)),
            reason: Arc::new(RwLock::new(None)),
            triggered_at: Arc::new(RwLock::new(None)),
        }
    }

    /// Trigger the kill switch
    pub async fn trigger(&self, reason: impl Into<String>) {
        let mut state = self.state.write().await;
        let mut r = self.reason.write().await;
        let mut t = self.triggered_at.write().await;
        
        *state = KillSwitchState::Triggered;
        *r = Some(reason.into());
        *t = Some(chrono::Utc::now());
        
        tracing::warn!("Kill switch triggered");
    }

    /// Release the kill switch
    pub async fn release(&self) {
        let mut state = self.state.write().await;
        let mut r = self.reason.write().await;
        let mut t = self.triggered_at.write().await;
        
        *state = KillSwitchState::Active;
        *r = None;
        *t = None;
        
        tracing::info!("Kill switch released");
    }

    /// Check if the kill switch is triggered
    pub async fn is_triggered(&self) -> bool {
        *self.state.read().await == KillSwitchState::Triggered
    }

    /// Get current state
    pub async fn state(&self) -> KillSwitchState {
        self.state.read().await.clone()
    }

    /// Get trigger reason
    pub async fn reason(&self) -> Option<String> {
        self.reason.read().await.clone()
    }

    /// Get when it was triggered
    pub async fn triggered_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        *self.triggered_at.read().await
    }

    /// Get current status info
    pub async fn status(&self) -> KillSwitchStatus {
        KillSwitchStatus {
            state: self.state.read().await.clone(),
            reason: self.reason.read().await.clone(),
            triggered_at: *self.triggered_at.read().await,
        }
    }
}

impl Default for KillSwitch {
    fn default() -> Self {
        Self::new()
    }
}

/// Status information about the kill switch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KillSwitchStatus {
    pub state: KillSwitchState,
    pub reason: Option<String>,
    pub triggered_at: Option<chrono::DateTime<chrono::Utc>>,
}
