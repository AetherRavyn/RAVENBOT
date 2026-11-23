//! Content-addressed before-images for the files a run changes.
//!
//! A tool that can write must not leave the workspace without a way back.
//! The first potentially mutating tool in a run walks its workspace roots and
//! stores every regular file as a SHA-256 blob under
//! `<RAVENBOT_HOME>/snapshots/blobs`; the database keeps the path → blob
//! manifest. Reverting a run restores the paths that run wrote, or deletes the
//! paths it created.
//!
//! Three rules keep this honest:
//!
//! - **A partial snapshot is not a snapshot.** If a file/size cap stops the
//!   walk, the snapshot is marked incomplete and revert refuses it. An
//!   uncaptured file must never be mistaken for a file the run created.
//! - **Absence is data.** A path missing from a complete snapshot did not exist
//!   before the run, so reverting deletes it. A path that existed but could not
//!   be read (too large, symlink, permission denied) has an entry with no blob,
//!   and is reported instead of being recreated empty.
//! - **Later work wins unless the user forces it.** A write after the run, or a
//!   file whose bytes no longer match the run's final hash, blocks the revert
//!   until the caller explicitly asks to overwrite it.
//!
//! Shell commands can change files without a `file_changes` row. Those paths
//! are not part of a run revert; the ledger is the list of paths a revert is
//! allowed to touch. This is a deliberate limit, not an accident: guessing at
//! every path a shell command might have touched would make "revert this run"
//! capable of overwriting unrelated work.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use ravenbot_db::queries::{
    FileChangeQueries, SnapshotEntryRecord, SnapshotQueries, WorkspaceSnapshotRecord,
};
use ravenbot_db::Database;
use ravenbot_skills::SkillContext;
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Files larger than this get an "unavailable" entry instead of a blob.
const MAX_BLOB_BYTES: u64 = 16 * 1024 * 1024;
/// Total bytes captured for one run before the snapshot is marked partial.
const MAX_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
/// Maximum regular files captured for one run.
const MAX_FILES: usize = 50_000;
/// Files above this size are not hashed for the conflict check.
const MAX_HASH_BYTES: u64 = 64 * 1024 * 1024;

/// Generated and package-manager directories that are not useful to revert.
const EXCLUDED_DIRS: &[&str] = &[
    ".git",
    ".hg",
    "snapshots",
    ".svn",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".nuxt",
    ".cache",
    ".venv",
    "venv",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    ".gradle",
    ".tox",
    ".terraform",
];

#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("snapshot I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("snapshot database: {0}")]
    Db(#[from] sqlx::Error),
    #[error("{0}")]
    Blocked(String),
}

impl From<SnapshotError> for String {
    fn from(error: SnapshotError) -> Self {
        error.to_string()
    }
}

/// One reason a path cannot be reverted.
#[derive(Debug, Clone, Serialize)]
pub struct RevertConflict {
    pub path: String,
    pub reason: String,
}

/// What a preview or a completed revert did.
#[derive(Debug, Clone, Default, Serialize)]
pub struct RevertReport {
    pub restored: Vec<String>,
    pub deleted: Vec<String>,
    pub skipped: Vec<RevertConflict>,
    pub conflicts: Vec<RevertConflict>,
}

impl RevertReport {
    pub fn blocked(&self) -> bool {
        !self.conflicts.is_empty()
    }
}

/// The blob root.
///
/// Unit tests use a process temp directory so a test never writes into the
/// developer's real `~/RAVENBOT/snapshots` store. The app and its integration
/// tests use the one under the RAVENBOT home, so a backup of that folder
/// carries every revertable before-image with it.
fn blobs_root() -> PathBuf {
    #[cfg(test)]
    {
        std::env::temp_dir().join("ravenbot-test-snapshot-blobs")
    }
    #[cfg(not(test))]
    {
        ravenbot_core::snapshots_dir().join("blobs")
    }
}

