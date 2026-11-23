//! In-process standard unified-diff parsing and application.
//!
//! `code_edit` historically shelled out to `patch(1)`. That makes a core edit
//! tool depend on a binary that may not be installed, gives no control over
//! what a partially applied multi-file patch leaves behind, and cannot report a
//! line-level diff back to the ledger for every file. This module parses the
//! same `---`/`+++`/`@@` format and applies it with exact context checks.
//!
//! It deliberately supports only what an agent edit loop needs: update, add,
//! and delete of UTF-8 text files. Renames, binary patches, and git metadata
//! are rejected with a clear message rather than half-applied.

/// One file section of a unified diff.
#[derive(Debug, Clone, Default)]
pub struct UnifiedFile {
    /// `None` when the file is being added (`--- /dev/null`).
    pub old_path: Option<String>,
    /// `None` when the file is being deleted (`+++ /dev/null`).
    pub new_path: Option<String>,
    pub hunks: Vec<UnifiedHunk>,
    /// The section exactly as it appeared, for the file-change ledger.
    pub raw: String,
}

impl UnifiedFile {
    /// The path the operation targets.
    pub fn path(&self) -> Option<&str> {
        self.new_path.as_deref().or(self.old_path.as_deref())
    }

    pub fn is_add(&self) -> bool {
        self.old_path.is_none() && self.new_path.is_some()
    }

    pub fn is_delete(&self) -> bool {
        self.new_path.is_none() && self.old_path.is_some()
    }

    pub fn lines_added(&self) -> usize {
        self.hunks.iter().map(|hunk| hunk.added).sum()
    }

    pub fn lines_deleted(&self) -> usize {
        self.hunks.iter().map(|hunk| hunk.deleted).sum()
    }
}

/// One `@@ -a,b +c,d @@` hunk.
#[derive(Debug, Clone, Default)]
pub struct UnifiedHunk {
    pub old_start: usize,
    pub old_count: usize,
    pub new_start: usize,
    pub new_count: usize,
    /// Context and removed lines, without their ` `/`-` prefix.
    pub old: Vec<String>,
    /// Context and added lines, without their ` `/`+` prefix.
    pub new: Vec<String>,
    pub added: usize,
    pub deleted: usize,
    /// A `\ No newline at end of file` marker appeared in this hunk.
    pub no_newline: bool,
}

/// Parse a standard unified diff.
///
/// The parser trusts the hunk counts rather than the line prefixes to know
/// where a hunk ends. That is what stops a removed line whose text begins with
/// `--` (a Markdown rule, a commented-out shell line) from being mistaken for
/// a `---` file header.
pub fn parse(patch: &str) -> Result<Vec<UnifiedFile>, String> {
    let lines: Vec<&str> = patch.lines().collect();
    let mut files = Vec::new();
    let mut current: Option<UnifiedFile> = None;
    let mut raw: Vec<String> = Vec::new();

    let flush =
        |files: &mut Vec<UnifiedFile>, current: &mut Option<UnifiedFile>, raw: &mut Vec<String>| {
            if let Some(mut file) = current.take() {
                file.raw = raw.join("\n");
                files.push(file);
            }
            raw.clear();
        };

    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i];

        if line.starts_with("diff --git ") {
            flush(&mut files, &mut current, &mut raw);
            current = Some(UnifiedFile::default());
            raw.push(line.to_string());
            i += 1;
            continue;
        }

        // A `---`/`+++` pair at the top level starts a file. `---` inside a
        // hunk is consumed by the count-driven loop below, never here.
        if line.starts_with("--- ") && i + 1 < lines.len() && lines[i + 1].starts_with("+++ ") {
            let old = parse_path(&line[4..]);
            let new = parse_path(&lines[i + 1][4..]);
            if old.is_none() && new.is_none() {
                return Err("A file header cannot have /dev/null on both sides.".to_string());
            }
            if let (Some(old), Some(new)) = (&old, &new) {
                if old != new {
                    return Err(format!(
                        "Renaming '{old}' to '{new}' is not supported in one patch. Send a delete for the old path and an add for the new one."
                    ));
                }
            }
            // A `diff --git` line creates an empty section that this pair
            // fills. A second pair after hunks starts the next file.
            let already_started = current.as_ref().is_some_and(|file| {
                file.old_path.is_some() || file.new_path.is_some() || !file.hunks.is_empty()
            });
            if already_started {
                flush(&mut files, &mut current, &mut raw);
            }
            if current.is_none() {
                current = Some(UnifiedFile::default());
            }
            {
                let file = current.as_mut().expect("current file");
                file.old_path = old;
                file.new_path = new;
            }
            raw.push(line.to_string());
            raw.push(lines[i + 1].to_string());
            i += 2;
            continue;
        }

        if line.starts_with("@@") {
            let Some(file) = current.as_mut() else {
                return Err(format!("A hunk appeared before any file header: {line}"));
            };
            let header = parse_hunk_header(line)?;
            raw.push(line.to_string());
            i += 1;
            let mut hunk = UnifiedHunk {
                old_start: header.0,
                old_count: header.1,
                new_start: header.2,
                new_count: header.3,
                ..UnifiedHunk::default()
            };
            let mut old_remaining = hunk.old_count;
            let mut new_remaining = hunk.new_count;
            while i < lines.len() && (old_remaining > 0 || new_remaining > 0) {
                let hunk_line = lines[i];
                if let Some(rest) = hunk_line.strip_prefix(' ') {
                    hunk.old.push(rest.to_string());
                    hunk.new.push(rest.to_string());
                    old_remaining = old_remaining.saturating_sub(1);
                    new_remaining = new_remaining.saturating_sub(1);
                } else if let Some(rest) = hunk_line.strip_prefix('-') {
                    hunk.old.push(rest.to_string());
                    hunk.deleted += 1;
                    old_remaining = old_remaining.saturating_sub(1);
                } else if let Some(rest) = hunk_line.strip_prefix('+') {
                    hunk.new.push(rest.to_string());
                    hunk.added += 1;
                    new_remaining = new_remaining.saturating_sub(1);
                } else if hunk_line.starts_with('\\') {
                    hunk.no_newline = true;
                } else {
                    return Err(format!(
                        "Hunk line '{}' does not start with ' ', '-', or '+'.",
                        hunk_line
                    ));
                }
                raw.push(hunk_line.to_string());
                i += 1;
            }
            if old_remaining > 0 || new_remaining > 0 {
                return Err(format!(
                    "The hunk at line {} ended early (needed {old_remaining} more old and {new_remaining} more new lines).",
                    hunk.old_start
                ));
            }
            // A no-newline marker follows the line it applies to.
            while i < lines.len() && lines[i].starts_with('\\') {
                hunk.no_newline = true;
                raw.push(lines[i].to_string());
                i += 1;
            }
            file.hunks.push(hunk);
            continue;
        }

        // Metadata lines belong to the current section's raw text. Anything
        // else at the top level is ignored rather than mistaken for content.
        if current.is_some() {
            raw.push(line.to_string());
        }
        i += 1;
    }

    flush(&mut files, &mut current, &mut raw);

    for file in &files {
        if file.hunks.is_empty() && !file.is_add() && !file.is_delete() {
            return Err(format!(
                "{} has no hunks. A patch that only changes file mode or index metadata is not supported.",
                file.path().unwrap_or("The patch")
            ));
        }
    }
    Ok(files)
}

