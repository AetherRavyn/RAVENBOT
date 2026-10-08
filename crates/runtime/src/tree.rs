//! Listing what is inside a workspace, so a person can see the agents' work.
//!
//! The isolation is enforced on the way *in* — `SkillContext::resolve_path`
//! refuses a path outside the workspace, and `confined()` re-checks containment
//! after canonicalising. Nothing in the app shows the result of that, so a user
//! configures a folder, watches an agent run for a while, and has no way to
//! find out whether it produced anything, wrote to the wrong place, or silently
//! did nothing at all. Setting a path string and never seeing the tree is how a
//! workspace stops feeling like a real room and starts feeling like a guess.
//!
//! This is a *read* of directories the user already pointed us at, so it is
//! bounded rather than sandboxed, and the bounds are the point:
//!
//!  - **Depth and entry caps.** A `node_modules` or a build tree is thousands
//!    of entries deep. Unbounded, one click would hang the UI thread on a
//!    directory the user chose not to think about.
//!  - **No symlink following.** A link out of the workspace would otherwise let
//!    the browser read the rest of the disk through a path the confinement
//!    check never saw. Links are listed as links and not descended into.
//!  - **Hidden files off by default.** `.git` is the reason most workspace
//!    trees are useless to look at, and no one browsing deliverables wants it.
//!  - **A truncation flag.** When a cap bites, the result says so. A silently
//!    short tree reads as "this is everything", which is the one thing a file
//!    browser must never imply.

use std::path::{Path, PathBuf};

/// A file or directory as the browser shows it.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// Path relative to the root that was listed, so the UI never has to do
    /// path arithmetic to render a tree.
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    /// `true` for a symlink. A link is never descended into, so this is the
    /// only thing the UI needs to decide not to offer a "open folder" action.
    pub is_link: bool,
    pub size_bytes: u64,
    /// `Some` when the file is small enough to preview inline.
    pub preview: Option<String>,
    /// Files a plain preview would mangle. Derived from the extension, not from
    /// sniffing, because a file that lies about its content should not be
    /// handed to the renderer as text.
    pub is_binary: bool,
}

impl Entry {
    /// Whether this entry is worth a preview pane, and small enough to send.
    fn preview_text(path: &Path, size: u64) -> (Option<String>, bool) {
        if BINARY_EXTENSIONS
            .iter()
            .any(|ext| path.to_string_lossy().to_ascii_lowercase().ends_with(ext))
        {
            return (None, true);
        }
        // A megabyte of text in a list response is a megabyte per row, and the
        // user is looking at a tree, not reading a file.
        if size > MAX_PREVIEW_BYTES {
            return (None, false);
        }
        match std::fs::read(path) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(text) => (Some(text), false),
                // Not valid UTF-8, so not text, whatever it is called.
                Err(_) => (None, true),
            },
            Err(_) => (None, false),
        }
    }
}

/// A listed tree, plus what was left out.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tree {
    /// The root that was listed, resolved.
    pub root: String,
    pub entries: Vec<Entry>,
    /// The root does not exist. Separate from `entries` being empty, because
    /// "the workspace has not been created yet" and "the workspace is empty" are
    /// different states and the user needs to be able to tell them apart.
    pub missing: bool,
    pub truncated: bool,
    /// Why it was truncated, in words the UI can show.
    pub note: Option<String>,
    pub file_count: usize,
    pub dir_count: usize,
    pub total_bytes: u64,
}

const MAX_DEPTH: usize = 3;
const MAX_ENTRIES: usize = 2000;
const MAX_PREVIEW_BYTES: u64 = 256 * 1024;
const MAX_SUMMARY_ENTRIES: usize = 400;

/// Extensions that are never shown as text.
///
/// A preview that renders binary as mojibake looks like a bug in the app rather
/// than a file of the wrong kind, so the browser declines instead.
const BINARY_EXTENSIONS: &[&str] = &[
    ".png", ".jpg", ".jpeg", ".gif", ".webp", ".avif", ".bmp", ".ico", ".icns", ".tiff", ".pdf",
    ".zip", ".gz", ".bz2", ".xz", ".tar", ".7z", ".rar", ".jar", ".class", ".so", ".dylib", ".dll",
    ".exe", ".bin", ".o", ".a", ".wasm", ".woff", ".woff2", ".ttf", ".otf", ".eot", ".mp3", ".wav",
    ".ogg", ".flac", ".m4a", ".mp4", ".mov", ".avi", ".mkv", ".webm", ".sqlite", ".db",
];

