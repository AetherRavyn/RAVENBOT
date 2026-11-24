//! Opt-in, conversation-scoped Git worktree isolation.
//!
//! A user can mark one 1:1 conversation as isolated. The first run after that
//! gets a real `git worktree` checkout under `<RAVENBOT_HOME>/worktrees`; every
//! later run in the same conversation reuses it. The main checkout is not
//! touched until the user reviews the worktree diff and chooses Apply.
//!
//! The rules are deliberately strict:
//!
//! - The project must be a Git repository with **no** uncommitted or untracked
//!   changes when the worktree is created. There is no honest way to seed a
//!   worktree from a dirty checkout without inventing a commit the user did not
//!   ask for.
//! - One worktree per conversation. Goal rounds and ordinary messages share it,
//!   so a multi-round goal sees its previous work.
//! - Apply is a real `git apply` against the original checkout. If the main
//!   checkout changed in a conflicting way, the apply is refused and the
//!   worktree stays available for review or discard.
//! - Git-ignored files are copied into the checkout when small and not a
//!   symlink, so local configuration is readable. They are still not part of
//!   `git diff`: an ignored file that an agent writes stays in the worktree and
//!   is not applied automatically.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use ravenbot_core::Run;
use ravenbot_db::queries::{
    AppSettingsQueries, RunProgressQueries, ThreadWorktreeQueries, ThreadWorktreeRecord,
};
use ravenbot_db::Database;
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use uuid::Uuid;

/// A review is a UI payload. Stop reading after this many bytes and say so;
/// the actual apply streams the complete diff locally.
const MAX_REVIEW_DIFF: usize = 2 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum WorktreeError {
    #[error("worktree I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("worktree database: {0}")]
    Db(#[from] sqlx::Error),
    #[error("{0}")]
    Git(String),
    #[error("{0}")]
    Blocked(String),
}

impl From<WorktreeError> for String {
    fn from(error: WorktreeError) -> Self {
        error.to_string()
    }
}

fn setting_key(thread_id: Uuid) -> String {
    format!("worktree_isolation:{thread_id}")
}

fn worktrees_root() -> PathBuf {
    #[cfg(test)]
    {
        std::env::temp_dir().join("ravenbot-test-worktrees")
    }
    #[cfg(not(test))]
    {
        ravenbot_core::worktrees_dir()
    }
}

/// Whether this conversation asks for isolated runs.
pub async fn isolation_enabled(
    pool: &sqlx::SqlitePool,
    thread_id: Uuid,
) -> Result<bool, WorktreeError> {
    Ok(AppSettingsQueries::get(pool, &setting_key(thread_id))
        .await?
        .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true")))
}

/// Turn isolation on or off for future runs in one conversation.
///
/// Turning it off does not touch an active checkout; the review/discard actions
/// remain available so unapplied work is never silently thrown away.
pub async fn set_isolation(
    pool: &sqlx::SqlitePool,
    thread_id: Uuid,
    enabled: bool,
) -> Result<bool, WorktreeError> {
    AppSettingsQueries::set(
        pool,
        &setting_key(thread_id),
        if enabled { "1" } else { "0" },
    )
    .await?;
    Ok(enabled)
}

/// The conversation's visible isolation state.
#[derive(Debug, Clone, Serialize)]
pub struct WorktreeStatus {
    pub enabled: bool,
    pub active: Option<ThreadWorktreeRecord>,
    pub latest: Option<ThreadWorktreeRecord>,
}

pub async fn status(
    pool: &sqlx::SqlitePool,
    thread_id: Uuid,
) -> Result<WorktreeStatus, WorktreeError> {
    Ok(WorktreeStatus {
        enabled: isolation_enabled(pool, thread_id).await?,
        active: ThreadWorktreeQueries::active_for_thread(pool, thread_id).await?,
        latest: ThreadWorktreeQueries::latest_for_thread(pool, thread_id).await?,
    })
}

/// A review or a completed apply/discard.
#[derive(Debug, Clone, Serialize)]
pub struct WorktreeReview {
    pub thread_id: String,
    pub original_root: String,
    pub worktree_path: String,
    pub status: String,
    pub note: String,
    /// `status\tpath` lines from `git diff --cached --name-status`.
    pub files: Vec<String>,
    pub diff: String,
    pub truncated: bool,
}

fn review_from(record: &ThreadWorktreeRecord) -> WorktreeReview {
    WorktreeReview {
        thread_id: record.thread_id.clone(),
        original_root: record.original_root.clone(),
        worktree_path: record.worktree_path.clone(),
        status: record.status.clone(),
        note: record.note.clone(),
        files: Vec::new(),
        diff: String::new(),
        truncated: false,
    }
}

async fn git_output(
    dir: &Path,
    args: &[&str],
    input: Option<&[u8]>,
) -> Result<std::process::Output, WorktreeError> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(|error| {
        WorktreeError::Blocked(format!(
            "Git is required for worktree isolation and could not be started: {error}"
        ))
    })?;
    if let Some(input) = input {
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(input).await?;
        }
    }
    child.wait_with_output().await.map_err(Into::into)
}