fn parse_path(raw: &str) -> Option<String> {
    // Timestamps are separated by a tab; the path is before it.
    let value = raw.split('\t').next().unwrap_or(raw).trim();
    if value == "/dev/null" {
        return None;
    }
    let value = value
        .strip_prefix("a/")
        .or_else(|| value.strip_prefix("b/"))
        .unwrap_or(value);
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn parse_hunk_header(line: &str) -> Result<(usize, usize, usize, usize), String> {
    let body = line
        .strip_prefix("@@")
        .ok_or_else(|| format!("Not a hunk header: {line}"))?;
    let body = body.split("@@").next().unwrap_or(body).trim();
    let mut parts = body.split_whitespace();
    let old = parts
        .next()
        .ok_or_else(|| format!("A hunk header is missing its old range: {line}"))?;
    let new = parts
        .next()
        .ok_or_else(|| format!("A hunk header is missing its new range: {line}"))?;
    let (old_start, old_count) = parse_range(old, '-')?;
    let (new_start, new_count) = parse_range(new, '+')?;
    Ok((old_start, old_count, new_start, new_count))
}

fn parse_range(value: &str, sign: char) -> Result<(usize, usize), String> {
    let value = value
        .strip_prefix(sign)
        .ok_or_else(|| format!("A hunk range must start with '{sign}': {value}"))?;
    let (start, count) = value.split_once(',').unwrap_or((value, "1"));
    let start = start
        .parse::<usize>()
        .map_err(|_| format!("Invalid hunk start '{start}'."))?;
    let count = count
        .parse::<usize>()
        .map_err(|_| format!("Invalid hunk count '{count}'."))?;
    Ok((start, count))
}

/// Apply one file section to its original text.
///
/// Line endings are preserved: a CRLF file stays CRLF, and an added line uses
/// the file's existing ending. A `\ No newline at end of file` marker removes
/// the final newline.
pub fn apply(original: &str, file: &UnifiedFile) -> Result<String, String> {
    if file.is_delete() {
        return Ok(String::new());
    }

    let line_ending = if original.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let had_newline = original.ends_with('\n');
    let mut lines = split_lines(original);
    let mut cursor = 0usize;
    let mut delta: isize = 0;
    let mut no_trailing_newline = false;

    for (index, hunk) in file.hunks.iter().enumerate() {
        let expected = if hunk.old_start > 0 {
            (hunk.old_start - 1) as isize + delta
        } else {
            cursor as isize
        };
        let max_start = lines.len().saturating_sub(hunk.old.len());
        let expected = expected.clamp(0, max_start as isize) as usize;
        let position = find_hunk(&lines, &hunk.old, expected, cursor).ok_or_else(|| {
            format!(
                "hunk {} (line {}) did not match the file. Read the file and include the exact surrounding lines.",
                index + 1,
                hunk.old_start
            )
        })?;
        if position < cursor {
            return Err(format!(
                "hunk {} is out of order or overlaps an earlier hunk.",
                index + 1
            ));
        }
        lines.splice(
            position..position + hunk.old.len(),
            hunk.new.iter().cloned(),
        );
        delta += hunk.new.len() as isize - hunk.old.len() as isize;
        cursor = position + hunk.new.len();
        no_trailing_newline |= hunk.no_newline;
    }

    let mut content = lines.join(line_ending);
    if !lines.is_empty() && (had_newline || original.is_empty()) && !no_trailing_newline {
        content.push_str(line_ending);
    }
    Ok(content)
}

fn split_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    text.split_inclusive('\n')
        .map(|line| {
            let line = line.strip_suffix('\n').unwrap_or(line);
            line.strip_suffix('\r').unwrap_or(line).to_string()
        })
        .collect()
}

