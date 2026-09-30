//! RAVENBOT sandbox isolation and resource management
//!
//! This crate provides per-bot sandboxed execution environments with
//! resource quotas and a global kill switch.

pub mod sandbox;
pub mod resource_limits;
pub mod network_policy;
pub mod kill_switch;

pub use sandbox::{Sandbox, SandboxConfig, SandboxStatus};
pub use resource_limits::{ResourceLimits, ResourceUsage};
pub use network_policy::{NetworkPolicy, NetworkRule};
pub use kill_switch::{KillSwitch, KillSwitchState};