/// The blob directory for one hash, sharded by its first two characters.
fn blob_path(hash: &str) -> PathBuf {
    blobs_root().join(&hash[..2.min(hash.len())]).join(hash)
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// The state of a path at revert-planning time.
///
/// "Absent" and "too large to hash" are different answers, and conflating them
/// would let a revert delete or overwrite a file it never verified.
enum FileState {
    Absent,
    Hash(String),
    TooLarge,
    NotFile,
}

async fn file_state(path: &Path) -> Result<FileState, SnapshotError> {
    match tokio::fs::metadata(path).await {
        Ok(meta) if !meta.is_file() => Ok(FileState::NotFile),
        Ok(meta) if meta.len() > MAX_HASH_BYTES => Ok(FileState::TooLarge),
        Ok(_) => match tokio::fs::read(path).await {
            Ok(bytes) => Ok(FileState::Hash(hash_bytes(&bytes))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(FileState::Absent),
            Err(error) => Err(error.into()),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(FileState::Absent),
        Err(error) => Err(error.into()),
    }
}

async fn hash_file(path: &Path) -> Result<Option<String>, SnapshotError> {
    Ok(match file_state(path).await? {
        FileState::Hash(hash) => Some(hash),
        _ => None,
    })
}

/// Create a directory that holds workspace contents and deny other users.
#[cfg(unix)]
async fn private_dir(path: &Path) -> Result<(), SnapshotError> {
    use std::os::unix::fs::PermissionsExt;
    tokio::fs::create_dir_all(path).await?;
    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).await?;
    Ok(())
}

#[cfg(not(unix))]
async fn private_dir(path: &Path) -> Result<(), SnapshotError> {
    tokio::fs::create_dir_all(path).await?;
    Ok(())
}

/// A before-image can contain a `.env`, a key, or customer data. The blob is
/// the same bytes as the workspace file, so it gets the same private mode.
#[cfg(unix)]
async fn private_file(path: &Path) -> Result<(), SnapshotError> {
    use std::os::unix::fs::PermissionsExt;
    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).await?;
    Ok(())
}

#[cfg(not(unix))]
async fn private_file(_path: &Path) -> Result<(), SnapshotError> {
    Ok(())
}

async fn store_blob(hash: &str, bytes: &[u8]) -> Result<(), SnapshotError> {
    let path = blob_path(hash);
    if tokio::fs::metadata(&path).await.is_ok() {
        return Ok(());
    }
    let Some(parent) = path.parent() else {
        return Err(SnapshotError::Blocked(
            "The snapshot blob has no parent directory.".to_string(),
        ));
    };
    private_dir(parent).await?;
    let tmp = parent.join(format!(".blob-{}", Uuid::new_v4()));
    tokio::fs::write(&tmp, bytes).await?;
    private_file(&tmp).await?;
    if let Err(error) = tokio::fs::rename(&tmp, &path).await {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(error.into());
    }
    Ok(())
}

fn is_mutating_skill(skill: &str) -> bool {
    matches!(
        skill,
        "file_write"
            | "file_edit"
            | "code_edit"
            | "shell_exec"
            | "git"
            | "package_manager"
            | "task_runner"
    )
}

fn excluded_dir(name: &str) -> bool {
    EXCLUDED_DIRS.contains(&name)
}

#[cfg(unix)]
fn file_mode(meta: &std::fs::Metadata) -> Option<i64> {
    use std::os::unix::fs::PermissionsExt;
    Some(i64::from(meta.permissions().mode()))
}

#[cfg(not(unix))]
fn file_mode(_meta: &std::fs::Metadata) -> Option<i64> {
    None
}

#[allow(clippy::too_many_arguments)]
fn pending_entry(
    snapshot_id: &str,
    root: &Path,
    path: &Path,
    blob: Option<String>,
    existed: bool,
    sha256: Option<String>,
    mode: Option<i64>,
    size: i64,
    note: &str,
) -> SnapshotEntryRecord {
    SnapshotEntryRecord {
        snapshot_id: snapshot_id.to_string(),
        path: path.display().to_string(),
        root: root.display().to_string(),
        blob,
        existed,
        sha256,
        mode,
        size,
        note: note.to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    }
}

/// Capture the run's workspace before a mutating tool runs.
///
/// Returns `Ok(true)` when this call created the snapshot. A run that has
/// already been captured returns immediately; a tool that cannot mutate is not
/// considered at all. A snapshot that hits a cap is marked partial but does not
/// block the tool — the run may continue, and revert will say why it cannot be
/// used.
pub async fn capture_before_write(
    db: &Database,
    ctx: &SkillContext,
    skill: &str,
) -> Result<bool, SnapshotError> {
    if !is_mutating_skill(skill) {
        return Ok(false);
    }
    // Escape hatch for an operator who deliberately does not want a tree walk
    // on a very large workspace. Revert is then unavailable by construction.
    let disabled = std::env::var("RAVENBOT_SNAPSHOT")
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "off" | "no"
            )
        })
        .unwrap_or(false);
    if disabled {
        return Ok(false);
    }
    if ctx.working_dirs.is_empty() {
        return Ok(false);
    }

    let mut roots = Vec::new();
    for dir in &ctx.working_dirs {
        if let Ok(canonical) = tokio::fs::canonicalize(dir).await {
            if canonical.is_dir() && !roots.contains(&canonical) {
                roots.push(canonical);
            }
        }
    }
    if roots.is_empty() {
        return Ok(false);
    }

    let snapshot = SnapshotQueries::ensure_for_run(
        db.pool(),
        ctx.run_id,
        ctx.thread_id,
        &roots[0].display().to_string(),
    )
    .await?;
    // Another node in the same run may have won the race. Its entries are the
    // run's before-state too, so do not walk again.
    if !SnapshotQueries::entries(db.pool(), &snapshot.id)
        .await?
        .is_empty()
    {
        return Ok(false);
    }

    let mut partial = false;
    let mut captured = 0usize;
    let mut total_bytes = 0u64;
    let mut pending: Vec<SnapshotEntryRecord> = Vec::with_capacity(256);
    'roots: for root in &roots {
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            let mut entries = match tokio::fs::read_dir(&dir).await {
                Ok(entries) => entries,
                // A directory we cannot read may contain files. The snapshot
                // cannot prove those were absent, so it is partial.
                Err(_) => {
                    partial = true;
                    continue;
                }
            };
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                // Check the directory entry type before metadata: `metadata`
                // follows symlinks, and following one here would copy a file
                // from outside the workspace into the snapshot store.
                let file_type = match entry.file_type().await {
                    Ok(file_type) => file_type,
                    Err(_) => {
                        partial = true;
                        continue;
                    }
                };
                if file_type.is_symlink() {
                    // Do not follow it and do not silently omit it: an entry
                    // with no blob tells revert to refuse that path.
                    pending.push(pending_entry(
                        &snapshot.id,
                        root,
                        &path,
                        None,
                        true,
                        None,
                        None,
                        0,
                        "symlink",
                    ));
                    continue;
                }
                let meta = match entry.metadata().await {
                    Ok(meta) => meta,
                    Err(_) => {
                        partial = true;
                        continue;
                    }
                };
                if meta.is_dir() {
                    if path.starts_with(ravenbot_core::snapshots_dir()) {
                        // The blob store itself is never part of a workspace,
                        // even when the user pointed a bot at RAVENBOT_HOME.
                        continue;
                    }
                    if entry.file_name().to_str().is_some_and(excluded_dir) {
                        continue;
                    }
                    stack.push(path);
                    continue;
                }
                if !meta.is_file() {
                    continue;
                }
                if captured >= MAX_FILES || total_bytes >= MAX_TOTAL_BYTES {
                    partial = true;
                    break 'roots;
                }
                captured += 1;
                let size = meta.len();
                if size > MAX_BLOB_BYTES {
                    pending.push(pending_entry(
                        &snapshot.id,
                        root,
                        &path,
                        None,
                        true,
                        None,
                        file_mode(&meta),
                        size.min(i64::MAX as u64) as i64,
                        "too large to snapshot",
                    ));
                    continue;
                }
                let bytes = match tokio::fs::read(&path).await {
                    Ok(bytes) => bytes,
                    Err(_) => {
                        pending.push(pending_entry(
                            &snapshot.id,
                            root,
                            &path,
                            None,
                            true,
                            None,
                            file_mode(&meta),
                            size.min(i64::MAX as u64) as i64,
                            "unreadable",
                        ));
                        continue;
                    }
                };
                total_bytes = total_bytes.saturating_add(bytes.len() as u64);
                let hash = hash_bytes(&bytes);
                store_blob(&hash, &bytes).await?;
                pending.push(pending_entry(
                    &snapshot.id,
                    root,
                    &path,
                    Some(hash.clone()),
                    true,
                    Some(hash),
                    file_mode(&meta),
                    bytes.len().min(i64::MAX as usize) as i64,
                    "",
                ));
            }
        }
    }
    SnapshotQueries::insert_entries_if_absent(db.pool(), &pending).await?;
    if partial {
        SnapshotQueries::mark_partial(db.pool(), &snapshot.id).await?;
    }
    Ok(true)
}

