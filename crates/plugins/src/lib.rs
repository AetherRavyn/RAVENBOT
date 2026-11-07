//! RAVENBOT Plugins — installed OpenAPI apps parsed into executable skills.
//!
//! The store keeps only real imported documents. A plugin with zero parsed
//! operations contributes no tools, and execution goes through the same
//! authenticated HTTP path for direct skills and the plugin meta tools.
pub mod catalog;
pub mod openapi;
pub mod registry;
pub mod store;

pub use catalog::CatalogService;
pub use openapi::OpenApiSkill;
pub use registry::PluginRegistry;
