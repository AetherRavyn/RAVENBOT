//! `make_slides` — a structured deck rendered to 1920×1080 images and a
//! full-bleed image PPTX.
//!
//! This is the designed-deck path. `make_document` produces editable text with
//! a plain theme; this produces the layout the model designed, as pixels, so
//! PowerPoint has nothing to reflow. The SVG sources stay beside the PNGs for
//! design edits.
//!
//! The deck is rendered in three verifiable steps:
//!
//! 1. code-defined vector layouts → one SVG per slide;
//! 2. `rsvg-convert` → one PNG per slide, with the PNG signature and IHDR
//!    dimensions checked rather than trusting the exit code;
//! 3. a minimal OOXML package with one full-bleed picture per 16:9 slide.

use async_trait::async_trait;
use ravenbot_core::Permission;
use serde::Deserialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

use crate::builtin::pptx;
use crate::builtin::slide_render::{self, SlideSpec, Theme, HEIGHT, WIDTH};
use crate::traits::{Skill, SkillContext, SkillError, SkillResult, SkillRisk};

pub struct MakeSlidesSkill;

impl MakeSlidesSkill {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MakeSlidesSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Default, Deserialize)]
struct DeckInput {
    #[serde(default)]
    title: String,
    #[serde(default)]
    theme: Option<Value>,
    #[serde(default)]
    slides: Vec<SlideSpec>,
    /// A plain Markdown deck, used when `slides` is absent. Slides are
    /// separated by a line containing only `---`.
    #[serde(default)]
    markdown: Option<String>,
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
        "deck".to_string()
    } else {
        out.chars().take(80).collect()
    }
}

/// Turn a Markdown deck into the structured slide model.
///
/// This is the fallback for a caller that already wrote the deck as prose:
/// headings become titles, list items become bullets, and the first paragraph
/// becomes a subtitle. It is deliberately simple; the structured spec carries
/// layouts, charts, and stats that Markdown cannot express.
fn slides_from_markdown(markdown: &str) -> Vec<SlideSpec> {
    let normalized = markdown.replace("\r\n", "\n");
    let mut chunks: Vec<Vec<&str>> = vec![Vec::new()];
    for line in normalized.lines() {
        if line.trim() == "---" {
            chunks.push(Vec::new());
        } else {
            chunks.last_mut().expect("at least one chunk").push(line);
        }
    }
    let mut slides = Vec::new();
    for chunk in chunks {
        let mut title = None;
        let mut bullets = Vec::new();
        let mut paragraphs = Vec::new();
        for line in chunk {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(heading) = trimmed.strip_prefix('#') {
                let heading = heading.trim_start_matches('#').trim();
                if !heading.is_empty() {
                    title = Some(heading.to_string());
                }
            } else if let Some(item) = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
                .or_else(|| trimmed.strip_prefix("+ "))
            {
                if !item.trim().is_empty() {
                    bullets.push(item.trim().to_string());
                }
            } else {
                paragraphs.push(trimmed.to_string());
            }
        }
        if title.is_none() && paragraphs.is_empty() && bullets.is_empty() {
            continue;
        }
        let layout = if !bullets.is_empty() {
            "bullets"
        } else if title.is_some() {
            "section"
        } else {
            "title"
        };
        let subtitle = paragraphs.join(" ");
        slides.push(SlideSpec {
            layout: layout.to_string(),
            title: title.or_else(|| {
                if paragraphs.is_empty() {
                    None
                } else {
                    Some(paragraphs[0].clone())
                }
            }),
            subtitle: if subtitle.is_empty() {
                None
            } else {
                Some(subtitle)
            },
            bullets,
            ..Default::default()
        });
    }
    slides
}

fn command_available(program: &str) -> bool {
    std::process::Command::new(program)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Read width/height from a PNG's IHDR chunk.
pub fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return None;
    }
    if &bytes[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    Some((width, height))
}

#[async_trait]
impl Skill for MakeSlidesSkill {
    fn id(&self) -> &str {
        "make_slides"
    }

    fn name(&self) -> &str {
        "Make designed slides"
    }