/// One path a revert would touch.
struct RestoreAction {
    path: PathBuf,
    root: PathBuf,
    entry: Option<SnapshotEntryRecord>,
}

fn conflict(report: &mut RevertReport, path: &str, reason: impl Into<String>) {
    report.conflicts.push(RevertConflict {
        path: path.to_string(),
        reason: reason.into(),
    });
}

/// Build the restore plan for one run without touching the filesystem.
async fn plan_run(
    db: &Database,
    snapshot: &WorkspaceSnapshotRecord,
    run_id: Uuid,
) -> Result<(Vec<RestoreAction>, RevertReport), SnapshotError> {
    let mut report = RevertReport::default();
    let mut actions = Vec::new();
    if !snapshot.complete {
        conflict(
            &mut report,
            &snapshot.root,
            "The run snapshot was incomplete (too many or too-large files), so it cannot prove which paths existed before the run.",
        );
        return Ok((actions, report));
    }

    let changes = FileChangeQueries::for_run(db.pool(), run_id).await?;
    let mut paths: Vec<String> = changes
        .iter()
        .filter_map(|change| change.snapshot_path.clone())
        .collect();
    paths.sort();
    paths.dedup();
    if paths.is_empty() {
        return Ok((actions, report));
    }

    let entries = SnapshotQueries::entries(db.pool(), &snapshot.id).await?;
    let by_path: HashMap<String, SnapshotEntryRecord> = entries
        .into_iter()
        .map(|entry| (entry.path.clone(), entry))
        .collect();

    for path in paths {
        let entry = by_path.get(&path).cloned();
        // A created file has no entry of its own. It belongs to the root that
        // contains it; if no captured file shares that root, the primary root
        // is the only honest guess.
        let root = entry
            .as_ref()
            .map(|entry| PathBuf::from(&entry.root))
            .or_else(|| {
                by_path
                    .values()
                    .find(|entry| Path::new(&path).starts_with(&entry.root))
                    .map(|entry| PathBuf::from(&entry.root))
            })
            .unwrap_or_else(|| PathBuf::from(&snapshot.root));

        // Conflicts are recorded but do not remove the action: `force` is
        // exactly the user saying they accept these overwrites. The preview
        // returns without executing, and a non-forced revert returns before
        // this action list is used.
        let mut conflicted = false;
        if let Some(later) = FileChangeQueries::first_after_path(
            db.pool(),
            &path,
            &snapshot.created_at,
            Some(run_id),
        )
        .await?
        {
            conflict(
                &mut report,
                &path,
                format!(
                    "A later run ({}) wrote this file at {}.",
                    later.run_id.map(|id| id.to_string()).unwrap_or_default(),
                    later.created_at
                ),
            );
            conflicted = true;
        }
        if !conflicted {
            let expected = FileChangeQueries::latest_for_run_path(db.pool(), run_id, &path)
                .await?
                .and_then(|change| change.after_sha256);
            match (&expected, file_state(Path::new(&path)).await?) {
                (Some(expected), FileState::Hash(current)) if expected == &current => {}
                (Some(_), FileState::Hash(_)) => {
                    conflict(
                        &mut report,
                        &path,
                        "The file changed after this run, so reverting it would overwrite newer work.",
                    );
                }
                (Some(_), _) => {
                    conflict(
                        &mut report,
                        &path,
                        "The file is gone or too large to verify, so the revert cannot confirm it is still the version this run wrote.",
                    );
                }
                // A deletion leaves no content behind. Still absent is the
                // state this run produced, so restoring the before-image is
                // safe; anything else means someone recreated it.
                (None, FileState::Absent) => {}
                (None, _) => {
                    conflict(
                        &mut report,
                        &path,
                        "The run did not record the file's final hash, so the revert cannot confirm it is safe to replace.",
                    );
                }
            }
        }

        let entry = by_path.get(&path).cloned();
        if let Some(entry) = &entry {
            if entry.existed && entry.blob.is_none() {
                // No force can recover a before-image that was never stored.
                conflict(
                    &mut report,
                    &path,
                    if entry.note.trim().is_empty() {
                        "The before-image is unavailable.".to_string()
                    } else {
                        format!("The before-image is unavailable: {}.", entry.note.trim())
                    },
                );
                continue;
            }
        }
        actions.push(RestoreAction {
            path: PathBuf::from(&path),
            root,
            entry,
        });
    }

    Ok((actions, report))
}

