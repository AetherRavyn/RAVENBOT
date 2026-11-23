//! `file_edit` — apply Codex-style patches or exact replacements.
//!
//! Models trained in other coding environments habitually call `file_edit`
//! with `*** Begin Patch` / `*** Update File` input. RAVENBOT only had
//! `code_edit`, which takes a unified diff, so those calls failed with
//! "Skill not found". This skill accepts the format the model already emits,
//! applies every hunk in memory, and writes only when the whole patch matches.
//! A malformed or truncated patch changes nothing and returns a precise error.

use crate::traits::{Skill, SkillContext, SkillError, SkillResult};
use async_trait::async_trait;
use ravenbot_core::Permission;
use std::path::{Path, PathBuf};

pub struct FileEditSkill;

impl FileEditSkill {
    pub fn new() -> Self {
        Self
    }
}

#[derive(Debug)]
struct FilePatch {
    path: String,
    add: bool,
    delete: bool,
    hunks: Vec<Hunk>,
    added_content: Vec<String>,
    raw: String,
}

#[derive(Debug)]
struct Hunk {
    old: Vec<String>,
    new: Vec<String>,
    added: usize,
    deleted: usize,
}

impl Hunk {
    fn new() -> Self {
        Self {
            old: Vec::new(),
            new: Vec::new(),
            added: 0,
            deleted: 0,
        }
    }
}

#[async_trait]
impl Skill for FileEditSkill {
    fn id(&self) -> &str {
        "file_edit"
    }
    fn name(&self) -> &str {
        "Edit File"
    }
    fn description(&self) -> &str {
        "Edit one or more files with a Codex-style apply_patch (*** Begin Patch / *** Update File) or exact old/new replacements. Applies atomically: a patch that does not match changes nothing. Use code_edit for a standard unified diff."
    }
    fn version(&self) -> &str {
        "1.0.0"
    }
    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::FileSystem {
            paths: vec![".".into()],
        }]
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "patch": {
                    "type": "string",
                    "description": "Codex apply_patch text: *** Begin Patch, then *** Update File/*** Add File/*** Delete File, hunks starting with @@, and *** End Patch. Context lines start with a space, removed with -, added with +."
                },
                "path": {
                    "type": "string",
                    "description": "Base directory for relative patch paths, or the target file itself. A file path is treated as the target."
                },
                "edits": {
                    "type": "array",
                    "description": "Exact replacements, an alternative to patch",
                    "items": {
                        "type": "object",
                        "properties": {
                            "path": {"type": "string"},
                            "oldText": {"type": "string"},
                            "newText": {"type": "string"},
                            "replaceAll": {"type": "boolean"}
                        },
                        "required": ["path", "oldText", "newText"]
                    }
                }
            },
            "anyOf": [{"required": ["patch"]}, {"required": ["edits"]}]
        })
    }

    async fn execute(
        &self,
        ctx: &SkillContext,
        args: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        // `path` is documented as a directory, but models often send the
        // target file instead (Shared/STATUS.md). A file is not a base; use
        // the workspace root so the patch's own relative path resolves. A real
        // directory remains the base, which supports patches whose file is a
        // bare name inside that directory.
        let mut default_file: Option<PathBuf> = None;
        let base = match args
            .get("path")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(raw) => {
                let resolved = ctx.resolve_path(raw)?;
                if resolved.is_dir() {
                    resolved
                } else {
                    default_file = Some(resolved);
                    ctx.primary_dir()
                        .map(std::path::Path::to_path_buf)
                        .unwrap_or_else(|| PathBuf::from("."))
                }
            }
            None => ctx
                .primary_dir()
                .map(std::path::Path::to_path_buf)
                .or_else(|| std::env::current_dir().ok())
                .ok_or_else(|| SkillError::Execution("No working directory.".into()))?,
        };
        if let Some(patch) = args.get("patch").and_then(|v| v.as_str()) {
            return apply_codex_patch(ctx, &base, patch).await;
        }
        if let Some(edits) = args.get("edits").and_then(|v| v.as_array()) {
            return apply_exact_edits(ctx, &base, default_file.as_deref(), edits).await;
        }
        Err(SkillError::InvalidArguments(
            "Provide 'patch' or 'edits'".into(),
        ))
    }
}

