//! RAVENBOT sandbox isolation and resource management
//!
//! This crate provides per-bot sandboxed execution environments with
//! resource quotas and a global kill switch.

pub mod desktop;
pub mod kill_switch;
pub mod network_policy;
pub mod resource_limits;
pub mod runner;
pub mod sandbox;

pub use desktop::DesktopSession;
pub use kill_switch::{KillSwitch, KillSwitchState};
pub use network_policy::{NetworkPolicy, NetworkRule};
pub use resource_limits::{ResourceLimits, ResourceUsage};
pub use runner::{SandboxBackend, SandboxReport, SandboxRunner};
pub use sandbox::{Sandbox, SandboxConfig, SandboxStatus};
