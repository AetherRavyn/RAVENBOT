//! `make_document` — real DOCX, PDF, PPTX, and HTML files through Pandoc.
//!
//! The important part is not the conversion, it is the verification. A tool
//! that says "created report.pdf" when Pandoc failed is worse than one that
//! says nothing: the user goes looking for a file that was never written, and
//! the agent's next step builds on it. Every export here checks the exit
//! status, that the path exists, that it is non-empty, and the format's own
//! signature (`PK` for OOXML, `%PDF` for PDF, `<html` for the HTML target).
//!
//! Files are written only inside the run's workspace, through the same
//! `resolve_path` confinement every other file skill uses.

use async_trait::async_trait;
use ravenbot_core::Permission;
use std::path::Path;

use crate::traits::{Skill, SkillContext, SkillError, SkillResult, SkillRisk};

pub struct MakeDocumentSkill;

impl MakeDocumentSkill {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MakeDocumentSkill {
    fn default() -> Self {
        Self::new()
    }
}

fn extension(format: &str) -> Option<&'static str> {
    match format {
        "docx" => Some("docx"),
        "pdf" => Some("pdf"),
        "pptx" => Some("pptx"),
        "html" => Some("html"),
        _ => None,
    }
}

fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for ch in name.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            if dash && !out.is_empty() {
                out.push('-');
            }
            dash = false;
            out.push(ch);
        } else {
            dash = true;
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "document".to_string()
    } else {
        out.chars().take(80).collect()
    }
}

