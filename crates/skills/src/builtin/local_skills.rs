//! Agent tools for the versioned local skill library.
//!
//! The library itself lives in [`crate::local`]. These wrappers exist so an
//! agent can author and test a skill without the desktop UI: create publishes
//! a first version, revise publishes a patch against a version it read, and
//! install makes an exact version active.
//!
//! Registration is data-only. Nothing here runs a script or sources a file.

use async_trait::async_trait;
use ravenbot_core::Permission;
use serde_json::Value;

use crate::local::{ImportSkillInput, LocalSkillLibrary, SkillFileInput};
use crate::traits::{Skill, SkillContext, SkillError, SkillResult, SkillRisk};

const FILE_SCHEMA: &str = "Supporting files, relative to the skill folder. Each item is {path, content}. Treated as data; never executed.";

fn files(arguments: &Value) -> Result<Vec<SkillFileInput>, String> {
    let Some(entries) = arguments.get("files") else {
        return Ok(Vec::new());
    };
    let Some(entries) = entries.as_array() else {
        return Err("'files' must be an array of {path, content} objects.".to_string());
    };
    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        let path = entry
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| "Each supporting file needs a 'path' string.".to_string())?;
        let content = entry
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| "Each supporting file needs a 'content' string.".to_string())?;
        out.push(SkillFileInput {
            path: path.to_string(),
            content: content.to_string(),
        });
    }
    Ok(out)
}

fn note(arguments: &Value) -> Option<String> {
    arguments
        .get("note")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Add a frontmatter block when the agent supplied only a body. A document
/// that already starts with frontmatter is left untouched: the agent is the
/// author, and rewriting its metadata would make preview and runtime disagree.
fn with_frontmatter(
    content: &str,
    name: Option<&str>,
    description: Option<&str>,
    example_prompt: Option<&str>,
) -> String {
    if content.trim_start().starts_with("---\n") {
        return content.to_string();
    }
    let mut map = serde_yaml::Mapping::new();
    if let Some(name) = name.filter(|value| !value.trim().is_empty()) {
        map.insert(
            serde_yaml::Value::String("name".into()),
            serde_yaml::Value::String(name.into()),
        );
    }
    if let Some(description) = description.filter(|value| !value.trim().is_empty()) {
        map.insert(
            serde_yaml::Value::String("description".into()),
            serde_yaml::Value::String(description.into()),
        );
    }
    if let Some(example) = example_prompt.filter(|value| !value.trim().is_empty()) {
        map.insert(
            serde_yaml::Value::String("example-prompt".into()),
            serde_yaml::Value::String(example.into()),
        );
    }
    if map.is_empty() {
        return content.to_string();
    }
    match serde_yaml::to_string(&map) {
        Ok(header) => format!(
            "---\n{}---\n\n{}",
            header.trim_start_matches("---\n"),
            content
        ),
        Err(_) => content.to_string(),
    }
}

// ── create ────────────────────────────────────────────────────────────────

pub struct CreateSkillSkill;

impl CreateSkillSkill {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CreateSkillSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Skill for CreateSkillSkill {
    fn id(&self) -> &str {
        "create_skill"
    }

    fn name(&self) -> &str {
        "Create local skill"
    }

    fn description(&self) -> &str {
        "Publish a new versioned SKILL.md in ~/RAVENBOT/skills. The skill is data: create it, then install an exact version before agents can read it. Include a frontmatter block with name, description, and example-prompt, or provide those fields separately."
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::FileSystem {
            paths: vec!["~/RAVENBOT/skills".to_string()],
        }]
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": {"type": "string", "description": "Filesystem-safe skill id (letters, digits, '-', '_', '.')."},
                "content": {"type": "string", "description": "The SKILL.md body; a YAML frontmatter block is optional."},
                "name": {"type": "string", "description": "Display name when the document has no frontmatter."},
                "description": {"type": "string", "description": "One-line description when the document has no frontmatter."},
                "example_prompt": {"type": "string", "description": "A prompt that demonstrates the skill."},
                "note": {"type": "string", "description": "Why this version exists (kept in the version history)."},
                "files": {
                    "type": "array",
                    "description": FILE_SCHEMA,
                    "items": {
                        "type": "object",
                        "properties": {
                            "path": {"type": "string"},
                            "content": {"type": "string"}
                        },
                        "required": ["path", "content"]
                    }
                }
            },
            "required": ["id", "content"]
        })
    }

    fn risk(&self) -> SkillRisk {
        SkillRisk::Low
    }

    async fn execute(
        &self,
        _context: &SkillContext,
        arguments: Value,
    ) -> Result<SkillResult, SkillError> {
        let id = arguments
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'id' field".to_string()))?;
        let content = arguments
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'content' field".to_string()))?;
        let content = with_frontmatter(
            content,
            arguments.get("name").and_then(Value::as_str),
            arguments.get("description").and_then(Value::as_str),
            arguments.get("example_prompt").and_then(Value::as_str),
        );
        let files = files(&arguments).map_err(SkillError::InvalidArguments)?;
        let library = LocalSkillLibrary::default();
        let view = library
            .create(id, &content, &files, note(&arguments))
            .map_err(SkillError::Execution)?;
        Ok(SkillResult::success(serde_json::json!({
            "skill": view,
            "installed": false,
            "next": format!("Install it with install_local_skill id='{}' version='{}'.", view.id, view.version)
        })))
    }
}