async fn git_ok(dir: &Path, args: &[&str]) -> Result<String, WorktreeError> {
    let output = git_output(dir, args, None).await?;
    if !output.status.success() {
        return Err(WorktreeError::Git(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Read at most `limit` bytes of a Git command's stdout.
///
/// A review must not load an arbitrarily large diff just to show a preview.
/// Killing the child once the budget is exhausted is safe: the diff is only a
/// display payload, and Apply regenerates the complete one locally.
async fn git_bounded(
    dir: &Path,
    args: &[&str],
    limit: usize,
) -> Result<(String, bool), WorktreeError> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(|error| {
        WorktreeError::Blocked(format!(
            "Git is required for worktree isolation and could not be started: {error}"
        ))
    })?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| WorktreeError::Blocked("Could not read the worktree diff.".to_string()))?;
    let mut buffer = Vec::with_capacity(limit.min(64 * 1024));
    let mut chunk = [0u8; 8192];
    let mut truncated = false;
    loop {
        let read = stdout.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        if buffer.len() + read > limit {
            let room = limit.saturating_sub(buffer.len());
            buffer.extend_from_slice(&chunk[..room]);
            truncated = true;
            let _ = child.kill().await;
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    let output = child.wait_with_output().await?;
    if !output.status.success() && !truncated {
        return Err(WorktreeError::Git(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok((String::from_utf8_lossy(&buffer).to_string(), truncated))
}

/// Ensure the conversation has an isolated checkout and return the directory
/// the run should use.
///
/// A no-op when isolation is off. A failure is fail-closed: the run does not
/// proceed against the main checkout while claiming to be isolated.
pub async fn maybe_isolate(
    db: &Database,
    run: &Run,
    dirs: &[PathBuf],
) -> Result<Vec<PathBuf>, WorktreeError> {
    if dirs.is_empty() || !isolation_enabled(db.pool(), run.thread_id).await? {
        return Ok(dirs.to_vec());
    }
    if let Some(record) = ThreadWorktreeQueries::active_for_thread(db.pool(), run.thread_id).await?
    {
        let path = PathBuf::from(if record.agent_path.trim().is_empty() {
            &record.worktree_path
        } else {
            &record.agent_path
        });
        if !path.is_dir() {
            return Err(WorktreeError::Blocked(
                "The isolated checkout for this conversation is missing. Discard it from the conversation, then run again."
                    .to_string(),
            ));
        }
        if record.last_run_id.as_deref() != Some(run.id.to_string().as_str()) {
            ThreadWorktreeQueries::touch_run(db.pool(), run.thread_id, run.id).await?;
        }
        return Ok(vec![path]);
    }
    if dirs.len() != 1 {
        return Err(WorktreeError::Blocked(
            "Worktree isolation supports exactly one project folder per conversation.".to_string(),
        ));
    }

    let requested = tokio::fs::canonicalize(&dirs[0]).await.map_err(|_| {
        WorktreeError::Blocked(format!(
            "The project folder does not exist: {}",
            dirs[0].display()
        ))
    })?;
    let root_text = match git_ok(&requested, &["rev-parse", "--show-toplevel"]).await {
        Ok(text) => text,
        Err(WorktreeError::Git(_)) => {
            return Err(WorktreeError::Blocked(
                "Worktree isolation requires a Git repository. Turn isolation off or initialize the project with Git."
                    .to_string(),
            ))
        }
        Err(error) => return Err(error),
    };
    let root = tokio::fs::canonicalize(root_text.trim())
        .await
        .map_err(|_| {
            WorktreeError::Blocked("The project is not a readable Git repository.".to_string())
        })?;
    let clean = git_ok(
        &root,
        &["status", "--porcelain", "--untracked-files=normal"],
    )
    .await?;
    if !clean.trim().is_empty() {
        return Err(WorktreeError::Blocked(
            "Worktree isolation requires a clean Git repository. Commit or stash the existing changes, or turn isolation off."
                .to_string(),
        ));
    }
    let head = git_ok(&root, &["rev-parse", "HEAD"]).await?;
    let path = worktrees_root().join(run.thread_id.to_string());
    if path.starts_with(&root) {
        return Err(WorktreeError::Blocked(
            "The project root contains RAVENBOT's worktree directory. Use a project folder outside the RAVENBOT home."
                .to_string(),
        ));
    }
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ =
                tokio::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)).await;
        }
    }
    if path.exists() {
        let path_text = path.display().to_string();
        let _ = git_ok(&root, &["worktree", "remove", "--force", &path_text]).await;
        if path.exists() {
            return Err(WorktreeError::Blocked(format!(
                "An old isolated checkout exists at {} and is not a registered Git worktree. Remove it manually, then run again.",
                path.display()
            )));
        }
    }
    let path_text = path.display().to_string();
    // If the configured folder was a subdirectory of the repository, keep the
    // agent confined to the same relative place inside the checkout.
    let agent_path = match requested.strip_prefix(&root) {
        Ok(relative) if !relative.as_os_str().is_empty() => path.join(relative),
        _ => path.clone(),
    };
    git_ok(&root, &["worktree", "add", "--detach", &path_text, "HEAD"]).await?;
    copy_ignored_files(&root, &path).await;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).await;
    }
    if agent_path != path {
        tokio::fs::create_dir_all(&agent_path).await?;
    }
    if let Err(error) = ThreadWorktreeQueries::activate(
        db.pool(),
        run.thread_id,
        &root.display().to_string(),
        &path_text,
        &agent_path.display().to_string(),
        head.trim(),
        run.id,
    )
    .await
    {
        let _ = git_ok(&root, &["worktree", "remove", "--force", &path_text]).await;
        return Err(error.into());
    }
    Ok(vec![agent_path])
}

async fn ensure_idle(pool: &sqlx::SqlitePool, thread_id: Uuid) -> Result<(), WorktreeError> {
    if RunProgressQueries::has_active_for_thread(pool, thread_id)
        .await
        .unwrap_or(true)
    {
        return Err(WorktreeError::Blocked(
            "A run is still using this checkout. Wait for it to finish or pause the goal, then review, apply, or discard."
                .to_string(),
        ));
    }
    Ok(())
}

/// The commit the checkout is compared against.
///
/// Using the recorded base rather than `HEAD` means an agent that commits
/// inside the worktree still produces a reviewable/applicable diff instead of
/// an empty one.
fn diff_base(record: &ThreadWorktreeRecord) -> String {
    if record.base_commit.trim().is_empty() {
        "HEAD".to_string()
    } else {
        record.base_commit.clone()
    }
}

/// Copy small, non-generated ignored files so local configuration is readable.
///
/// A Git checkout contains tracked files only. Refusing every repository with a
/// `.env` would make isolation useless, so small ignored files are copied into
/// the checkout for reading. They remain ignored for Apply: the review and the
/// apply patch are about tracked/untracked source changes, not secrets.
async fn copy_ignored_files(root: &Path, worktree: &Path) {
    let Ok(list) = git_ok(
        root,
        &[
            "ls-files",
            "--others",
            "--ignored",
            "--exclude-standard",
            "--directory",
            "-z",
        ],
    )
    .await
    else {
        return;
    };
    for raw in list.split('\0') {
        if raw.is_empty() || raw.ends_with('/') {
            continue;
        }
        let relative = Path::new(raw);
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            continue;
        }
        let source = root.join(relative);
        let Ok(meta) = tokio::fs::symlink_metadata(&source).await else {
            continue;
        };
        if meta.file_type().is_symlink() || !meta.is_file() || meta.len() > 2 * 1024 * 1024 {
            continue;
        }
        let destination = worktree.join(relative);
        if destination.exists() {
            continue;
        }
        if let Some(parent) = destination.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        if tokio::fs::copy(&source, &destination).await.is_err() {
            tracing::warn!(
                path = %source.display(),
                "could not copy an ignored file into the isolated checkout"
            );
        }
    }
}