async fn apply_codex_patch(
    ctx: &SkillContext,
    base: &Path,
    patch: &str,
) -> Result<SkillResult, SkillError> {
    let files = match parse_codex_patch(patch) {
        Ok(files) => files,
        Err(error) => return Ok(SkillResult::failure(error)),
    };
    if files.is_empty() {
        return Ok(SkillResult::failure(
            "The patch contains no file operations. Use *** Update File, *** Add File, or *** Delete File.",
        ));
    }

    // Apply every operation in memory first. A failure anywhere leaves the
    // workspace byte-identical to before the call.
    let mut writes: Vec<(PathBuf, String)> = Vec::new();
    let mut deletes: Vec<PathBuf> = Vec::new();
    let mut changes = Vec::new();
    for file in &files {
        let full = resolve(ctx, base, &file.path)?;
        if file.delete {
            if full.exists() {
                deletes.push(full);
            }
            changes.push(serde_json::json!({
                "path": file.path,
                "lines_added": 0,
                "lines_deleted": 0,
                "diff": file.raw,
            }));
            continue;
        }
        if file.add {
            if full.exists() {
                return Ok(SkillResult::failure(format!(
                    "*** Add File target already exists: {}",
                    file.path
                )));
            }
            let mut content = file.added_content.join("\n");
            if !content.is_empty() {
                content.push('\n');
            }
            writes.push((full, content));
            changes.push(serde_json::json!({
                "path": file.path,
                "lines_added": file.added_content.len(),
                "lines_deleted": 0,
                "diff": file.raw,
            }));
            continue;
        }
        if file.hunks.is_empty() {
            return Ok(SkillResult::failure(format!(
                "*** Update File {} has no @@ hunks.",
                file.path
            )));
        }
        let existing = tokio::fs::read_to_string(&full)
            .await
            .map_err(|error| SkillError::Io(format!("Could not read {}: {error}", file.path)))?;
        let trailing_newline = existing.ends_with('\n');
        let lines: Vec<String> = existing.lines().map(str::to_string).collect();
        let new_lines = match apply_hunks(&lines, &file.hunks) {
            Ok(lines) => lines,
            Err(error) => {
                return Ok(SkillResult::failure(format!(
                    "Patch for {} failed: {error}",
                    file.path
                )))
            }
        };
        let mut content = new_lines.join("\n");
        if trailing_newline || !new_lines.is_empty() {
            content.push('\n');
        }
        let added: usize = file.hunks.iter().map(|hunk| hunk.added).sum();
        let deleted: usize = file.hunks.iter().map(|hunk| hunk.deleted).sum();
        writes.push((full, content));
        changes.push(serde_json::json!({
            "path": file.path,
            "lines_added": added,
            "lines_deleted": deleted,
            "diff": file.raw,
        }));
    }

    for (path, content) in &writes {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|error| SkillError::Io(error.to_string()))?;
        }
        tokio::fs::write(path, content).await.map_err(|error| {
            SkillError::Io(format!("Could not write {}: {error}", path.display()))
        })?;
    }
    for path in &deletes {
        tokio::fs::remove_file(path)
            .await
            .map_err(|error| SkillError::Io(error.to_string()))?;
    }

    Ok(SkillResult::success(serde_json::json!({
        "applied": true,
        "files": changes,
        "file_changes": changes,
    })))
}