/// Hidden names skipped unless asked for.
const ALWAYS_HIDDEN: &[&str] = &[".DS_Store", "Thumbs.db"];

/// Whether a name should be left out of the listing.
fn is_hidden(name: &str, show_hidden: bool) -> bool {
    !show_hidden && name.starts_with('.')
}

/// List `root` for the workspace browser.
///
/// `show_hidden` includes dotfiles, which is the switch that makes `.git` and
/// `.env` appear — useful when an agent says it created a file and the user
/// cannot find it.
pub fn list(root: &Path, show_hidden: bool) -> Tree {
    let resolved = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let canonical = std::fs::canonicalize(&resolved).unwrap_or_else(|_| resolved.clone());

    let mut tree = Tree {
        root: canonical.to_string_lossy().to_string(),
        entries: Vec::new(),
        missing: false,
        truncated: false,
        note: None,
        file_count: 0,
        dir_count: 0,
        total_bytes: 0,
    };

    if !canonical.is_dir() {
        tree.missing = true;
        tree.note = Some(format!(
            "{} does not exist yet. It is created the first time an agent works here.",
            canonical.to_string_lossy()
        ));
        return tree;
    }

    let mut budget = MAX_ENTRIES;
    let mut entries = Vec::new();
    walk(
        &canonical,
        &canonical,
        0,
        show_hidden,
        &mut budget,
        &mut entries,
        &mut tree,
    );

    entries.sort_by(|a, b| {
        // Folders first, then case-insensitive by name. A tree that interleaves
        // files and folders is much harder to scan.
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    tree.entries = entries;

    if tree.truncated {
        tree.note = Some(format!(
            "Showing the first {MAX_ENTRIES} entries, up to {MAX_DEPTH} levels deep. \
             Narrow the folder or turn on hidden files to see more."
        ));
    }
    tree
}

/// Depth-first from `dir`, writing into `out` while counting into `tree`.
fn walk(
    root: &Path,
    dir: &Path,
    depth: usize,
    show_hidden: bool,
    budget: &mut usize,
    out: &mut Vec<Entry>,
    tree: &mut Tree,
) {
    if depth >= MAX_DEPTH || *budget == 0 {
        if *budget == 0 {
            tree.truncated = true;
        }
        return;
    }

    let read = match std::fs::read_dir(dir) {
        Ok(r) => r,
        // A directory the user cannot read is a fact about their permissions,
        // not an error to report; the rest of the tree is still useful.
        Err(_) => return,
    };

    // Collected first so the sort is stable and a partial read cannot leave the
    // budget inconsistent with what was emitted.
    let mut children: Vec<(PathBuf, String, bool, bool, u64)> = Vec::new();
    for item in read.flatten() {
        let name = item.file_name().to_string_lossy().to_string();
        if ALWAYS_HIDDEN.contains(&name.as_str()) || is_hidden(&name, show_hidden) {
            continue;
        }
        let path = item.path();
        // `symlink_metadata` does not follow the link, which is exactly the
        // point: a link into the rest of the disk must be reported as a link,
        // not walked into.
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        let is_link = meta.file_type().is_symlink();
        let is_dir = meta.is_dir();
        let size = if is_dir { 0 } else { meta.len() };
        children.push((path, name, is_dir, is_link, size));
    }

    for (path, name, is_dir, is_link, size) in children {
        if *budget == 0 {
            tree.truncated = true;
            return;
        }
        *budget -= 1;

        let (preview, is_binary) = if is_dir || is_link {
            // A link is never read either. `fs::read` follows it, so previewing
            // a link to a file would hand the contents of whatever it points at
            // — possibly outside the workspace entirely — to the UI as though it
            // were a file in the room.
            (None, false)
        } else {
            Entry::preview_text(&path, size)
        };

        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();

        if is_dir {
            tree.dir_count += 1;
        } else {
            tree.file_count += 1;
            tree.total_bytes += size;
        }

        out.push(Entry {
            path: rel,
            name,
            is_dir,
            is_link,
            size_bytes: size,
            preview,
            is_binary,
        });

        // A link is listed and not descended into: following it would read
        // through a path the workspace confinement never checked.
        if is_dir && !is_link {
            walk(root, &path, depth + 1, show_hidden, budget, out, tree);
        }
    }
}

/// A one-line description of a workspace, for a header or a tooltip.
///
/// Separate from [`list`] because the office header wants this on every render
/// and a full listing on none of them.
pub fn summarize(root: &Path) -> String {
    let canonical = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    if !canonical.is_dir() {
        return format!("{} (not created yet)", canonical.to_string_lossy());
    }
    let counts = count(&canonical, MAX_SUMMARY_ENTRIES);

    let human = human_bytes(counts.bytes);
    if counts.files == 0 && counts.dirs == 0 {
        format!("{} — empty", canonical.to_string_lossy())
    } else {
        format!(
            "{} — {files} file{s}, {dirs} folder{s}, {human}",
            canonical.to_string_lossy(),
            files = counts.files,
            s = if counts.files == 1 { "" } else { "s" },
            dirs = counts.dirs,
        )
    }
}

/// File, folder and byte totals for a tree, without building it.
struct Counts {
    files: usize,
    dirs: usize,
    bytes: u64,
}

/// Count a tree without materialising entries or previews.
///
/// A summary is rendered in a list of workspaces, so it cannot afford to read
/// the files it is counting. This walks names and sizes only.
fn count(root: &Path, budget: usize) -> Counts {
    let mut totals = Counts {
        files: 0,
        dirs: 0,
        bytes: 0,
    };
    let mut left = budget;
    count_into(root, 0, &mut left, &mut totals);
    totals
}

fn count_into(dir: &Path, depth: usize, budget: &mut usize, totals: &mut Counts) {
    if depth >= MAX_DEPTH || *budget == 0 {
        return;
    }
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for item in read.flatten() {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        let name = item.file_name().to_string_lossy().to_string();
        if ALWAYS_HIDDEN.contains(&name.as_str()) || is_hidden(&name, false) {
            continue;
        }
        let Ok(meta) = std::fs::symlink_metadata(item.path()) else {
            continue;
        };
        if meta.is_dir() {
            totals.dirs += 1;
            // Never through a link, for the same reason the browser does not.
            if !meta.file_type().is_symlink() {
                count_into(&item.path(), depth + 1, budget, totals);
            }
        } else {
            totals.files += 1;
            totals.bytes += meta.len();
        }
    }
}

/// A byte count a person would write down.
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_root(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ravenbot-tree-{tag}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn lists_files_and_folders() {
        let root = temp_root("basic");
        fs::create_dir_all(root.join("deliverables")).unwrap();
        fs::write(root.join("OFFICE.md"), "# Office\n").unwrap();
        fs::write(root.join("deliverables/report.md"), "the report\n").unwrap();

        let tree = list(&root, false);
        assert!(!tree.missing);
        assert_eq!(tree.file_count, 2);
        assert_eq!(tree.dir_count, 1);
        // Folders sort above files.
        assert_eq!(tree.entries[0].name, "deliverables");
        assert!(tree.entries[0].is_dir);
        // Nested content is reached, and reported relative to the root.
        let nested = tree.entries.iter().find(|e| e.name == "report.md").unwrap();
        assert_eq!(nested.path, "deliverables/report.md");
        assert_eq!(nested.preview.as_deref(), Some("the report\n"));
    }

    #[test]
    fn hidden_files_are_opt_in() {
        let root = temp_root("hidden");
        fs::write(root.join(".env"), "SECRET=1\n").unwrap();
        fs::write(root.join("visible.txt"), "hi\n").unwrap();

        assert_eq!(list(&root, false).file_count, 1);
        assert_eq!(list(&root, true).file_count, 2);
    }

    /// A workspace is a directory the user pointed at, and a symlink inside it
    /// can reach anywhere. Listing through one would turn a read of the
    /// workspace into a read of the disk.
    #[test]
    fn a_symlink_is_listed_but_never_followed() {
        let root = temp_root("link");
        let outside = temp_root("link-outside");
        fs::write(outside.join("secret.txt"), "not yours\n").unwrap();
        fs::create_dir_all(root.join("escape")).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("escape/link")).unwrap();

        let tree = list(&root, false);
        let link = tree
            .entries
            .iter()
            .find(|e| e.name == "link")
            .expect("the link should be listed");
        assert!(link.is_link, "the link should be marked as one");
        assert!(
            !tree.entries.iter().any(|e| e.name == "secret.txt"),
            "the listing followed a symlink out of the workspace"
        );
    }

    #[test]
    fn a_symlink_to_a_file_is_not_previewed() {
        let root = temp_root("linkfile");
        let outside = temp_root("linkfile-outside");
        fs::write(outside.join("secret.txt"), "not yours\n").unwrap();
        std::os::unix::fs::symlink(outside.join("secret.txt"), root.join("link.txt")).unwrap();

        let tree = list(&root, false);
        let link = tree.entries.iter().find(|e| e.name == "link.txt").unwrap();
        assert!(link.is_link);
        assert!(
            link.preview.is_none(),
            "followed a symlink to read its target"
        );
    }

    #[test]
    fn depth_is_bounded() {
        let root = temp_root("deep");
        let deep = root.join("a/b/c/d/e");
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("buried.txt"), "too far\n").unwrap();

        let tree = list(&root, false);
        assert!(
            !tree.entries.iter().any(|e| e.name == "buried.txt"),
            "the depth cap did not hold"
        );
    }

    #[test]
    fn the_entry_cap_truncates_and_says_so() {
        let root = temp_root("many");
        for i in 0..(MAX_ENTRIES + 50) {
            fs::write(root.join(format!("f{i:05}.txt")), "x").unwrap();
        }
        let tree = list(&root, false);
        assert!(tree.truncated);
        assert_eq!(tree.entries.len(), MAX_ENTRIES);
        // A short tree that does not say it is short reads as "this is
        // everything", which is the one thing this must not imply.
        let note = tree.note.expect("truncation must be explained");
        assert!(note.contains(&MAX_ENTRIES.to_string()), "{note}");
    }

    #[test]
    fn a_missing_workspace_is_distinct_from_an_empty_one() {
        let root = temp_root("gone");
        let gone = root.join("not-created-yet");
        let tree = list(&gone, false);
        assert!(tree.missing, "a missing root must not look empty");
        assert!(tree.note.is_some());

        let empty = temp_root("empty");
        let tree = list(&empty, false);
        assert!(!tree.missing);
        assert_eq!(tree.file_count, 0);
    }

    #[test]
    fn a_binary_file_is_not_sent_as_text() {
        let root = temp_root("bin");
        // A PNG header, which is also the honest test: the bytes are not UTF-8.
        fs::write(
            root.join("logo.png"),
            [0x89u8, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0xff, 0xfe],
        )
        .unwrap();

        let tree = list(&root, false);
        let png = tree.entries.iter().find(|e| e.name == "logo.png").unwrap();
        assert!(png.is_binary);
        assert!(
            png.preview.is_none(),
            "binary content was sent as a text preview"
        );
    }

    #[test]
    fn a_large_text_file_is_listed_without_its_content() {
        let root = temp_root("big");
        let big = "x".repeat((MAX_PREVIEW_BYTES + 1) as usize);
        fs::write(root.join("big.txt"), big).unwrap();

        let tree = list(&root, false);
        let entry = tree.entries.iter().find(|e| e.name == "big.txt").unwrap();
        assert!(entry.preview.is_none(), "sent a quarter-megabyte per row");
        assert!(!entry.is_binary, "large is not the same as binary");
    }

    #[test]
    fn byte_counts_are_human_readable() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1024), "1.0 KB");
        assert_eq!(human_bytes(1536), "1.5 KB");
        assert_eq!(human_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(human_bytes(3 * 1024 * 1024 * 1024), "3.0 GB");
    }

    #[test]
    fn a_summary_reports_what_is_there() {
        let root = temp_root("summary");
        fs::create_dir_all(root.join("notes")).unwrap();
        fs::write(root.join("notes/a.md"), "a").unwrap();
        fs::write(root.join("b.md"), "bb").unwrap();

        let s = summarize(&root);
        assert!(s.contains("2 files"), "{s}");
        assert!(s.contains("1 folder"), "{s}");
        assert!(s.contains("3 B"), "{s}");
    }

    #[test]
    fn a_summary_of_a_missing_workspace_says_so() {
        let root = temp_root("summary-gone");
        let s = summarize(&root.join("never-created"));
        assert!(s.contains("not created yet"), "{s}");
    }
}