/// Stage everything in the worktree and produce a display review.
pub async fn review(
    pool: &sqlx::SqlitePool,
    thread_id: Uuid,
) -> Result<WorktreeReview, WorktreeError> {
    let Some(record) = ThreadWorktreeQueries::latest_for_thread(pool, thread_id).await? else {
        return Err(WorktreeError::Blocked(
            "This conversation has no isolated worktree.".to_string(),
        ));
    };
    let mut review = review_from(&record);
    if record.status != "active" {
        if review.note.trim().is_empty() {
            review.note = format!("This worktree was {}.", record.status);
        }
        return Ok(review);
    }
    ensure_idle(pool, thread_id).await?;
    let path = Path::new(&record.worktree_path);
    if !path.is_dir() {
        return Err(WorktreeError::Blocked(
            "The isolated checkout is missing. Discard it from the conversation, then run again."
                .to_string(),
        ));
    }
    let base = diff_base(&record);
    git_ok(path, &["add", "-A"]).await?;
    review.files = git_ok(
        path,
        &["diff", "--cached", "--name-status", &base, "--no-ext-diff"],
    )
    .await?
    .lines()
    .map(str::to_string)
    .collect();
    let (diff, truncated) = git_bounded(
        path,
        &[
            "diff",
            "--cached",
            "--binary",
            &base,
            "--no-ext-diff",
            "--no-color",
        ],
        MAX_REVIEW_DIFF,
    )
    .await?;
    review.diff = diff;
    review.truncated = truncated;
    Ok(review)
}