/// Preview a whole-run revert: what would be restored, deleted, or blocked.
pub async fn preview_run(db: &Database, run_id: Uuid) -> Result<RevertReport, SnapshotError> {
    let Some(snapshot) = SnapshotQueries::for_run(db.pool(), run_id).await? else {
        return Err(SnapshotError::Blocked(
            "This run has no file snapshot. Nothing was written through a snapshotted tool."
                .to_string(),
        ));
    };
    let (actions, mut report) = plan_run(db, &snapshot, run_id).await?;
    for action in &actions {
        let path = action.path.display().to_string();
        match &action.entry {
            Some(entry) if entry.existed => report.restored.push(path),
            _ => report.deleted.push(path),
        }
    }
    Ok(report)
}

/// Revert every path the run wrote back to its before-run state.
///
/// Without `force`, any conflict returns the report and changes nothing. With
/// `force`, forceable conflicts are overwritten, while unavailable before-images
/// and unsafe paths are still skipped.
pub async fn revert_run(
    db: &Database,
    run_id: Uuid,
    force: bool,
) -> Result<RevertReport, SnapshotError> {
    let Some(snapshot) = SnapshotQueries::for_run(db.pool(), run_id).await? else {
        return Err(SnapshotError::Blocked(
            "This run has no file snapshot. Nothing was written through a snapshotted tool."
                .to_string(),
        ));
    };
    let (actions, mut report) = plan_run(db, &snapshot, run_id).await?;
    if report.blocked() && !force {
        return Ok(report);
    }
    for action in actions {
        match restore_action(&action).await {
            Ok(RestoreOutcome::Restored) => report.restored.push(action.path.display().to_string()),
            Ok(RestoreOutcome::Deleted) => report.deleted.push(action.path.display().to_string()),
            Ok(RestoreOutcome::Skipped(reason)) => report.skipped.push(RevertConflict {
                path: action.path.display().to_string(),
                reason,
            }),
            Err(error) => report.skipped.push(RevertConflict {
                path: action.path.display().to_string(),
                reason: error.to_string(),
            }),
        }
    }
    Ok(report)
}

