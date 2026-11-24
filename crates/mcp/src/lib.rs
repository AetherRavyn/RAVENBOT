//! RAVENBOT MCP — every MCP server as native Skill
pub mod client;
pub mod events;
pub mod registry;
pub mod server;
pub mod servers;
pub mod store;
pub mod team;

pub use client::{McpClient, McpTool};
pub use events::{notification_matches, parse_notification, McpNotification};
pub use registry::McpRegistry;
pub use servers::{all_servers, category_of, McpServerConfig, McpServerSummary, McpTestResult};