/// Apply the active worktree to the original checkout, then remove it.
pub async fn apply(db: &Database, thread_id: Uuid) -> Result<WorktreeReview, WorktreeError> {
    let Some(record) = ThreadWorktreeQueries::active_for_thread(db.pool(), thread_id).await? else {
        return Err(WorktreeError::Blocked(
            "This conversation has no active isolated worktree.".to_string(),
        ));
    };
    ensure_idle(db.pool(), thread_id).await?;
    let worktree = Path::new(&record.worktree_path);
    if !worktree.is_dir() {
        return Err(WorktreeError::Blocked(
            "The isolated checkout is missing. Discard it instead of applying.".to_string(),
        ));
    }
    let original = Path::new(&record.original_root);
    let base = diff_base(&record);
    git_ok(worktree, &["add", "-A"]).await?;
    let diff = git_ok(
        worktree,
        &[
            "diff",
            "--cached",
            "--binary",
            &base,
            "--no-ext-diff",
            "--no-color",
        ],
    )
    .await?;

    let no_changes = diff.trim().is_empty();
    if !no_changes {
        let check = git_output(
            original,
            &["apply", "--binary", "--check", "-"],
            Some(diff.as_bytes()),
        )
        .await?;
        if !check.status.success() {
            return Err(WorktreeError::Git(format!(
                "The main checkout could not accept the worktree patch: {}",
                String::from_utf8_lossy(&check.stderr).trim()
            )));
        }
        let applied =
            git_output(original, &["apply", "--binary", "-"], Some(diff.as_bytes())).await?;
        if !applied.status.success() {
            return Err(WorktreeError::Git(format!(
                "The worktree patch passed its check but could not be applied: {}",
                String::from_utf8_lossy(&applied.stderr).trim()
            )));
        }
    }
    let mut note = if no_changes {
        "The isolated worktree had no changes to apply.".to_string()
    } else {
        "Changes applied to the project.".to_string()
    };

    if let Err(error) = git_ok(
        original,
        &["worktree", "remove", "--force", &record.worktree_path],
    )
    .await
    {
        let _ = git_ok(original, &["worktree", "prune"]).await;
        note.push_str(&format!(
            " The checkout at {} could not be removed: {error}",
            record.worktree_path
        ));
    }
    ThreadWorktreeQueries::finish(db.pool(), thread_id, "applied", &note).await?;
    review(db.pool(), thread_id).await
}