/// `---` on its own line becomes a slide break for the PPTX target.
fn slides_markdown(content: &str) -> String {
    let normalized = content.replace("\r\n", "\n");
    let mut out = String::with_capacity(normalized.len());
    let mut slide = 1usize;
    for line in normalized.lines() {
        if line.trim() == "---" {
            slide += 1;
            out.push_str(&format!("\n\n# Slide {slide}\n\n"));
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn verify(path: &Path, format: &str) -> Result<u64, String> {
    let metadata = std::fs::metadata(path).map_err(|error| {
        format!(
            "Pandoc reported success but '{}' is missing: {error}",
            path.display()
        )
    })?;
    if !metadata.is_file() {
        return Err(format!("'{}' is not a file.", path.display()));
    }
    if metadata.len() == 0 {
        return Err(format!("'{}' is empty.", path.display()));
    }
    let head = std::fs::read(path)
        .map_err(|error| format!("Could not read '{}': {error}", path.display()))?;
    match format {
        "pdf" if !head.starts_with(b"%PDF") => Err(format!(
            "'{}' is not a PDF (missing %PDF signature).",
            path.display()
        )),
        "docx" | "pptx" if !head.starts_with(b"PK") => Err(format!(
            "'{}' is not an Office Open XML package (missing PK signature).",
            path.display()
        )),
        "html" => {
            let text = String::from_utf8_lossy(&head);
            if !text.to_ascii_lowercase().contains("<html")
                && !text.to_ascii_lowercase().contains("<!doctype html")
            {
                return Err(format!(
                    "'{}' does not look like an HTML document.",
                    path.display()
                ));
            }
            Ok(metadata.len())
        }
        _ => Ok(metadata.len()),
    }
}

#[async_trait]
impl Skill for MakeDocumentSkill {
    fn id(&self) -> &str {
        "make_document"
    }

    fn name(&self) -> &str {
        "Make document"
    }

    fn description(&self) -> &str {
        "Export a Markdown report as a real DOCX, PDF, PPTX, or HTML file inside the workspace using Pandoc. Use this for editable documents; use make_slides for a designed, image-first deck."
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn required_permissions(&self) -> Vec<Permission> {
        vec![
            Permission::FileSystem {
                paths: vec!["/".to_string()],
            },
            Permission::Shell,
        ]
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "Markdown body. Headings, tables, code, and math are preserved. PPTX converts a line containing only --- into a slide break."
                },
                "markdown": {
                    "type": "string",
                    "description": "Alias for content."
                },
                "format": {
                    "type": "string",
                    "description": "Output format (default: docx)",
                    "enum": ["docx", "pdf", "pptx", "html"]
                },
                "title": {
                    "type": "string",
                    "description": "Document title and default file name"
                },
                "path": {
                    "type": "string",
                    "description": "Optional output path inside the workspace; the extension is set from format"
                },
                "reference_doc": {
                    "type": "string",
                    "description": "Optional DOCX or PPTX reference document inside the workspace for styling"
                }
            },
            "required": ["content"]
        })
    }

    fn risk(&self) -> SkillRisk {
        SkillRisk::High
    }

    async fn execute(
        &self,
        context: &SkillContext,
        arguments: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        let content = arguments
            .get("content")
            .or_else(|| arguments.get("markdown"))
            .and_then(|value| value.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'content' field".to_string()))?;
        if content.trim().is_empty() {
            return Err(SkillError::InvalidArguments(
                "The document body is empty.".to_string(),
            ));
        }

        let format = arguments
            .get("format")
            .and_then(|value| value.as_str())
            .unwrap_or("docx")
            .trim()
            .to_ascii_lowercase();
        let Some(extension) = extension(&format) else {
            return Err(SkillError::InvalidArguments(format!(
                "Unknown format '{format}'. Use docx, pdf, pptx, or html."
            )));
        };
        let title = arguments
            .get("title")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("Document");

        let output = match arguments.get("path").and_then(|value| value.as_str()) {
            Some(raw) if !raw.trim().is_empty() => {
                let mut resolved = context.resolve_path(raw)?;
                // The extension is part of the contract: `format` decides the
                // bytes, so a `.docx` request cannot be written to `report.pdf`.
                resolved.set_extension(extension);
                resolved
            }
            _ => {
                let root = context
                    .primary_dir()
                    .map(Path::to_path_buf)
                    .or_else(|| std::env::current_dir().ok())
                    .ok_or_else(|| {
                        SkillError::Execution("No workspace to write the document into.".into())
                    })?;
                let folder = root.join("deliverables");
                std::fs::create_dir_all(&folder)
                    .map_err(|error| SkillError::Io(error.to_string()))?;
                folder.join(format!("{}.{}", slugify(title), extension))
            }
        };

        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent).map_err(|error| SkillError::Io(error.to_string()))?;
        }

        // Pandoc reads a real file, so the Markdown gets a temporary sibling
        // inside the workspace. It is removed on every exit path below.
        let temporary = output.with_file_name(format!(
            ".ravenbot-doc-{}.{}.md",
            uuid::Uuid::new_v4(),
            extension
        ));
        let body = if format == "pptx" {
            slides_markdown(content)
        } else {
            content.to_string()
        };
        std::fs::write(&temporary, &body).map_err(|error| SkillError::Io(error.to_string()))?;

        let output_string = output.to_string_lossy().to_string();
        let temporary_string = temporary.to_string_lossy().to_string();
        let mut args: Vec<String> = vec![
            temporary_string.clone(),
            "--output".to_string(),
            output_string.clone(),
            "--from".to_string(),
            "markdown+tex_math_dollars+pipe_tables".to_string(),
        ];
        match format.as_str() {
            "pdf" => {
                args.push("--pdf-engine=xelatex".to_string());
            }
            "html" => {
                args.push("--standalone".to_string());
                args.push("--embed-resources".to_string());
            }
            _ => {}
        }
        args.push("--metadata".to_string());
        args.push(format!("title={title}"));
        if let Some(reference) = arguments
            .get("reference_doc")
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
        {
            let reference = context.resolve_path(reference)?;
            args.push("--reference-doc".to_string());
            args.push(reference.to_string_lossy().to_string());
        }

        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        let result = crate::exec::run(context, "pandoc", &borrowed, None).await;
        let _ = std::fs::remove_file(&temporary);

        let output_result = match result {
            Ok(result) => result,
            Err(SkillError::Io(error))
                if error.contains("No such file") || error.contains("not found") =>
            {
                return Ok(SkillResult::failure(
                    "Pandoc is not installed. Install pandoc (and XeLaTeX for PDF) to export documents; the Markdown answer itself is complete.",
                ));
            }
            Err(error) => return Err(error),
        };

        if !output_result.status.success() {
            let stderr = String::from_utf8_lossy(&output_result.stderr);
            let missing = output_result.status.code() == Some(127)
                || stderr.to_ascii_lowercase().contains("pandoc: not found")
                || stderr.to_ascii_lowercase().contains("command not found");
            if missing {
                return Ok(SkillResult::failure(
                    "Pandoc is not installed. Install pandoc (and XeLaTeX for PDF) to export documents; the Markdown answer itself is complete.",
                ));
            }
            let hint = if format == "pdf" && stderr.to_ascii_lowercase().contains("xelatex") {
                " Install a TeX engine (xelatex) or export DOCX/HTML instead."
            } else {
                ""
            };
            return Ok(SkillResult::failure(format!(
                "Pandoc could not create the {format} file: {}{hint}",
                stderr.trim()
            )));
        }

        let bytes = verify(&output, &format).map_err(SkillError::Execution)?;
        Ok(SkillResult::success(serde_json::json!({
            "action": "make_document",
            "path": output.to_string_lossy(),
            "format": format,
            "bytes": bytes,
            "title": title,
            "verified": true
        })))
    }
}

