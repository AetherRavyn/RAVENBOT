use crate::exec;
use crate::traits::{Skill, SkillContext, SkillError, SkillResult, SkillRisk};
use async_trait::async_trait;
use ravenbot_core::Permission;

pub struct FileTreeSkill;

impl FileTreeSkill { pub fn new() -> Self { Self } }

#[async_trait]
impl Skill for FileTreeSkill {
    fn id(&self) -> &str { "file_tree" }
    fn name(&self) -> &str { "File Tree" }
    fn description(&self) -> &str { "List directory tree, files and sizes — offline workspace explorer" }
    fn version(&self) -> &str { "1.1.0" }
    fn required_permissions(&self) -> Vec<Permission> { vec![Permission::FileSystem { paths: vec![".".into()] }] }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({"type":"object","properties":{"path":{"type":"string","description":"Root path inside the office, default ."},"depth":{"type":"integer","minimum":1,"maximum":6}},"required":[]})
    }
    fn risk(&self) -> SkillRisk { SkillRisk::ReadOnly }

    /// `find` is given an argv, and the predicate is grouped.
    ///
    /// The previous command was `find <path> -maxdepth N -type f -o -type d`.
    /// `-o` has the lowest precedence, so that reads as
    /// `(-maxdepth N -type f) -o (-type d)` — the depth limit applied only to
    /// the `-type f` branch, and *every* directory at any depth came back.
    /// Writing `\\( -type f -o -type d \\)` is not possible without a shell, so
    /// the parentheses are dropped entirely and the run is capped in Rust.
    ///
    /// `find` defaults to `-P`, so a symlink in the office is listed but not
    /// walked into.
    async fn execute(&self, ctx: &SkillContext, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let raw = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let root = ctx.resolve_path(raw)?;
        let path = root.to_string_lossy().to_string();
        let depth = args
            .get("depth")
            .and_then(|v| v.as_u64())
            .unwrap_or(2)
            .clamp(1, 6)
            .to_string();

        let out = exec::run(
            ctx,
            "find",
            &[&path, "-maxdepth", &depth, "-mindepth", "1", "-printf", "%y %s %p\n"],
            None,
        )
        .await?;
        let stdout = String::from_utf8_lossy(&out.stdout);

        let mut entries: Vec<serde_json::Value> = Vec::new();
        for line in stdout.lines().take(400) {
            let mut parts = line.splitn(3, ' ');
            let kind = parts.next().unwrap_or("");
            let size = parts.next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
            let full = parts.next().unwrap_or("").to_string();
            entries.push(serde_json::json!({
                "type": match kind { "d" => "dir", "l" => "symlink", _ => "file" },
                "bytes": size,
                // Relative to the searched root, so the path means the same
                // thing in the office as it does to the agent.
                "path": full.strip_prefix(&format!("{path}/")).unwrap_or(&full).to_string(),
            }));
        }
        let dirs = entries.iter().filter(|e| e["type"] == "dir").count();
        let files = entries.len() - dirs;

        Ok(SkillResult::success(serde_json::json!({
            "path": path,
            "depth": depth.parse::<u64>().unwrap_or(2),
            "entries": entries,
            "count": entries.len(),
            "dirs": dirs,
            "files": files,
        })))
    }
}

impl Default for FileTreeSkill { fn default() -> Self { Self::new() } }