/// Discard the active worktree without changing the project.
pub async fn discard(db: &Database, thread_id: Uuid) -> Result<WorktreeReview, WorktreeError> {
    let Some(record) = ThreadWorktreeQueries::active_for_thread(db.pool(), thread_id).await? else {
        return Err(WorktreeError::Blocked(
            "This conversation has no active isolated worktree.".to_string(),
        ));
    };
    ensure_idle(db.pool(), thread_id).await?;
    let original = Path::new(&record.original_root);
    let mut note = "The isolated worktree was discarded without changing the project.".to_string();
    if Path::new(&record.worktree_path).exists() {
        if let Err(error) = git_ok(
            original,
            &["worktree", "remove", "--force", &record.worktree_path],
        )
        .await
        {
            let _ = git_ok(original, &["worktree", "prune"]).await;
            if Path::new(&record.worktree_path).exists() {
                return Err(WorktreeError::Git(format!(
                    "The isolated checkout could not be removed: {error}"
                )));
            }
            note.push_str(" The checkout was already missing; its Git metadata was pruned.");
        }
    } else {
        let _ = git_ok(original, &["worktree", "prune"]).await;
        note.push_str(" The checkout was already missing; its Git metadata was pruned.");
    }
    ThreadWorktreeQueries::finish(db.pool(), thread_id, "discarded", &note).await?;
    review(db.pool(), thread_id).await
}