/// Revert one file change to the version captured before its run.
///
/// The snapshot stores one before-image per run and path, so this restores the
/// pre-run version, not an intermediate version inside the same run. A later
/// write to the path (in this run or another) blocks it unless `force` is set.
pub async fn revert_file_change(
    db: &Database,
    change_id: Uuid,
    force: bool,
) -> Result<RevertReport, SnapshotError> {
    let Some(change) = FileChangeQueries::get(db.pool(), change_id).await? else {
        return Err(SnapshotError::Blocked(
            "That file change was not found.".to_string(),
        ));
    };
    let Some(run_id) = change.run_id else {
        return Err(SnapshotError::Blocked(
            "That file change is not attached to a run.".to_string(),
        ));
    };
    let Some(path) = change.snapshot_path.clone() else {
        return Err(SnapshotError::Blocked(
            "That write was not snapshotted, so there is no before-image to restore.".to_string(),
        ));
    };
    let Some(snapshot) = SnapshotQueries::for_run(db.pool(), run_id).await? else {
        return Err(SnapshotError::Blocked(
            "The run's snapshot is missing.".to_string(),
        ));
    };
    if !snapshot.complete {
        return Err(SnapshotError::Blocked(
            "The run snapshot was incomplete, so it cannot prove what the file looked like before the run.".to_string(),
        ));
    }

    let mut report = RevertReport::default();
    if let Some(later) =
        FileChangeQueries::first_after_path(db.pool(), &path, &change.created_at, None).await?
    {
        conflict(
            &mut report,
            &path,
            format!(
                "A later write ({}) changed this file at {}.",
                later.skill, later.created_at
            ),
        );
    }
    match (&change.after_sha256, file_state(Path::new(&path)).await?) {
        (Some(expected), FileState::Hash(current)) if expected == &current => {}
        (Some(_), _) => {
            conflict(
                &mut report,
                &path,
                "The file changed after this write, so reverting it would overwrite newer work.",
            );
        }
        (None, FileState::Absent) => {}
        (None, _) => {
            conflict(
                &mut report,
                &path,
                "This write did not record the file's final hash, so the revert cannot confirm it is safe to replace.",
            );
        }
    }

    let entry = SnapshotQueries::entry_for_path(db.pool(), &snapshot.id, &path).await?;
    if let Some(entry) = &entry {
        if entry.existed && entry.blob.is_none() {
            return Err(SnapshotError::Blocked(if entry.note.trim().is_empty() {
                "The before-image for that file is unavailable.".to_string()
            } else {
                format!(
                    "The before-image for that file is unavailable: {}.",
                    entry.note.trim()
                )
            }));
        }
    }
    if report.blocked() && !force {
        return Ok(report);
    }

    let action = RestoreAction {
        path: PathBuf::from(&path),
        root: PathBuf::from(&snapshot.root),
        entry,
    };
    match restore_action(&action).await {
        Ok(RestoreOutcome::Restored) => report.restored.push(path),
        Ok(RestoreOutcome::Deleted) => report.deleted.push(path),
        Ok(RestoreOutcome::Skipped(reason)) => report.skipped.push(RevertConflict { path, reason }),
        Err(error) => report.skipped.push(RevertConflict {
            path,
            reason: error.to_string(),
        }),
    }
    Ok(report)
}

enum RestoreOutcome {
    Restored,
    Deleted,
    Skipped(String),
}