// ── revise ────────────────────────────────────────────────────────────────

pub struct ReviseSkillSkill;

impl ReviseSkillSkill {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ReviseSkillSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Skill for ReviseSkillSkill {
    fn id(&self) -> &str {
        "revise_skill"
    }

    fn name(&self) -> &str {
        "Revise local skill"
    }

    fn description(&self) -> &str {
        "Publish a new patch version of a local skill. `base_version` must be the version you read; a stale base is refused instead of overwriting a newer revision. Revising does not change the installed copy."
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::FileSystem {
            paths: vec!["~/RAVENBOT/skills".to_string()],
        }]
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "base_version": {"type": "string", "description": "The version this edit is based on."},
                "content": {"type": "string"},
                "note": {"type": "string"},
                "files": {
                    "type": "array",
                    "description": FILE_SCHEMA,
                    "items": {
                        "type": "object",
                        "properties": {
                            "path": {"type": "string"},
                            "content": {"type": "string"}
                        },
                        "required": ["path", "content"]
                    }
                }
            },
            "required": ["id", "base_version", "content"]
        })
    }

    fn risk(&self) -> SkillRisk {
        SkillRisk::Low
    }

    async fn execute(
        &self,
        _context: &SkillContext,
        arguments: Value,
    ) -> Result<SkillResult, SkillError> {
        let id = arguments
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'id' field".to_string()))?;
        let base = arguments
            .get("base_version")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                SkillError::InvalidArguments("Missing 'base_version' field".to_string())
            })?;
        let content = arguments
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'content' field".to_string()))?;
        let content = with_frontmatter(
            content,
            arguments.get("name").and_then(Value::as_str),
            arguments.get("description").and_then(Value::as_str),
            arguments.get("example_prompt").and_then(Value::as_str),
        );
        let files = files(&arguments).map_err(SkillError::InvalidArguments)?;
        let library = LocalSkillLibrary::default();
        let view = library
            .revise(id, base, &content, &files, note(&arguments))
            .map_err(SkillError::Execution)?;
        Ok(SkillResult::success(serde_json::json!({
            "skill": view,
            "installed": false,
            "next": format!("Install it with install_local_skill id='{}' version='{}'.", view.id, view.version)
        })))
    }
}

// ── list ──────────────────────────────────────────────────────────────────

pub struct ListLocalSkillsSkill;

impl ListLocalSkillsSkill {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ListLocalSkillsSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Skill for ListLocalSkillsSkill {
    fn id(&self) -> &str {
        "list_local_skills"
    }

    fn name(&self) -> &str {
        "List local skills"
    }

    fn description(&self) -> &str {
        "List versioned local skills in ~/RAVENBOT/skills with their latest and installed versions."
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn required_permissions(&self) -> Vec<Permission> {
        vec![]
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({"type": "object", "properties": {}})
    }

    fn risk(&self) -> SkillRisk {
        SkillRisk::ReadOnly
    }

    async fn execute(
        &self,
        _context: &SkillContext,
        _arguments: Value,
    ) -> Result<SkillResult, SkillError> {
        let library = LocalSkillLibrary::default();
        let skills = library.list().map_err(SkillError::Execution)?;
        Ok(SkillResult::success(serde_json::json!({"skills": skills})))
    }
}

// ── read ──────────────────────────────────────────────────────────────────

pub struct ReadLocalSkillSkill;

impl ReadLocalSkillSkill {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ReadLocalSkillSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Skill for ReadLocalSkillSkill {
    fn id(&self) -> &str {
        "read_local_skill"
    }

    fn name(&self) -> &str {
        "Read local skill"
    }

    fn description(&self) -> &str {
        "Read one local skill. With no version the installed copy is returned; with a version, that immutable library snapshot is returned."
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn required_permissions(&self) -> Vec<Permission> {
        vec![]
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "version": {"type": "string", "description": "Optional exact version; omit for the installed copy."}
            },
            "required": ["id"]
        })
    }

    fn risk(&self) -> SkillRisk {
        SkillRisk::ReadOnly
    }

    async fn execute(
        &self,
        _context: &SkillContext,
        arguments: Value,
    ) -> Result<SkillResult, SkillError> {
        let id = arguments
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'id' field".to_string()))?;
        let version = arguments.get("version").and_then(Value::as_str);
        let library = LocalSkillLibrary::default();
        let view = library.read(id, version).map_err(SkillError::Execution)?;
        Ok(SkillResult::success(serde_json::to_value(view).map_err(
            |error| SkillError::Execution(error.to_string()),
        )?))
    }
}

// ── install ───────────────────────────────────────────────────────────────

pub struct InstallLocalSkillSkill;

impl InstallLocalSkillSkill {
    pub fn new() -> Self {
        Self
    }
}

impl Default for InstallLocalSkillSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Skill for InstallLocalSkillSkill {
    fn id(&self) -> &str {
        "install_local_skill"
    }