fn find_hunk(lines: &[String], needle: &[String], expected: usize, min: usize) -> Option<usize> {
    if needle.is_empty() {
        return Some(expected.min(lines.len()));
    }
    if needle.len() > lines.len() {
        return None;
    }
    let max = lines.len() - needle.len();
    let expected = expected.min(max);
    for position in expected..=max {
        if lines[position..position + needle.len()] == *needle {
            return Some(position);
        }
    }
    if expected > min {
        for position in (min..expected).rev() {
            if lines[position..position + needle.len()] == *needle {
                return Some(position);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update(patch: &str) -> UnifiedFile {
        crate::patch::parse(patch).expect("parse").remove(0)
    }

    #[test]
    fn a_standard_update_preserves_context_and_counts() {
        let patch = "--- a/notes.txt\n+++ b/notes.txt\n@@ -1,3 +1,3 @@\n one\n-two\n+TWO\n three\n";
        let file = update(patch);
        assert_eq!(file.path(), Some("notes.txt"));
        assert_eq!(file.lines_added(), 1);
        assert_eq!(file.lines_deleted(), 1);
        assert_eq!(
            apply("one\ntwo\nthree\n", &file).unwrap(),
            "one\nTWO\nthree\n"
        );
    }

    #[test]
    fn multiple_hunks_are_applied_in_order() {
        let patch = "--- a/a.txt\n+++ b/a.txt\n@@ -1,2 +1,2 @@\n-one\n+ONE\n two\n@@ -4,1 +4,1 @@\n-four\n+FOUR\n";
        let file = update(patch);
        assert_eq!(
            apply("one\ntwo\nthree\nfour\n", &file).unwrap(),
            "ONE\ntwo\nthree\nFOUR\n"
        );
    }

    #[test]
    fn adds_and_deletes_use_dev_null() {
        let add = update("--- /dev/null\n+++ b/new.txt\n@@ -0,0 +1,2 @@\n+hello\n+world\n");
        assert!(add.is_add());
        assert_eq!(apply("", &add).unwrap(), "hello\nworld\n");

        let delete = update("--- a/old.txt\n+++ /dev/null\n@@ -1,2 +0,0 @@\n-bye\n-now\n");
        assert!(delete.is_delete());
        assert_eq!(apply("bye\nnow\n", &delete).unwrap(), "");
    }

    #[test]
    fn a_mismatched_context_fails() {
        let patch = "--- a/a.txt\n+++ b/a.txt\n@@ -1,1 +1,1 @@\n-missing\n+new\n";
        let file = update(patch);
        assert!(apply("present\n", &file).is_err());
    }

    #[test]
    fn crlf_and_no_final_newline_are_preserved() {
        let patch = "--- a/a.txt\n+++ b/a.txt\n@@ -1,1 +1,1 @@\n-old\n+new\n";
        let file = update(patch);
        assert_eq!(apply("old\r\n", &file).unwrap(), "new\r\n");

        let no_newline =
            "--- a/a.txt\n+++ b/a.txt\n@@ -1,1 +1,1 @@\n-old\n+new\n\\ No newline at end of file\n";
        let file = update(no_newline);
        assert_eq!(apply("old", &file).unwrap(), "new");
    }

    #[test]
    fn multiple_file_sections_are_parsed_without_diff_git_headers() {
        let patch = "--- a/a.txt\n+++ b/a.txt\n@@ -1,1 +1,1 @@\n-a\n+A\n--- /dev/null\n+++ b/new.txt\n@@ -0,0 +1,1 @@\n+hello\n--- a/old.txt\n+++ /dev/null\n@@ -1,1 +0,0 @@\n-bye\n";
        let files = parse(patch).expect("parse");
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].path(), Some("a.txt"));
        assert!(files[1].is_add());
        assert_eq!(files[1].path(), Some("new.txt"));
        assert!(files[2].is_delete());
        assert_eq!(files[2].path(), Some("old.txt"));
    }

    #[test]
    fn a_rename_is_rejected_instead_of_half_applied() {
        let error = parse("--- a/old.txt\n+++ b/new.txt\n@@ -1,1 +1,1 @@\n-a\n+b\n").unwrap_err();
        assert!(error.contains("Renaming"), "{error}");
    }
}
