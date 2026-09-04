//! RAVENBOT MCP — every MCP server as native Skill
pub mod client;
pub mod registry;
pub mod server;
pub mod servers;
pub mod store;

pub use client::{McpClient, McpTool};
pub use registry::McpRegistry;
pub use servers::{McpServerConfig, McpServerSummary, McpTestResult, all_servers, category_of};