    fn name(&self) -> &str {
        "Install local skill"
    }

    fn description(&self) -> &str {
        "Install one exact version of a local skill as the active copy. A locally edited installed copy is refused unless force=true, which preserves it under backups/ first."
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::FileSystem {
            paths: vec!["~/RAVENBOT/skills".to_string()],
        }]
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "version": {"type": "string"},
                "force": {"type": "boolean", "description": "Back up and replace local edits (default false)."}
            },
            "required": ["id", "version"]
        })
    }

    fn risk(&self) -> SkillRisk {
        SkillRisk::Low
    }

    async fn execute(
        &self,
        _context: &SkillContext,
        arguments: Value,
    ) -> Result<SkillResult, SkillError> {
        let id = arguments
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'id' field".to_string()))?;
        let version = arguments
            .get("version")
            .and_then(Value::as_str)
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'version' field".to_string()))?;
        let force = arguments
            .get("force")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let library = LocalSkillLibrary::default();
        let view = library
            .install(id, version, force)
            .map_err(SkillError::Execution)?;
        Ok(SkillResult::success(serde_json::json!({
            "skill": view,
            "installed": true
        })))
    }
}

/// Import a document from the live catalog. Used by the settings command, not
/// registered as an agent tool: the UI is where a user grants that permission.
pub fn import_catalog_document(
    id: &str,
    content: &str,
    source: &str,
    update: bool,
) -> Result<crate::local::SkillView, String> {
    LocalSkillLibrary::default().import_document(ImportSkillInput {
        id: id.to_string(),
        name: None,
        description: None,
        content: content.to_string(),
        source: source.to_string(),
        update,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::LocalSkillLibrary;
    use uuid::Uuid;

    fn context() -> SkillContext {
        SkillContext::with_default_tier(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4())
    }

    fn with_library<T>(body: impl FnOnce() -> T) -> T {
        // The library root is process-global through the environment, so this
        // test module serializes itself.
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let root = std::env::temp_dir().join(format!("rb-local-tools-{}", Uuid::new_v4()));
        std::env::set_var("RAVENBOT_SKILLS_DIR", &root);
        let result = body();
        std::env::remove_var("RAVENBOT_SKILLS_DIR");
        let _ = std::fs::remove_dir_all(&root);
        result
    }

    #[test]
    fn create_revise_install_round_trip_through_the_tools() {
        with_library(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                let created = CreateSkillSkill::new()
                    .execute(
                        &context(),
                        serde_json::json!({
                            "id": "demo",
                            "name": "Demo",
                            "description": "A demo",
                            "content": "Do the thing.",
                            "note": "first"
                        }),
                    )
                    .await
                    .expect("create");
                assert!(created.success);
                let created = created.output.get("skill").cloned().unwrap();
                assert_eq!(
                    created.get("version").and_then(Value::as_str),
                    Some("0.1.0")
                );

                let revised = ReviseSkillSkill::new()
                    .execute(
                        &context(),
                        serde_json::json!({
                            "id": "demo",
                            "base_version": "0.1.0",
                            "content": "Do the better thing."
                        }),
                    )
                    .await
                    .expect("revise");
                assert!(revised.success);

                let installed = InstallLocalSkillSkill::new()
                    .execute(
                        &context(),
                        serde_json::json!({"id": "demo", "version": "0.1.1"}),
                    )
                    .await
                    .expect("install");
                assert!(installed.success);

                let read = ReadLocalSkillSkill::new()
                    .execute(&context(), serde_json::json!({"id": "demo"}))
                    .await
                    .expect("read");
                assert!(read.success);
                assert_eq!(
                    read.output.get("version").and_then(Value::as_str),
                    Some("0.1.1")
                );
                assert!(read
                    .output
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .contains("Do the better thing."));

                let listed = ListLocalSkillsSkill::new()
                    .execute(&context(), serde_json::json!({}))
                    .await
                    .expect("list");
                let skills = listed
                    .output
                    .get("skills")
                    .and_then(Value::as_array)
                    .unwrap();
                assert_eq!(skills.len(), 1);
            });
        });
    }

    #[test]
    fn a_document_without_frontmatter_gets_one() {
        let document = with_frontmatter("Body", Some("Name"), Some("Desc"), Some("Try me"));
        assert!(document.starts_with("---\n"));
        assert!(document.contains("name: Name"));
        assert!(document.contains("example-prompt: Try me"));
        let library = LocalSkillLibrary::at(
            std::env::temp_dir().join(format!("rb-frontmatter-{}", Uuid::new_v4())),
        );
        let view = library.create("demo", &document, &[], None).unwrap();
        assert_eq!(view.name, "Name");
        assert_eq!(view.example_prompt.as_deref(), Some("Try me"));
    }
}
