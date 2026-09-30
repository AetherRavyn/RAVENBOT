//! RAVENBOT Plugins — in-app via OpenAPI + Awesome → native Skill (no mocks, no external service)
pub mod openapi;
pub mod registry;
pub mod store;

pub use openapi::OpenApiSkill;
pub use registry::PluginRegistry;