async fn apply_exact_edits(
    ctx: &SkillContext,
    base: &Path,
    default_file: Option<&Path>,
    edits: &[serde_json::Value],
) -> Result<SkillResult, SkillError> {
    if edits.is_empty() {
        return Err(SkillError::InvalidArguments("'edits' is empty".into()));
    }
    let mut writes = Vec::new();
    let mut changes = Vec::new();
    for edit in edits {
        let fallback = default_file.map(|path| path.to_string_lossy().to_string());
        let path = edit
            .get("path")
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .or(fallback)
            .ok_or_else(|| SkillError::InvalidArguments("An edit is missing 'path'".into()))?;
        let old = edit
            .get("oldText")
            .or_else(|| edit.get("old_string"))
            .or_else(|| edit.get("old"))
            .and_then(|value| value.as_str())
            .ok_or_else(|| {
                SkillError::InvalidArguments(format!("Edit for {path} is missing 'oldText'"))
            })?;
        let new = edit
            .get("newText")
            .or_else(|| edit.get("new_string"))
            .or_else(|| edit.get("new"))
            .and_then(|value| value.as_str())
            .ok_or_else(|| {
                SkillError::InvalidArguments(format!("Edit for {path} is missing 'newText'"))
            })?;
        if old.is_empty() {
            return Ok(SkillResult::failure(
                "An exact replacement needs a non-empty oldText. Use *** Add File for a new file.",
            ));
        }
        let full = resolve(ctx, base, &path)?;
        let existing = tokio::fs::read_to_string(&full)
            .await
            .map_err(|error| SkillError::Io(format!("Could not read {path}: {error}")))?;
        let count = existing.matches(old).count();
        if count == 0 {
            return Ok(SkillResult::failure(format!(
                "Edit for {path} did not match: oldText was not found. Nothing was changed."
            )));
        }
        let replace_all = edit
            .get("replaceAll")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        if count > 1 && !replace_all {
            return Ok(SkillResult::failure(format!(
                "Edit for {path} matched {count} places. Include more surrounding text or set replaceAll=true. Nothing was changed."
            )));
        }
        let updated = if replace_all {
            existing.replace(old, new)
        } else {
            existing.replacen(old, new, 1)
        };
        writes.push((full, updated));
        changes.push(serde_json::json!({
            "path": path,
            "lines_added": new.lines().count(),
            "lines_deleted": old.lines().count(),
        }));
    }
    for (path, content) in writes {
        tokio::fs::write(&path, content).await.map_err(|error| {
            SkillError::Io(format!("Could not write {}: {error}", path.display()))
        })?;
    }
    Ok(SkillResult::success(serde_json::json!({
        "applied": true,
        "files": changes,
        "file_changes": changes,
    })))
}

fn resolve(ctx: &SkillContext, base: &Path, raw: &str) -> Result<PathBuf, SkillError> {
    let path = Path::new(raw);
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    ctx.resolve_path(&candidate.to_string_lossy())
}

fn apply_hunks(lines: &[String], hunks: &[Hunk]) -> Result<Vec<String>, String> {
    let mut current = lines.to_vec();
    for (index, hunk) in hunks.iter().enumerate() {
        if hunk.old.is_empty() {
            return Err(format!(
                "hunk {} has no context or removed lines; add a context line before the change",
                index + 1
            ));
        }
        let Some(position) = find_subsequence(&current, &hunk.old) else {
            return Err(format!(
                "hunk {} did not match the file. Read the current file and include the exact surrounding lines.",
                index + 1
            ));
        };
        current.splice(
            position..position + hunk.old.len(),
            hunk.new.iter().cloned(),
        );
    }
    Ok(current)
}