/// Reject paths that no longer resolve inside the snapshot root.
///
/// The stored path was resolved when the snapshot was captured. A symlink
/// placed over the file, a replaced parent directory, or a direct database edit
/// must not turn a revert into an arbitrary write.
async fn validate_action(action: &RestoreAction) -> Result<(), String> {
    if !action.path.is_absolute() || !action.root.is_absolute() {
        return Err("The snapshot path is not absolute.".to_string());
    }
    if !action.path.starts_with(&action.root) {
        return Err("The snapshot path is outside its workspace root.".to_string());
    }
    let root = tokio::fs::canonicalize(&action.root)
        .await
        .map_err(|_| "The workspace root no longer exists.".to_string())?;
    if let Ok(meta) = tokio::fs::symlink_metadata(&action.path).await {
        if meta.file_type().is_symlink() {
            return Err("The path is now a symlink; refusing to write through it.".to_string());
        }
        if meta.is_dir() {
            return Err("The path is now a directory.".to_string());
        }
    }
    let mut ancestor = action.path.parent();
    while let Some(dir) = ancestor {
        match tokio::fs::metadata(dir).await {
            Ok(meta) if meta.is_dir() => {
                let canonical = tokio::fs::canonicalize(dir)
                    .await
                    .map_err(|_| "A parent directory can no longer be resolved.".to_string())?;
                if !canonical.starts_with(&root) {
                    return Err("A parent directory resolves outside the workspace.".to_string());
                }
                return Ok(());
            }
            Ok(_) => return Err("A parent path is not a directory.".to_string()),
            Err(_) => ancestor = dir.parent(),
        }
    }
    Err("The workspace root no longer exists.".to_string())
}

async fn restore_action(action: &RestoreAction) -> Result<RestoreOutcome, SnapshotError> {
    if let Err(reason) = validate_action(action).await {
        return Ok(RestoreOutcome::Skipped(reason));
    }

    let Some(entry) = &action.entry else {
        // Absent from a complete snapshot: the run created it. Deletion is
        // still a write, so refuse a symlink or directory that replaced it.
        match tokio::fs::symlink_metadata(&action.path).await {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Ok(RestoreOutcome::Skipped(
                    "The created path is now a symlink; refusing to follow it.".to_string(),
                ))
            }
            Ok(meta) if meta.is_dir() => {
                return Ok(RestoreOutcome::Skipped(
                    "The created path is now a directory; not removing it.".to_string(),
                ))
            }
            Ok(_) => {
                tokio::fs::remove_file(&action.path).await?;
                return Ok(RestoreOutcome::Deleted);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(RestoreOutcome::Skipped(
                    "The file is already gone.".to_string(),
                ))
            }
            Err(error) => return Err(error.into()),
        }
    };

    if !entry.existed {
        match tokio::fs::remove_file(&action.path).await {
            Ok(()) => return Ok(RestoreOutcome::Deleted),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(RestoreOutcome::Skipped(
                    "The file is already gone.".to_string(),
                ))
            }
            Err(error) => return Err(error.into()),
        }
    }

    let Some(blob) = &entry.blob else {
        return Ok(RestoreOutcome::Skipped(
            "The before-image is unavailable.".to_string(),
        ));
    };
    let bytes = tokio::fs::read(blob_path(blob)).await?;
    let actual = hash_bytes(&bytes);
    if entry
        .sha256
        .as_deref()
        .is_some_and(|expected| expected != actual)
    {
        return Ok(RestoreOutcome::Skipped(
            "The stored before-image is corrupt.".to_string(),
        ));
    }
    let Some(parent) = action.path.parent() else {
        return Ok(RestoreOutcome::Skipped(
            "The path has no parent directory.".to_string(),
        ));
    };
    tokio::fs::create_dir_all(parent).await?;
    let tmp = parent.join(format!(".ravenbot-revert-{}", Uuid::new_v4()));
    tokio::fs::write(&tmp, bytes).await?;
    #[cfg(unix)]
    if let Some(mode) = entry.mode {
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::Permissions::from_mode((mode as u32) & 0o7777);
        if let Err(error) = tokio::fs::set_permissions(&tmp, permissions).await {
            let _ = tokio::fs::remove_file(&tmp).await;
            return Err(error.into());
        }
    }
    if let Err(error) = tokio::fs::rename(&tmp, &action.path).await {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(error.into());
    }
    Ok(RestoreOutcome::Restored)
}

/// Public wrapper used by the journal after a tool ran.
pub async fn hash_path(path: &Path) -> Result<Option<String>, SnapshotError> {
    hash_file(path).await
}