    fn description(&self) -> &str {
        "Render a designed deck from a structured spec: one 1920×1080 PNG per slide plus a .pptx with every image full-bleed on a blank 16:9 slide. Layouts: title, section, bullets, cards, stats, timeline, quote, chart, closing. Use make_document when editable slide text is wanted."
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
                "title": {"type": "string", "description": "Deck title; also the default folder/share name."},
                "theme": {
                    "type": "object",
                    "description": "Optional palette. Colours must be #RRGGBB; invalid values fall back.",
                    "properties": {
                        "background": {"type": "string"},
                        "surface": {"type": "string"},
                        "text": {"type": "string"},
                        "muted": {"type": "string"},
                        "accent": {"type": "string"},
                        "accent2": {"type": "string"},
                        "font": {"type": "string"}
                    }
                },
                "folder": {"type": "string", "description": "Optional output folder inside the workspace."},
                "filename": {"type": "string", "description": "Optional PPTX file name (default: deck.pptx)."},
                "markdown": {"type": "string", "description": "Fallback: a Markdown deck with slides separated by a line containing only ---. Used when slides is absent."},
                "slides": {
                    "type": "array",
                    "description": "Ordered slides. Each item is {layout, kicker?, title?, subtitle?, bullets?, cards?, stats?, items?, quote?, attribution?, chart?, footer?, number?}.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "layout": {"type": "string", "enum": ["title", "section", "bullets", "cards", "stats", "timeline", "quote", "chart", "closing"]},
                            "kicker": {"type": "string"},
                            "title": {"type": "string"},
                            "subtitle": {"type": "string"},
                            "bullets": {"type": "array", "items": {"type": "string"}},
                            "cards": {"type": "array", "items": {"type": "object", "properties": {"title": {"type": "string"}, "body": {"type": "string"}}}},
                            "stats": {"type": "array", "items": {"type": "object", "properties": {"value": {"type": "string"}, "label": {"type": "string"}}}},
                            "items": {"type": "array", "items": {"type": "object", "properties": {"when": {"type": "string"}, "what": {"type": "string"}}}},
                            "quote": {"type": "string"},
                            "attribution": {"type": "string"},
                            "chart": {"type": "object"},
                            "footer": {"type": "string"},
                            "number": {"type": "string"}
                        }
                    }
                }
            },
            "required": ["title"]
        })
    }

    fn risk(&self) -> SkillRisk {
        SkillRisk::High
    }

    async fn execute(
        &self,
        context: &SkillContext,
        arguments: Value,
    ) -> Result<SkillResult, SkillError> {
        let mut deck: DeckInput = serde_json::from_value(arguments.clone())
            .map_err(|error| SkillError::InvalidArguments(format!("Invalid deck spec: {error}")))?;
        if deck.slides.is_empty() {
            if let Some(markdown) = deck.markdown.as_deref() {
                deck.slides = slides_from_markdown(markdown);
            }
        }
        if deck.slides.is_empty() {
            return Err(SkillError::InvalidArguments(
                "A deck needs at least one slide, or a Markdown body with `---` separators."
                    .to_string(),
            ));
        }
        if deck.slides.len() > 40 {
            return Err(SkillError::InvalidArguments(
                "A deck may have at most 40 slides.".to_string(),
            ));
        }

        let title = if deck.title.trim().is_empty() {
            "Designed slides".to_string()
        } else {
            deck.title.trim().to_string()
        };
        let theme = Theme::from_value(deck.theme.as_ref());

        let root = context
            .primary_dir()
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .ok_or_else(|| SkillError::Execution("No workspace to write the deck into.".into()))?;
        let folder_arg = arguments
            .get("folder")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        let folder = match folder_arg {
            Some(raw) => context.resolve_path(raw)?,
            None => root.join("deliverables").join(slugify(&title)),
        };
        std::fs::create_dir_all(&folder).map_err(|error| SkillError::Io(error.to_string()))?;

        let mut images: Vec<PathBuf> = Vec::with_capacity(deck.slides.len());
        let mut slide_entries: Vec<Value> = Vec::with_capacity(deck.slides.len());
        for (index, slide) in deck.slides.iter().enumerate() {
            let number = index + 1;
            let stem = format!("slide-{number:02}");
            let svg_path = folder.join(format!("{stem}.svg"));
            let png_path = folder.join(format!("{stem}.png"));
            let svg = slide_render::render_slide(slide, index, &theme);
            std::fs::write(&svg_path, svg).map_err(|error| SkillError::Io(error.to_string()))?;

            if !command_available("rsvg-convert") {
                return Ok(SkillResult::failure(
                    "rsvg-convert is not installed, so the slide SVGs were written but no PNG/PPTX was produced. Install librsvg (rsvg-convert), then run make_slides again.",
                ));
            }
            let svg_string = svg_path.to_string_lossy().to_string();
            let png_string = png_path.to_string_lossy().to_string();
            let width = WIDTH.to_string();
            let height = HEIGHT.to_string();
            let args = [
                "-w",
                width.as_str(),
                "-h",
                height.as_str(),
                "-o",
                png_string.as_str(),
                svg_string.as_str(),
            ];
            let output = crate::exec::run(context, "rsvg-convert", &args, None).await?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Ok(SkillResult::failure(format!(
                    "rsvg-convert could not render slide {number}: {}",
                    stderr.trim()
                )));
            }
            let bytes = std::fs::read(&png_path).map_err(|error| {
                SkillError::Execution(format!(
                    "rsvg-convert reported success but '{}' is missing: {error}",
                    png_path.display()
                ))
            })?;
            match png_dimensions(&bytes) {
                Some((width, height)) if width == WIDTH && height == HEIGHT => {}
                Some((width, height)) => {
                    return Ok(SkillResult::failure(format!(
                        "Slide {number} rendered at {width}×{height}; expected {WIDTH}×{HEIGHT}."
                    )));
                }
                None => {
                    return Ok(SkillResult::failure(format!(
                        "Slide {number} is not a valid PNG."
                    )));
                }
            }

            images.push(png_path.clone());
            slide_entries.push(serde_json::json!({
                "index": number,
                "file": format!("{stem}.png"),
                "svg": format!("{stem}.svg"),
                "path": png_path.to_string_lossy(),
            }));
        }

        let filename = arguments
            .get("filename")
            .and_then(Value::as_str)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| "deck.pptx".to_string());
        if filename.contains('/') || filename.contains('\\') || filename.contains("..") {
            return Err(SkillError::InvalidArguments(
                "The PPTX file name must not contain a path.".to_string(),
            ));
        }
        let pptx_path = folder.join(if filename.to_ascii_lowercase().ends_with(".pptx") {
            filename
        } else {
            format!("{filename}.pptx")
        });
        let refs: Vec<&Path> = images.iter().map(PathBuf::as_path).collect();
        pptx::write_pptx(&refs, &pptx_path, &title).map_err(SkillError::Execution)?;
        pptx::verify_pptx(&pptx_path, images.len()).map_err(SkillError::Execution)?;

        Ok(SkillResult::success(serde_json::json!({
            "action": "make_slides",
            "title": title,
            "count": images.len(),
            "width": WIDTH,
            "height": HEIGHT,
            "folder": folder.to_string_lossy(),
            "slides": slide_entries,
            "pptx": pptx_path.to_string_lossy(),
            "verified": true
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn context(root: &Path) -> SkillContext {
        SkillContext::with_default_tier(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4())
            .with_working_dirs(vec![root.to_path_buf()])
    }

    #[test]
    fn png_dimensions_are_read_from_the_ihdr() {
        let mut bytes = vec![0u8; 33];
        bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        bytes[12..16].copy_from_slice(b"IHDR");
        bytes[16..20].copy_from_slice(&1920u32.to_be_bytes());
        bytes[20..24].copy_from_slice(&1080u32.to_be_bytes());
        assert_eq!(png_dimensions(&bytes), Some((1920, 1080)));
        assert_eq!(png_dimensions(b"not a png"), None);
    }

    #[tokio::test]
    async fn a_folder_that_escapes_the_workspace_is_refused() {
        let root = std::env::temp_dir().join(format!("rb-slides-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let error = MakeSlidesSkill::new()
            .execute(
                &context(&root),
                serde_json::json!({
                    "title": "Deck",
                    "folder": "../escape",
                    "slides": [{"layout": "title", "title": "Hello"}]
                }),
            )
            .await
            .expect_err("traversal must be refused");
        assert!(
            matches!(error, SkillError::PermissionDenied(_)),
            "{error:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_markdown_deck_becomes_slides() {
        let slides = slides_from_markdown(
            "# Opening\n\nOne line.\n\n---\n\n## What changes\n\n- First\n- Second\n",
        );
        assert_eq!(slides.len(), 2);
        assert_eq!(slides[0].title.as_deref(), Some("Opening"));
        assert_eq!(slides[0].subtitle.as_deref(), Some("One line."));
        assert_eq!(slides[1].layout, "bullets");
        assert_eq!(slides[1].bullets, vec!["First", "Second"]);
    }

    #[tokio::test]
    async fn an_empty_deck_is_refused() {
        let root = std::env::temp_dir().join(format!("rb-slides-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let error = MakeSlidesSkill::new()
            .execute(
                &context(&root),
                serde_json::json!({"title": "Deck", "slides": []}),
            )
            .await
            .expect_err("empty deck must be refused");
        assert!(
            matches!(error, SkillError::InvalidArguments(_)),
            "{error:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn a_deck_renders_pngs_and_a_pptx_that_reads_back() {
        if !command_available("rsvg-convert") {
            eprintln!("rsvg-convert not installed; skipping render test");
            return;
        }
        let root = std::env::temp_dir().join(format!("rb-slides-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let result = MakeSlidesSkill::new()
            .execute(
                &context(&root),
                serde_json::json!({
                    "title": "Quarterly Review",
                    "slides": [
                        {"layout": "title", "kicker": "Q3", "title": "Quarterly Review", "subtitle": "A real deck"},
                        {"layout": "stats", "title": "Numbers", "stats": [{"value": "42%", "label": "Growth"}]},
                        {"layout": "chart", "title": "Trend", "chart": {"type": "bar", "labels": ["A", "B"], "values": [3, 7]}}
                    ]
                }),
            )
            .await
            .expect("render");
        assert!(result.success, "{result:?}");
        let output = &result.output;
        assert_eq!(output.get("count").and_then(Value::as_u64), Some(3));
        let slides = output.get("slides").and_then(Value::as_array).unwrap();
        for entry in slides {
            let path = entry.get("path").and_then(Value::as_str).unwrap();
            let bytes = std::fs::read(path).unwrap();
            assert_eq!(png_dimensions(&bytes), Some((1920, 1080)));
        }
        let pptx_path = output.get("pptx").and_then(Value::as_str).unwrap();
        pptx::verify_pptx(Path::new(pptx_path), 3).unwrap();
        let _ = std::fs::remove_dir_all(&root);
    }
}