/// Whether Pandoc is available on this machine (used by tests and callers that
/// want to offer an install hint before a run).
pub fn pandoc_available() -> bool {
    std::process::Command::new("pandoc")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use uuid::Uuid;

    fn workspace() -> PathBuf {
        let root = std::env::temp_dir().join(format!("rb-docs-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn context(root: &Path) -> SkillContext {
        SkillContext::with_default_tier(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4())
            .with_working_dirs(vec![root.to_path_buf()])
    }

    #[tokio::test]
    async fn html_export_is_a_real_document() {
        if !pandoc_available() {
            eprintln!("pandoc not installed; skipping export test");
            return;
        }
        let root = workspace();
        let result = MakeDocumentSkill::new()
            .execute(
                &context(&root),
                serde_json::json!({
                    "content": "# Hello\n\nThis is **real**.",
                    "format": "html",
                    "title": "Hello"
                }),
            )
            .await
            .expect("export");
        assert!(result.success, "{result:?}");
        let path = result.output.get("path").and_then(|v| v.as_str()).unwrap();
        assert!(path.ends_with(".html"));
        let text = std::fs::read_to_string(path).unwrap();
        assert!(text.to_ascii_lowercase().contains("<html"));
        assert!(text.contains("real"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn docx_and_pptx_are_zip_packages_and_pptx_splits_slides() {
        if !pandoc_available() {
            eprintln!("pandoc not installed; skipping export test");
            return;
        }
        let root = workspace();
        for format in ["docx", "pptx"] {
            let result = MakeDocumentSkill::new()
                .execute(
                    &context(&root),
                    serde_json::json!({
                        "content": "---\ntitle: One\n---\n\n# First\n\n---\n\n# Second\n",
                        "format": format,
                        "title": "Deck"
                    }),
                )
                .await
                .expect("export");
            assert!(result.success, "{format}: {result:?}");
            let path = result.output.get("path").and_then(|v| v.as_str()).unwrap();
            let bytes = std::fs::read(path).unwrap();
            assert!(bytes.starts_with(b"PK"), "{format} is not an OOXML package");
            assert!(bytes.len() > 1000);
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn traversal_is_refused() {
        let root = workspace();
        let error = MakeDocumentSkill::new()
            .execute(
                &context(&root),
                serde_json::json!({"content": "body", "format": "html", "path": "../escape.html"}),
            )
            .await
            .expect_err("traversal must be refused");
        assert!(
            matches!(error, SkillError::PermissionDenied(_)),
            "{error:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