#[cfg(test)]
pub(crate) fn temporary_repo() -> Option<PathBuf> {
    use std::process::Command as StdCommand;
    if StdCommand::new("git").arg("--version").output().is_err() {
        return None;
    }
    let dir = std::env::temp_dir().join(format!("ravenbot-worktree-repo-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).ok()?;
    let git = |args: &[&str]| {
        let status = StdCommand::new("git")
            .arg("-C")
            .arg(&dir)
            .args(args)
            .status()
            .ok()?;
        status.success().then_some(())
    };
    git(&["init", "-q"])?;
    git(&["config", "user.email", "worktree@example.invalid"])?;
    git(&["config", "user.name", "Worktree Test"])?;
    std::fs::write(dir.join("note.txt"), "one\n").ok()?;
    git(&["add", "note.txt"])?;
    git(&["commit", "-qm", "initial"])?;
    Some(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ravenbot_db::queries::AppSettingsQueries;
    use std::process::Command as StdCommand;

    async fn temp_db() -> (Database, PathBuf) {
        let path = std::env::temp_dir().join(format!("ravenbot-worktree-{}.db", Uuid::new_v4()));
        let db = Database::new(&path).await.expect("temp db");
        (db, path)
    }

    fn git(dir: &Path, args: &[&str]) {
        let status = StdCommand::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .status()
            .expect("run git");
        assert!(status.success(), "git {args:?} failed");
    }

    fn cleanup_db(path: &Path) {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    fn run(thread_id: Uuid) -> Run {
        Run::new(Uuid::new_v4(), thread_id)
    }

    #[tokio::test]
    async fn a_clean_repo_is_checkout_shared_then_applied() {
        let Some(repo) = temporary_repo() else {
            return;
        };
        let (db, db_path) = temp_db().await;
        let thread = Uuid::new_v4();
        set_isolation(db.pool(), thread, true)
            .await
            .expect("enable");
        let first = run(thread);
        let dirs = maybe_isolate(&db, &first, std::slice::from_ref(&repo))
            .await
            .expect("isolate");
        assert_ne!(dirs[0], repo);
        assert_eq!(dirs[0].parent().unwrap(), worktrees_root());
        assert_eq!(
            std::fs::read_to_string(dirs[0].join("note.txt")).unwrap(),
            "one\n"
        );
        let second = run(thread);
        let reused = maybe_isolate(&db, &second, std::slice::from_ref(&repo))
            .await
            .expect("reuse");
        assert_eq!(reused, dirs);

        std::fs::write(dirs[0].join("note.txt"), "two\n").unwrap();
        std::fs::write(dirs[0].join("new.txt"), "new\n").unwrap();
        // An agent is allowed to commit inside its checkout; the review must
        // compare against the recorded base, not the moved HEAD.
        git(&dirs[0], &["add", "-A"]);
        git(&dirs[0], &["commit", "-qm", "isolated work"]);
        let review = review(db.pool(), thread).await.expect("review");
        assert!(review.files.iter().any(|line| line.contains("note.txt")));
        assert!(review.files.iter().any(|line| line.contains("new.txt")));
        assert!(review.diff.contains("+two"), "{}", review.diff);

        let applied = apply(&db, thread).await.expect("apply");
        assert_eq!(applied.status, "applied");
        assert_eq!(
            std::fs::read_to_string(repo.join("note.txt")).unwrap(),
            "two\n"
        );
        assert_eq!(
            std::fs::read_to_string(repo.join("new.txt")).unwrap(),
            "new\n"
        );
        assert!(!dirs[0].exists(), "the checkout is removed after apply");

        cleanup_db(&db_path);
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[tokio::test]
    async fn a_nested_project_folder_stays_nested_in_the_checkout() {
        let Some(repo) = temporary_repo() else {
            return;
        };
        std::fs::create_dir_all(repo.join("app")).unwrap();
        std::fs::write(repo.join("app/main.rs"), "fn main() {}\n").unwrap();
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-qm", "nested app"]);

        let (db, db_path) = temp_db().await;
        let thread = Uuid::new_v4();
        set_isolation(db.pool(), thread, true)
            .await
            .expect("enable");
        let dirs = maybe_isolate(&db, &run(thread), std::slice::from_ref(&repo.join("app")))
            .await
            .expect("isolate nested");
        assert_eq!(
            dirs[0].file_name().and_then(|name| name.to_str()),
            Some("app")
        );
        assert!(dirs[0].join("main.rs").is_file());
        let record = ThreadWorktreeQueries::active_for_thread(db.pool(), thread)
            .await
            .unwrap()
            .expect("record");
        assert_eq!(
            std::path::Path::new(&record.agent_path)
                .file_name()
                .and_then(|name| name.to_str()),
            Some("app")
        );

        discard(&db, thread).await.expect("discard");
        cleanup_db(&db_path);
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[tokio::test]
    async fn small_ignored_local_files_are_available_in_the_checkout() {
        let Some(repo) = temporary_repo() else {
            return;
        };
        std::fs::write(repo.join(".gitignore"), ".env\n").unwrap();
        git(&repo, &["add", ".gitignore"]);
        git(&repo, &["commit", "-qm", "ignore env"]);
        std::fs::write(repo.join(".env"), "SECRET=local\n").unwrap();

        let (db, db_path) = temp_db().await;
        let thread = Uuid::new_v4();
        set_isolation(db.pool(), thread, true)
            .await
            .expect("enable");
        let dirs = maybe_isolate(&db, &run(thread), std::slice::from_ref(&repo))
            .await
            .expect("isolate");
        assert_eq!(
            std::fs::read_to_string(dirs[0].join(".env")).unwrap(),
            "SECRET=local\n",
            "local ignored configuration is readable in the checkout"
        );
        discard(&db, thread).await.expect("discard");
        cleanup_db(&db_path);
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[tokio::test]
    async fn discard_leaves_the_project_untouched() {
        let Some(repo) = temporary_repo() else {
            return;
        };
        let (db, db_path) = temp_db().await;
        let thread = Uuid::new_v4();
        set_isolation(db.pool(), thread, true)
            .await
            .expect("enable");
        let run = run(thread);
        let dirs = maybe_isolate(&db, &run, std::slice::from_ref(&repo))
            .await
            .expect("isolate");
        std::fs::write(dirs[0].join("note.txt"), "throwaway\n").unwrap();
        let discarded = discard(&db, thread).await.expect("discard");
        assert_eq!(discarded.status, "discarded");
        assert_eq!(
            std::fs::read_to_string(repo.join("note.txt")).unwrap(),
            "one\n"
        );
        assert!(!dirs[0].exists());

        cleanup_db(&db_path);
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[tokio::test]
    async fn isolation_refuses_a_dirty_or_non_git_folder() {
        let Some(repo) = temporary_repo() else {
            return;
        };
        let (db, db_path) = temp_db().await;
        let thread = Uuid::new_v4();
        set_isolation(db.pool(), thread, true)
            .await
            .expect("enable");
        std::fs::write(repo.join("dirty.txt"), "uncommitted\n").unwrap();
        let error = maybe_isolate(&db, &run(thread), std::slice::from_ref(&repo))
            .await
            .expect_err("dirty repo");
        assert!(
            error.to_string().contains("clean Git repository"),
            "{error}"
        );

        let plain =
            std::env::temp_dir().join(format!("ravenbot-worktree-plain-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&plain).unwrap();
        let thread = Uuid::new_v4();
        set_isolation(db.pool(), thread, true)
            .await
            .expect("enable");
        let error = maybe_isolate(&db, &run(thread), std::slice::from_ref(&plain))
            .await
            .expect_err("not a repo");
        assert!(error.to_string().contains("Git"), "{error}");

        // Disabled isolation is a no-op even for a plain folder.
        let thread = Uuid::new_v4();
        let dirs = maybe_isolate(&db, &run(thread), std::slice::from_ref(&plain))
            .await
            .expect("disabled");
        assert_eq!(dirs, vec![plain.clone()]);

        cleanup_db(&db_path);
        let _ = std::fs::remove_dir_all(&repo);
        let _ = std::fs::remove_dir_all(&plain);
    }

    #[tokio::test]
    async fn the_toggle_is_stored_per_conversation() {
        let (db, db_path) = temp_db().await;
        let thread = Uuid::new_v4();
        assert!(!isolation_enabled(db.pool(), thread).await.expect("off"));
        set_isolation(db.pool(), thread, true).await.expect("on");
        assert!(isolation_enabled(db.pool(), thread).await.expect("on"));
        let key = setting_key(thread);
        assert_eq!(
            AppSettingsQueries::get(db.pool(), &key)
                .await
                .expect("get")
                .as_deref(),
            Some("1")
        );
        cleanup_db(&db_path);
    }
}