fn find_subsequence(haystack: &[String], needle: &[String]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn parse_codex_patch(input: &str) -> Result<Vec<FilePatch>, String> {
    let mut lines = input.lines();
    let first = lines.next().unwrap_or_default().trim();
    if first != "*** Begin Patch" {
        return Err(
            "Expected a Codex patch starting with '*** Begin Patch'. For a standard unified diff use code_edit."
                .to_string(),
        );
    }

    let mut files: Vec<FilePatch> = Vec::new();
    let mut current: Option<FilePatch> = None;
    let mut hunk: Option<Hunk> = None;
    let mut raw_lines: Vec<String> = vec![first.to_string()];
    let mut saw_end = false;

    for line in lines {
        raw_lines.push(line.to_string());
        if line == "*** End Patch" {
            flush_hunk(&mut current, &mut hunk);
            flush_file(&mut files, &mut current);
            saw_end = true;
            break;
        }
        if let Some(path) = line.strip_prefix("*** Update File: ") {
            flush_hunk(&mut current, &mut hunk);
            flush_file(&mut files, &mut current);
            current = Some(FilePatch {
                path: path.trim().to_string(),
                add: false,
                delete: false,
                hunks: Vec::new(),
                added_content: Vec::new(),
                raw: String::new(),
            });
            continue;
        }
        if let Some(path) = line.strip_prefix("*** Add File: ") {
            flush_hunk(&mut current, &mut hunk);
            flush_file(&mut files, &mut current);
            current = Some(FilePatch {
                path: path.trim().to_string(),
                add: true,
                delete: false,
                hunks: Vec::new(),
                added_content: Vec::new(),
                raw: String::new(),
            });
            continue;
        }
        if let Some(path) = line.strip_prefix("*** Delete File: ") {
            flush_hunk(&mut current, &mut hunk);
            flush_file(&mut files, &mut current);
            current = Some(FilePatch {
                path: path.trim().to_string(),
                add: false,
                delete: true,
                hunks: Vec::new(),
                added_content: Vec::new(),
                raw: String::new(),
            });
            continue;
        }
        if line.starts_with("*** End of File") {
            continue;
        }
        let Some(file) = current.as_mut() else {
            if line.trim().is_empty() {
                continue;
            }
            return Err(format!("Unexpected line before a file header: {line}"));
        };
        if file.add {
            if let Some(content) = line.strip_prefix('+') {
                file.added_content.push(content.to_string());
            } else if line.trim().is_empty() {
                file.added_content.push(String::new());
            } else {
                return Err(format!(
                    "Add File {} may only contain '+content' lines; found {line}",
                    file.path
                ));
            }
            continue;
        }
        if file.delete {
            if line.trim().is_empty() {
                continue;
            }
            return Err(format!(
                "Delete File {} must not contain content; found {line}",
                file.path
            ));
        }
        if line.starts_with("@@") {
            flush_hunk(&mut current, &mut hunk);
            hunk = Some(Hunk::new());
            continue;
        }
        let Some(hunk) = hunk.as_mut() else {
            if line.trim().is_empty() {
                continue;
            }
            return Err(format!(
                "Expected '@@' before hunk content for {}: {line}",
                file.path
            ));
        };
        if let Some(context) = line.strip_prefix(' ') {
            hunk.old.push(context.to_string());
            hunk.new.push(context.to_string());
        } else if let Some(removed) = line.strip_prefix('-') {
            hunk.old.push(removed.to_string());
            hunk.deleted += 1;
        } else if let Some(added) = line.strip_prefix('+') {
            hunk.new.push(added.to_string());
            hunk.added += 1;
        } else if line.is_empty() {
            hunk.old.push(String::new());
            hunk.new.push(String::new());
        } else {
            return Err(format!(
                "A hunk line must start with ' ', '-', or '+': {line}"
            ));
        }
    }

    if !saw_end {
        return Err(
            "The patch is incomplete: missing '*** End Patch'. Nothing was changed; send the complete patch."
                .to_string(),
        );
    }
    flush_hunk(&mut current, &mut hunk);
    flush_file(&mut files, &mut current);

    // Attach the raw text for the change ledger.
    let raw = raw_lines.join("\n");
    for file in &mut files {
        file.raw = raw.clone();
    }
    Ok(files)
}

fn flush_hunk(current: &mut Option<FilePatch>, hunk: &mut Option<Hunk>) {
    if let (Some(file), Some(hunk)) = (current.as_mut(), hunk.take()) {
        file.hunks.push(hunk);
    }
}

fn flush_file(files: &mut Vec<FilePatch>, current: &mut Option<FilePatch>) {
    if let Some(file) = current.take() {
        files.push(file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(dir: &Path) -> SkillContext {
        SkillContext::new(
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            ravenbot_core::SandboxTier::Host,
        )
        .with_working_dirs(vec![dir.to_path_buf()])
        .confined()
    }

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ravenbot-file-edit-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn a_codex_patch_updates_the_file_atomically() {
        let dir = temp_dir();
        std::fs::write(
            dir.join("app.py"),
            "def view():\n    state = OLD\n    return state\n",
        )
        .unwrap();
        let ctx = context(&dir);
        let result = FileEditSkill::new()
            .execute(
                &ctx,
                serde_json::json!({
                    "patch": "*** Begin Patch\n*** Update File: app.py\n@@\n def view():\n-    state = OLD\n+    state = NEW\n     return state\n*** End Patch"
                }),
            )
            .await
            .expect("execute");
        assert!(result.success, "{:?}", result.error);
        let body = std::fs::read_to_string(dir.join("app.py")).unwrap();
        assert!(body.contains("state = NEW"));
        assert!(!body.contains("state = OLD"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_file_path_as_the_base_is_treated_as_the_target() {
        let dir = temp_dir();
        std::fs::create_dir_all(dir.join("Shared")).unwrap();
        let path = dir.join("Shared/STATUS.md");
        std::fs::write(&path, "old status\n").unwrap();
        let ctx = context(&dir);
        let result = FileEditSkill::new()
            .execute(
                &ctx,
                serde_json::json!({
                    "path": "Shared/STATUS.md",
                    "patch": "*** Begin Patch\n*** Update File: Shared/STATUS.md\n@@\n-old status\n+new status\n*** End Patch"
                }),
            )
            .await
            .expect("execute");
        assert!(result.success, "{:?}", result.error);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new status\n");

        // An edit may omit its own path when the caller supplied the target
        // file as `path`.
        let exact = FileEditSkill::new()
            .execute(
                &ctx,
                serde_json::json!({
                    "path": "Shared/STATUS.md",
                    "edits": [{"oldText": "new status", "newText": "edited"}]
                }),
            )
            .await
            .expect("execute");
        assert!(exact.success, "{:?}", exact.error);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "edited\n");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_truncated_patch_changes_nothing() {
        let dir = temp_dir();
        let path = dir.join("app.py");
        std::fs::write(&path, "state = OLD\n").unwrap();
        let ctx = context(&dir);
        let result = FileEditSkill::new()
            .execute(
                &ctx,
                serde_json::json!({
                    "patch": "*** Begin Patch\n*** Update File: app.py\n@@\n-state = OLD\n+state = NEW"
                }),
            )
            .await
            .expect("execute");
        assert!(!result.success);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "state = OLD\n");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn exact_edits_replace_or_refuse_ambiguity() {
        let dir = temp_dir();
        let path = dir.join("app.py");
        std::fs::write(&path, "a = 1\na = 1\n").unwrap();
        let ctx = context(&dir);
        let ambiguous = FileEditSkill::new()
            .execute(
                &ctx,
                serde_json::json!({
                    "edits": [{"path": "app.py", "oldText": "a = 1", "newText": "a = 2"}]
                }),
            )
            .await
            .expect("execute");
        assert!(!ambiguous.success);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "a = 1\na = 1\n");

        let replaced = FileEditSkill::new()
            .execute(
                &ctx,
                serde_json::json!({
                    "edits": [{"path": "app.py", "oldText": "a = 1", "newText": "a = 2", "replaceAll": true}]
                }),
            )
            .await
            .expect("execute");
        assert!(replaced.success, "{:?}", replaced.error);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "a = 2\na = 2\n");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_path_outside_the_workspace_is_refused() {
        let dir = temp_dir();
        let ctx = context(&dir);
        let result = FileEditSkill::new()
            .execute(
                &ctx,
                serde_json::json!({
                    "patch": "*** Begin Patch\n*** Add File: ../escape.py\n+owned\n*** End Patch"
                }),
            )
            .await;
        assert!(result.is_err(), "{result:?}");
        assert!(!dir.parent().unwrap().join("escape.py").exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