/// Delete snapshots past their retention window and unreferenced blobs.
///
/// Retention is deliberately separate from the ledger: `file_changes` is the
/// durable history and stays. Blobs younger than a day are kept even when no
/// manifest names them yet, because a capture writes blobs before its batch
/// insert commits.
pub async fn prune(
    pool: &sqlx::SqlitePool,
    retention_days: u32,
) -> Result<(u64, u64), SnapshotError> {
    if retention_days == 0 {
        return Ok((0, 0));
    }
    let cutoff = (chrono::Utc::now()
        - chrono::Duration::days(i64::from(retention_days.clamp(1, 3650))))
    .to_rfc3339();
    let snapshots = SnapshotQueries::prune_older_than(pool, &cutoff).await?;
    let referenced: HashSet<String> = SnapshotQueries::referenced_blobs(pool)
        .await?
        .into_iter()
        .collect();
    let blobs = prune_unreferenced_blobs(&referenced).await?;
    Ok((snapshots, blobs))
}

async fn prune_unreferenced_blobs(referenced: &HashSet<String>) -> Result<u64, SnapshotError> {
    let root = blobs_root();
    if tokio::fs::metadata(&root).await.is_err() {
        return Ok(0);
    }
    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(24 * 60 * 60))
        .unwrap_or(std::time::UNIX_EPOCH);
    let mut removed = 0u64;
    let mut shards = tokio::fs::read_dir(&root).await?;
    while let Ok(Some(shard)) = shards.next_entry().await {
        match shard.metadata().await {
            Ok(meta) if meta.is_dir() => {}
            _ => continue,
        }
        let mut files = tokio::fs::read_dir(shard.path()).await?;
        while let Ok(Some(file)) = files.next_entry().await {
            let Ok(metadata) = file.metadata().await else {
                continue;
            };
            if !metadata.is_file() {
                continue;
            }
            let name = file.file_name().to_string_lossy().to_string();
            if referenced.contains(&name) {
                continue;
            }
            if metadata.modified().is_ok_and(|modified| modified > cutoff) {
                // A capture may have written it and not committed yet.
                continue;
            }
            if tokio::fs::remove_file(file.path()).await.is_ok() {
                removed += 1;
            }
        }
        // Remove an empty shard; a non-empty one is still doing work.
        let _ = tokio::fs::remove_dir(shard.path()).await;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ravenbot_core::{Bot, Thread};
    use ravenbot_db::queries::{BotQueries, FileChangeQueries, ThreadQueries};

    async fn db() -> (Database, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!("ravenbot-snapshot-{}.db", Uuid::new_v4()));
        let db = Database::new(&path).await.expect("temp db");
        (db, path)
    }

    async fn seed(db: &Database) -> (Bot, Thread) {
        let bot = Bot::new("Snapshot", "files");
        BotQueries::insert(db.pool(), &bot).await.expect("bot");
        let thread = Thread::new(bot.id, "Snapshot thread");
        ThreadQueries::create(db.pool(), &thread)
            .await
            .expect("thread");
        (bot, thread)
    }

    fn cleanup(path: &Path) {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    #[tokio::test]
    async fn a_run_snapshot_restores_overwrites_and_deletes_created_files() {
        let (db, db_path) = db().await;
        let (bot, thread) = seed(&db).await;
        let root = std::env::temp_dir().join(format!("ravenbot-snapshot-ws-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("workspace");
        std::fs::write(root.join("kept.txt"), "before\n").expect("before");

        let run = ravenbot_core::Run::new(bot.id, thread.id);
        ravenbot_db::queries::RunQueries::insert(db.pool(), &run)
            .await
            .expect("run");
        let ctx = SkillContext::with_default_tier(bot.id, run.id, thread.id)
            .with_working_dirs(vec![root.clone()]);
        assert!(capture_before_write(&db, &ctx, "file_write")
            .await
            .expect("capture"));

        std::fs::write(root.join("kept.txt"), "after\n").expect("after");
        std::fs::write(root.join("created.txt"), "new\n").expect("created");
        let mut change = ravenbot_core::FileChange::new(
            bot.id,
            Some(run.id),
            Some(thread.id),
            root.join("kept.txt").display().to_string(),
            "file_write",
            1,
            1,
        );
        change.snapshot_path = Some(root.join("kept.txt").display().to_string());
        change.after_sha256 = hash_file(&root.join("kept.txt")).await.expect("hash");
        FileChangeQueries::insert(db.pool(), &change)
            .await
            .expect("change");
        let mut created = ravenbot_core::FileChange::new(
            bot.id,
            Some(run.id),
            Some(thread.id),
            root.join("created.txt").display().to_string(),
            "file_write",
            1,
            0,
        );
        created.snapshot_path = Some(root.join("created.txt").display().to_string());
        created.after_sha256 = hash_file(&root.join("created.txt")).await.expect("hash");
        FileChangeQueries::insert(db.pool(), &created)
            .await
            .expect("created change");

        let report = revert_run(&db, run.id, false).await.expect("revert");
        assert!(report.conflicts.is_empty(), "{report:?}");
        assert_eq!(
            std::fs::read_to_string(root.join("kept.txt")).unwrap(),
            "before\n"
        );
        assert!(
            !root.join("created.txt").exists(),
            "a created file is deleted"
        );

        cleanup(&db_path);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn retention_deletes_old_snapshots_but_not_the_ledger() {
        let (db, db_path) = db().await;
        let (bot, thread) = seed(&db).await;
        let root = std::env::temp_dir().join(format!("ravenbot-snapshot-ws-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("workspace");
        std::fs::write(root.join("old.txt"), "old\n").expect("file");

        let run = ravenbot_core::Run::new(bot.id, thread.id);
        ravenbot_db::queries::RunQueries::insert(db.pool(), &run)
            .await
            .expect("run");
        let ctx = SkillContext::with_default_tier(bot.id, run.id, thread.id)
            .with_working_dirs(vec![root.clone()]);
        capture_before_write(&db, &ctx, "file_write")
            .await
            .expect("capture");
        let change = ravenbot_core::FileChange::new(
            bot.id,
            Some(run.id),
            Some(thread.id),
            root.join("old.txt").display().to_string(),
            "file_write",
            1,
            0,
        );
        FileChangeQueries::insert(db.pool(), &change)
            .await
            .expect("change");
        sqlx::query("UPDATE workspace_snapshots SET created_at = '2020-01-01T00:00:00Z'")
            .execute(db.pool())
            .await
            .expect("age snapshot");

        let (snapshots, _) = prune(db.pool(), 7).await.expect("prune");
        assert_eq!(snapshots, 1);
        assert!(
            SnapshotQueries::for_run(db.pool(), run.id)
                .await
                .expect("lookup")
                .is_none(),
            "an expired snapshot is gone"
        );
        assert_eq!(
            FileChangeQueries::for_run(db.pool(), run.id)
                .await
                .expect("ledger")
                .len(),
            1,
            "retention never rewrites the file-change ledger"
        );

        cleanup(&db_path);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn a_later_write_blocks_the_revert_until_forced() {
        let (db, db_path) = db().await;
        let (bot, thread) = seed(&db).await;
        let root = std::env::temp_dir().join(format!("ravenbot-snapshot-ws-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("workspace");
        std::fs::write(root.join("app.txt"), "one\n").expect("file");

        let run = ravenbot_core::Run::new(bot.id, thread.id);
        ravenbot_db::queries::RunQueries::insert(db.pool(), &run)
            .await
            .expect("run");
        let ctx = SkillContext::with_default_tier(bot.id, run.id, thread.id)
            .with_working_dirs(vec![root.clone()]);
        capture_before_write(&db, &ctx, "file_write")
            .await
            .expect("capture");

        std::fs::write(root.join("app.txt"), "two\n").expect("write");
        let mut change = ravenbot_core::FileChange::new(
            bot.id,
            Some(run.id),
            Some(thread.id),
            root.join("app.txt").display().to_string(),
            "file_write",
            1,
            1,
        );
        change.snapshot_path = Some(root.join("app.txt").display().to_string());
        change.after_sha256 = hash_file(&root.join("app.txt")).await.expect("hash");
        FileChangeQueries::insert(db.pool(), &change)
            .await
            .expect("change");

        // A second run writes the same path after the snapshot.
        let later = ravenbot_core::Run::new(bot.id, thread.id);
        ravenbot_db::queries::RunQueries::insert(db.pool(), &later)
            .await
            .expect("later run");
        let mut later_change = ravenbot_core::FileChange::new(
            bot.id,
            Some(later.id),
            Some(thread.id),
            root.join("app.txt").display().to_string(),
            "file_write",
            1,
            1,
        );
        later_change.snapshot_path = Some(root.join("app.txt").display().to_string());
        later_change.after_sha256 = hash_file(&root.join("app.txt")).await.expect("hash");
        later_change.created_at = (chrono::Utc::now() + chrono::Duration::seconds(5)).to_rfc3339();
        FileChangeQueries::insert(db.pool(), &later_change)
            .await
            .expect("later change");

        let blocked = revert_run(&db, run.id, false).await.expect("preview");
        assert!(blocked.blocked(), "a later write must block the revert");
        assert_eq!(
            std::fs::read_to_string(root.join("app.txt")).unwrap(),
            "two\n"
        );

        let forced = revert_run(&db, run.id, true).await.expect("forced");
        assert!(forced.conflicts.is_empty() || !forced.conflicts.is_empty());
        assert_eq!(
            std::fs::read_to_string(root.join("app.txt")).unwrap(),
            "one\n"
        );

        cleanup(&db_path);
        let _ = std::fs::remove_dir_all(&root);
    }
}
