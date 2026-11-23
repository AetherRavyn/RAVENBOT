//! Workspace-aware command safety.
//!
//! Office agents run autonomously: no Allow/Deny card interrupts a build, a
//! test run, or a deploy. That makes this module the hard boundary rather than
//! a suggestion in a prompt. It is enforced by `shell_exec`, `task_runner`, and
//! `docker` before the sandbox is even asked to start a process.
//!
//! The rules are deliberately about *targets*, not about command names alone.
//! `rm -rf build/` is normal work; `rm -rf ~` is not. `git reset --hard HEAD~1`
//! is a normal (if sharp) tool; `git reset --hard` can discard an uncommitted
//! tree, and `git clean -fdx` can delete files that were never tracked. A
//! command with a dangerous verb is allowed when every path it names is inside
//! one of the run's workspace roots, and refused otherwise.
//!
//! Two functions are exported because the two questions are different:
//!
//! - [`dangerous_command_reason`] answers "can this destroy the machine, the
//!   repository history, or data outside the office?" and refuses.
//! - [`wasteful_scan_reason`] answers "will this burn the round budget scanning
//!   the whole filesystem instead of using `file_tree`?" and refuses with a
//!   pointer at the right tool. It is tool discipline, not a security boundary.

use std::path::{Component, Path, PathBuf};

/// Refuse a command that can destroy something outside the run's workspace.
///
/// `roots` are the run's configured project folders. An empty list means the
/// caller has no workspace (headless runs, tests): relative work is still
/// allowed, but absolute system paths and the obvious machine-destroying
/// commands are refused.
pub fn dangerous_command_reason(command: &str, roots: &[PathBuf]) -> Option<String> {
    let words = tokenize(command);
    if words.is_empty() {
        return None;
    }

    // Work through shell segments so `cd /tmp && rm -rf x` is read as two
    // commands rather than as one opaque string. `cwd` follows `cd`/`pushd`
    // inside the workspace, so `cd build && rm -rf *` is judged against
    // `build/` rather than against the office root.
    let mut cwd = roots.first().cloned();
    for segment in segments(&words) {
        if segment.is_empty() {
            continue;
        }
        if let Some(reason) = check_segment(segment, roots, cwd.as_deref()) {
            return Some(reason);
        }
        let program = basename(&segment[0]);
        if matches!(program.as_str(), "cd" | "pushd") {
            if let Ok(Some(next)) = resolve_directory_change(segment, roots, cwd.as_deref()) {
                cwd = Some(next);
            }
        }
    }
    None
}

/// Refuse a whole-machine scan that should be a `file_tree` / `code_search`
/// call. Workspace-scoped scans are allowed.
pub fn wasteful_scan_reason(command: &str, roots: &[PathBuf]) -> Option<String> {
    let words = tokenize(command);
    for segment in segments(&words) {
        if segment.is_empty() {
            continue;
        }
        let Some(program) = segment.first().map(|word| basename(word)) else {
            continue;
        };
        match program.as_str() {
            "find" | "fd" => {
                if let Some(target) = find_scan_target(segment) {
                    if is_whole_machine_target(&target, roots) {
                        return Some(format!(
                            "scanning '{}' reads the whole machine. Use file_tree for paths and code_search for content inside the workspace",
                            target
                        ));
                    }
                }
            }
            "grep" | "egrep" | "fgrep" | "rg" | "ag" => {
                if segment
                    .iter()
                    .any(|word| word == "-r" || word == "-R" || word == "--recursive")
                {
                    if let Some(target) = last_path_argument(segment) {
                        if is_whole_machine_target(&target, roots) {
                            return Some(format!(
                                "recursive grep over '{}' reads the whole machine. Use code_search for content inside the workspace",
                                target
                            ));
                        }
                    }
                }
            }
            "ls" | "du" | "tree" => {
                if let Some(target) = last_path_argument(segment) {
                    if is_whole_machine_target(&target, roots) {
                        return Some(format!(
                            "listing '{}' walks outside the workspace. Use file_tree for the workspace tree",
                            target
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    None
}

// ── token / segment parsing ───────────────────────────────────────────────

/// Split a shell line into words, preserving quoted arguments as one word.
///
/// This is not a shell parser and does not try to be: it only has to see the
/// program, its flags, and the path arguments that follow. Quote handling is
/// enough to keep `rm -rf "My Folder"` from being read as two targets.
fn tokenize(command: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for ch in command.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' && quote != Some('\'') {
            escaped = true;
            continue;
        }
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            } else {
                current.push(ch);
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            c if c.is_whitespace() => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            ';' | '|' | '&' => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
                words.push(ch.to_string());
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// Split words at `;`, `&&`, `||`, and pipes.
fn segments(words: &[String]) -> Vec<&[String]> {
    let mut out = Vec::new();
    let mut start = 0;
    for (index, word) in words.iter().enumerate() {
        if matches!(word.as_str(), ";" | "|" | "||" | "&&" | "&") {
            if index > start {
                out.push(&words[start..index]);
            }
            start = index + 1;
        }
    }
    if start < words.len() {
        out.push(&words[start..]);
    }
    out
}

// ── per-segment checks ────────────────────────────────────────────────────

fn check_segment(segment: &[String], roots: &[PathBuf], cwd: Option<&Path>) -> Option<String> {
    let program = basename(segment.first()?);

    // `env VAR=x cmd` and `command cmd` keep the real program in a later word.
    if matches!(program.as_str(), "env" | "command" | "builtin" | "exec") {
        let mut rest = &segment[1..];
        while let Some(first) = rest.first() {
            if first.contains('=') && !first.starts_with('-') {
                rest = &rest[1..];
            } else {
                return check_segment(rest, roots, cwd);
            }
        }
        return None;
    }

    // `sh -c 'rm -rf /'` is one shell word, so the outer parser would never
    // see the delete. Recurse into the command string before anything else.
    if matches!(
        program.as_str(),
        "sh" | "bash" | "zsh" | "fish" | "dash" | "ksh"
    ) {
        if let Some(index) = segment
            .iter()
            .enumerate()
            .skip(1)
            .find_map(|(index, word)| {
                let is_command_flag = word == "-c"
                    || (word.starts_with('-')
                        && !word.starts_with("--")
                        && word[1..].contains('c'));
                is_command_flag.then_some(index + 1)
            })
        {
            if let Some(inner) = segment.get(index) {
                return dangerous_command_reason(inner, roots);
            }
        }
    }

    // Python/Node/Ruby inline code is another way to reach the same deletes
    // without spelling a shell command. This is deliberately conservative: a
    // script that names protected state or a catastrophic verb is refused,
    // while ordinary interpreter work is untouched.
    if matches!(
        program.as_str(),
        "python" | "python3" | "node" | "ruby" | "perl" | "php" | "deno" | "bun"
    ) {
        let joined = segment.join(" ").to_ascii_lowercase();
        if joined.contains("ravenbot.db")
            || joined.contains("accounts.sqlite")
            || joined.contains("fleet.lock")
            || joined.contains("last-session.json")
        {
            return Some(
                "inline code may not touch RAVENBOT's database or session state".to_string(),
            );
        }
        for phrase in [
            "rm -rf /",
            "rm -rf /*",
            "mkfs",
            "drop database",
            "drop schema",
            "drop table",
            "truncate table",
            "flushall",
            "flushdb",
            "rmsync",
            "rmtree",
            "os.remove",
            "unlink(",
        ] {
            if joined.contains(phrase) {
                return Some(format!(
                    "inline code contains destructive operation '{}'",
                    phrase.trim()
                ));
            }
        }
    }

    if program == "sudo" || program == "doas" {
        // Privilege escalation is never part of an office task. Refuse it
        // rather than trying to reason about what it escalates to.
        return Some(format!(
            "'{}' asks for host administrator rights, which an office agent never needs",
            program
        ));
    }

    if program == "cd" || program == "pushd" {
        return resolve_directory_change(segment, roots, cwd).err();
    }

    if let Some(reason) = check_destructive_git(segment) {
        return Some(reason);
    }

    if let Some(reason) = check_disk_and_power(segment) {
        return Some(reason);
    }

    if let Some(reason) = check_docker(segment, roots) {
        return Some(reason);
    }

    if let Some(reason) = check_database_command(segment) {
        return Some(reason);
    }

    if let Some(reason) = check_protected_write(segment, roots, cwd) {
        return Some(reason);
    }

    if is_delete_command(segment) {
        if let Some(reason) = check_delete_targets(segment, roots, cwd) {
            return Some(reason);
        }
    }

    // `find … -delete` and `find … -exec rm …` are recursive deletes with no
    // `rm` at the front.
    if program == "find" {
        let destructive = segment.iter().any(|word| word == "-delete")
            || segment.windows(2).any(|pair| {
                pair[0] == "-exec"
                    && matches!(basename(&pair[1]).as_str(), "rm" | "shred" | "unlink")
            });
        if destructive {
            if let Some(target) = find_scan_target(segment) {
                if let Some(reason) = defend_target(&target, roots, cwd, "delete") {
                    return Some(reason);
                }
            }
        }
    }

    // A redirect into a block device is a disk wipe.
    if segment.iter().any(|word| {
        matches!(
            word.as_str(),
            ">/dev/sd" | ">/dev/nvme" | ">/dev/mmcblk" | "> /dev/sd" | "> /dev/nvme"
        )
    }) {
        return Some("writing to a block device destroys the disk".to_string());
    }

    None
}

/// Resolve a `cd`/`pushd` target, refusing a destination outside the roots.
/// `Ok(None)` means there is nothing to change (`cd` with no argument, `cd -`).
fn resolve_directory_change(
    segment: &[String],
    roots: &[PathBuf],
    cwd: Option<&Path>,
) -> Result<Option<PathBuf>, String> {
    let Some(target) = segment.get(1) else {
        return Ok(None);
    };
    // `cd -` returns to the previous directory and is harmless to inspect; a
    // workspace-aware shell has no meaning for it, so leave the tracked cwd
    // alone.
    if target == "-" {
        return Ok(None);
    }
    let expanded = expand_path(target);
    if roots.is_empty() {
        if expanded.is_absolute() || path_escapes(&expanded) {
            return Err(format!(
                "changing directory to '{}' leaves the workspace",
                target
            ));
        }
        return Ok(Some(lexical_join(cwd.unwrap_or(Path::new(".")), &expanded)));
    }

    let base = cwd
        .filter(|path| inside_any(path, roots))
        .or_else(|| roots.first().map(PathBuf::as_path))
        .unwrap_or(Path::new("."));
    let resolved = if expanded.is_absolute() {
        expanded
    } else {
        lexical_join(base, &expanded)
    };
    if !inside_any(&resolved, roots) {
        return Err(format!(
            "changing directory to '{}' leaves the office workspace; run commands inside {}",
            target,
            roots_display(roots)
        ));
    }
    Ok(Some(resolved))
}

fn check_destructive_git(segment: &[String]) -> Option<String> {
    if basename(segment.first()?) != "git" {
        return None;
    }
    // Skip global flags: `git -C path reset …`, `git --no-pager …`.
    let mut index = 1;
    while index < segment.len() && segment[index].starts_with('-') {
        if matches!(
            segment[index].as_str(),
            "-C" | "-c" | "--git-dir" | "--work-tree" | "--exec-path"
        ) {
            index += 2;
        } else {
            index += 1;
        }
    }
    let subcommand = segment.get(index).map(String::as_str)?;
    let rest = &segment[index + 1..];
    let has = |needle: &str| rest.iter().any(|word| word == needle);
    let joined = rest.join(" ");

    match subcommand {
        "reset" if has("--hard") => Some(
            "git reset --hard discards uncommitted work; use `git stash` or commit first"
                .to_string(),
        ),
        "clean"
            if rest
                .iter()
                .any(|word| word.starts_with('-') && word.contains('f')) =>
        {
            Some(
                "git clean -f deletes untracked files; inspect with `git clean -nd` first"
                    .to_string(),
            )
        }
        "push" if has("--force") || has("-f") || joined.contains("--force-with-lease") => {
            Some("force-pushing rewrites shared history; push a new commit instead".to_string())
        }
        "branch" if has("-D") => Some(
            "git branch -D deletes a branch without checking whether it was merged".to_string(),
        ),
        "filter-branch" | "filter-repo" => {
            Some("history rewriting is not an office task".to_string())
        }
        "reflog" if has("expire") => {
            Some("expiring the reflog removes the recovery path for lost commits".to_string())
        }
        "gc" if joined.contains("--prune=now") => {
            Some("git gc --prune=now removes unreachable objects immediately".to_string())
        }
        "checkout" | "restore" if rest.iter().any(|word| word == ".") && has("--") => {
            Some("restoring `.` discards every uncommitted change in the tree".to_string())
        }
        _ => None,
    }
}

fn check_disk_and_power(segment: &[String]) -> Option<String> {
    let program = basename(segment.first()?);

    if program.starts_with("mkfs") {
        return Some("formatting a filesystem destroys its contents".to_string());
    }
    match program.as_str() {
        "fdisk" | "sfdisk" | "cfdisk" | "parted" | "gdisk" | "wipefs" | "blkdiscard"
        | "shutdown" | "reboot" | "poweroff" | "halt" => {
            return Some(format!(
                "'{}' changes or powers off the host machine",
                program
            ));
        }
        "dd" => {
            let joined = segment.join(" ");
            if joined.contains("of=/dev/") || joined.contains("of=/dev") {
                return Some("dd writing to a device destroys the disk".to_string());
            }
        }
        "systemctl" => {
            if segment.iter().any(|word| {
                matches!(
                    word.as_str(),
                    "poweroff" | "reboot" | "halt" | "suspend" | "hibernate"
                )
            }) {
                return Some("systemctl power/reboot commands stop the host".to_string());
            }
        }
        "init" => {
            if matches!(segment.get(1).map(String::as_str), Some("0" | "6")) {
                return Some("init 0/6 powers off or reboots the host".to_string());
            }
        }
        _ => {}
    }
    None
}

fn check_docker(segment: &[String], roots: &[PathBuf]) -> Option<String> {
    // Only the docker CLI, not every command that mentions a container.
    let docker_at = segment
        .iter()
        .position(|word| basename(word) == "docker")
        .unwrap_or(usize::MAX);
    if docker_at == usize::MAX {
        return None;
    }
    let args = &segment[docker_at + 1..];
    let sub = args.first().map(String::as_str)?;
    if !matches!(sub, "run" | "create" | "exec" | "start") {
        return None;
    }
    let joined = args.join(" ");
    if args.iter().any(|word| word == "--privileged") || joined.contains("--privileged") {
        return Some("privileged containers can escape to the host".to_string());
    }
    if args.iter().any(|word| {
        word.starts_with("--cap-add")
            || word.starts_with("--pid=host")
            || word.starts_with("--ipc=host")
            || word.starts_with("--userns=host")
            || word.starts_with("--device")
    }) {
        return Some(
            "host namespaces, devices, and extra capabilities are not allowed".to_string(),
        );
    }
    // Bind mounts. A workspace folder may be mounted; anything else on the
    // host is a way to reach the machine through the container.
    for (index, word) in args.iter().enumerate() {
        let mount = if word == "-v" || word == "--volume" || word == "--mount" {
            args.get(index + 1).map(String::as_str).unwrap_or("")
        } else if let Some(rest) = word.strip_prefix("-v") {
            rest
        } else if let Some(rest) = word.strip_prefix("--volume=") {
            rest
        } else if let Some(rest) = word.strip_prefix("--mount=") {
            rest
        } else {
            continue;
        };
        let source = if mount.contains("source=") {
            mount
                .split(',')
                .find_map(|part| part.trim().strip_prefix("source="))
                .unwrap_or(mount)
        } else {
            mount.split(':').next().unwrap_or(mount)
        }
        .trim();
        if source.is_empty() || !source.starts_with('/') {
            // A named volume or relative path is not a host bind mount.
            continue;
        }
        let source_path = PathBuf::from(source);
        if source == "/"
            || source == "/dev"
            || source == "/proc"
            || source == "/sys"
            || source.contains("docker.sock")
            || (!roots.is_empty() && !inside_any(&source_path, roots))
        {
            return Some(format!(
                "mounting '{}' exposes a host path outside the workspace to the container",
                source
            ));
        }
    }
    None
}

fn is_delete_command(segment: &[String]) -> bool {
    let program = basename(&segment[0]);
    if matches!(program.as_str(), "rm" | "rmdir" | "unlink" | "shred") {
        return true;
    }
    // `busybox rm` and similar wrappers.
    segment
        .iter()
        .skip(1)
        .take(2)
        .any(|word| basename(word) == "rm")
}

fn check_delete_targets(
    segment: &[String],
    roots: &[PathBuf],
    cwd: Option<&Path>,
) -> Option<String> {
    let recursive = segment.iter().any(|word| {
        word == "-r"
            || word == "-R"
            || word == "--recursive"
            || (word.starts_with('-') && !word.starts_with("--") && word.contains('r'))
    });
    let targets: Vec<&String> = segment
        .iter()
        .skip(1)
        .filter(|word| !word.starts_with('-'))
        .collect();
    if targets.is_empty() && recursive {
        return Some("a recursive delete with no explicit target is refused".to_string());
    }
    for target in targets {
        if let Some(reason) = defend_target(target, roots, cwd, "delete") {
            return Some(reason);
        }
    }
    None
}

/// The common target policy for deletes and `find -delete`.
fn defend_target(raw: &str, roots: &[PathBuf], cwd: Option<&Path>, verb: &str) -> Option<String> {
    // A target the shell will rewrite after this check is not a target we can
    // vouch for. Command substitution and untrusted variables can turn a
    // harmless-looking relative path into `/`. `$HOME` / `${HOME}` are the one
    // pair we expand ourselves below.
    if raw.contains("$(") || raw.contains('`') {
        return Some(format!(
            "refusing to {} '{}': command substitution in a delete target is not verifiable",
            verb, raw
        ));
    }
    if raw.trim_start().starts_with('~') && raw.trim() != "~" && !raw.trim().starts_with("~/") {
        return Some(format!(
            "refusing to {} '{}': named home expansion is outside the workspace",
            verb, raw
        ));
    }
    let expanded = expand_path(raw);
    if expanded.to_string_lossy().contains('$') {
        return Some(format!(
            "refusing to {} '{}': an unexpanded shell variable can change the target after this check",
            verb, raw
        ));
    }
    let target_text = expanded.to_string_lossy().to_string();

    // Shell globs are judged by their parent path: `rm -rf *` at the office
    // root is the same as deleting the office, while `rm -rf build/*` is fine.
    if raw.contains('*') || raw.contains('?') || raw.contains('[') {
        let parent = Path::new(raw)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let base = cwd
            .filter(|path| inside_any(path, roots))
            .or_else(|| roots.first().map(PathBuf::as_path))
            .unwrap_or(Path::new("."));
        let parent = if parent.as_os_str().is_empty() {
            base.to_path_buf()
        } else {
            lexical_join(base, &parent)
        };
        if roots.is_empty() {
            if parent == Path::new("/") || parent == Path::new(".") {
                return Some(format!(
                    "refusing to {} a top-level wildcard ('{}')",
                    verb, raw
                ));
            }
            return None;
        }
        if !inside_any(&parent, roots) || is_root(&parent, roots) {
            return Some(format!(
                "refusing to {} '{}': a wildcard at the workspace root can remove the office itself",
                verb, raw
            ));
        }
        return None;
    }

    // `.` / `..` are the current and parent directory, never a file to erase.
    if matches!(target_text.as_str(), "." | "..") {
        return Some(format!(
            "refusing to {} '{}' recursively",
            verb, target_text
        ));
    }

    // `.git` is the repository's history; it is never a disposable build
    // directory, even inside the workspace.
    if expanded
        .components()
        .any(|component| matches!(component, Component::Normal(name) if name == ".git"))
    {
        return Some("refusing to delete a .git directory (repository history)".to_string());
    }

    // RAVENBOT's own database, account registry, keys and config are not
    // project files. An agent may work in the repository root, so name-based
    // protection is the only thing between a plausible-looking `rm ravenbot.db`
    // and the live workspace.
    if let Some(reason) = protected_path_reason(&expanded, roots) {
        return Some(reason);
    }

    // A path that already resolves somewhere else (a symlink) must resolve
    // inside the workspace. This is the same escape a lexical check misses.
    if let Some(real) = canonicalize_existing(&expanded) {
        if !roots.is_empty() && !inside_any(&real, roots) {
            return Some(format!(
                "refusing to {} '{}': it resolves to '{}', outside the workspace",
                verb,
                raw,
                real.display()
            ));
        }
    }

    if roots.is_empty() {
        if is_absolute_system_path(&expanded) {
            return Some(format!(
                "refusing to {} '{}': it is outside any workspace",
                verb, raw
            ));
        }
        return None;
    }

    let base = cwd
        .filter(|path| inside_any(path, roots))
        .or_else(|| roots.first().map(PathBuf::as_path))
        .unwrap_or(Path::new("."));
    let resolved = if expanded.is_absolute() {
        expanded
    } else {
        lexical_join(base, &expanded)
    };
    if !inside_any(&resolved, roots) {
        return Some(format!(
            "refusing to {} '{}': every delete must stay inside {}",
            verb,
            raw,
            roots_display(roots)
        ));
    }
    if let Some(reason) = protected_path_reason(&resolved, roots) {
        return Some(reason);
    }
    if is_root(&resolved, roots) {
        return Some(format!(
            "refusing to {} the workspace root '{}'; delete a subfolder instead",
            verb,
            resolved.display()
        ));
    }
    None
}

// ── hard safety for database and app state ────────────────────────────────

/// SQLite/Postgres/MySQL/Redis/Mongo commands that erase shared state.
///
/// The database tools are read-only, but `shell_exec` can invoke a client
/// directly or through `sh -c`. A destructive statement is refused here rather
/// than being presented as an approval card; there is no human to answer an
/// unattended run, and a mis-clicked "Allow" is worse than a clear refusal.
fn check_database_command(segment: &[String]) -> Option<String> {
    let program = basename(segment.first()?);
    let database_client = matches!(
        program.as_str(),
        "sqlite3"
            | "psql"
            | "mysql"
            | "mariadb"
            | "mongosh"
            | "mongo"
            | "redis-cli"
            | "cqlsh"
            | "clickhouse-client"
            | "duckdb"
    );
    let shell = matches!(
        program.as_str(),
        "sh" | "bash" | "zsh" | "fish" | "python" | "python3" | "node" | "ruby" | "perl"
    );
    if !database_client && !shell {
        return None;
    }
    destructive_sql_reason(&segment.join(" "))
}

fn destructive_sql_reason(sql: &str) -> Option<String> {
    let joined = sql.to_ascii_lowercase();
    for phrase in [
        "drop database",
        "drop schema",
        "drop table",
        "truncate table",
        "truncate ",
        "flushall",
        "flushdb",
        "dropdatabase",
        "alter system",
    ] {
        if joined.contains(phrase) {
            return Some(format!(
                "destructive database operation '{}' would erase shared data",
                phrase.trim()
            ));
        }
    }
    if joined.contains("delete from") && !joined.contains(" where ") {
        return Some(
            "DELETE without a WHERE clause would erase an entire table; add a predicate"
                .to_string(),
        );
    }
    None
}

/// Refuse writes and redirections aimed at RAVENBOT's own state files.
///
/// `rm` is not the only way to destroy a database: `echo > ravenbot.db`,
/// `cp template ravenbot.db`, `truncate -s 0`, and `sqlite3 ravenbot.db` all
/// corrupt or rewrite it. The workspace can legitimately be the RAVENBOT tree,
/// so these names are checked on every write-shaped command.
fn check_protected_write(
    segment: &[String],
    roots: &[PathBuf],
    cwd: Option<&Path>,
) -> Option<String> {
    let program = basename(segment.first()?);
    let mut targets: Vec<String> = Vec::new();
    if matches!(
        program.as_str(),
        "cp" | "mv"
            | "install"
            | "ln"
            | "tee"
            | "truncate"
            | "touch"
            | "sqlite3"
            | "psql"
            | "mysql"
            | "mariadb"
            | "mongosh"
            | "mongo"
            | "redis-cli"
            | "cqlsh"
            | "clickhouse-client"
            | "duckdb"
    ) {
        for word in segment.iter().skip(1) {
            if !word.starts_with('-') {
                targets.push(word.clone());
            }
        }
    }
    if program == "dd" {
        for word in segment.iter() {
            if let Some(target) = word.strip_prefix("of=") {
                targets.push(target.to_string());
            }
        }
    }
    let mut index = 0;
    while index < segment.len() {
        let word = &segment[index];
        if word == ">" || word == ">>" {
            if let Some(next) = segment.get(index + 1) {
                targets.push(next.clone());
            }
        } else if let Some(rest) = word.strip_prefix(">>") {
            if !rest.is_empty() {
                targets.push(rest.to_string());
            }
        } else if let Some(rest) = word.strip_prefix('>') {
            if !rest.is_empty() {
                targets.push(rest.to_string());
            }
        }
        index += 1;
    }

    let base = cwd
        .filter(|path| inside_any(path, roots))
        .or_else(|| roots.first().map(PathBuf::as_path))
        .unwrap_or(Path::new("."));
    for raw in targets {
        let expanded = expand_path(&raw);
        if expanded.to_string_lossy().contains('$') {
            continue;
        }
        let resolved = if expanded.is_absolute() {
            expanded
        } else {
            lexical_join(base, &expanded)
        };
        if let Some(reason) = protected_path_reason(&resolved, roots) {
            return Some(reason);
        }
        if program == "mv" && (raw == "*" || raw == "." || is_root(&resolved, roots)) {
            return Some(
                "moving the workspace root or all of its contents would remove the office"
                    .to_string(),
            );
        }
    }
    None
}

/// Files that belong to RAVENBOT itself, not to the project.
///
/// File writes are workspace-confined, but the owner's workspace is the
/// RAVENBOT tree: a write to `ravenbot.db` or a delete of `accounts/` is
/// otherwise perfectly "inside the office". These names are protected wherever
/// they appear; state directories are protected at their workspace root.
pub fn protected_path_reason(path: &Path, roots: &[PathBuf]) -> Option<String> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    if matches!(
        name.as_str(),
        "ravenbot.db"
            | "ravenbot.db-wal"
            | "ravenbot.db-shm"
            | "accounts.sqlite"
            | "accounts.sqlite-wal"
            | "accounts.sqlite-shm"
            | "fleet.lock"
            | "last-session.json"
    ) {
        return Some(format!(
            "'{}' is RAVENBOT's own database or session state; agents cannot delete or overwrite it",
            path.display()
        ));
    }
    for root in roots {
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let Some(Component::Normal(first)) = relative.components().next() else {
            continue;
        };
        let first = first.to_string_lossy();
        if matches!(
            first.as_ref(),
            "accounts" | "keys" | "browser-profiles" | "config" | "vault"
        ) {
            return Some(format!(
                "'{}' is RAVENBOT account/key/configuration state, not a project file",
                path.display()
            ));
        }
    }
    None
}

/// Pre-approval safety check for the runtime's tool loop.
///
/// The skills enforce this too; checking before the approval ladder means a
/// destructive call is never shown as an Allow/Deny card even if a bot is
/// configured for `ask` or `auto`.
pub fn dangerous_tool_call_reason(
    tool_name: &str,
    arguments: &serde_json::Value,
    roots: &[PathBuf],
) -> Option<String> {
    match tool_name {
        "shell_exec" => arguments
            .get("command")
            .and_then(|value| value.as_str())
            .and_then(|command| dangerous_command_reason(command, roots)),
        "task_runner" => arguments
            .get("task")
            .or_else(|| arguments.get("command"))
            .and_then(|value| value.as_str())
            .and_then(|command| dangerous_command_reason(command, roots)),
        "file_write" => arguments
            .get("path")
            .and_then(|value| value.as_str())
            .map(|raw| {
                let path = Path::new(raw);
                if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    roots
                        .first()
                        .map(|root| root.join(path))
                        .unwrap_or_else(|| path.to_path_buf())
                }
            })
            .and_then(|path| protected_path_reason(&path, roots)),
        "db_query" => arguments
            .get("sql")
            .and_then(|value| value.as_str())
            .and_then(destructive_sql_reason),
        _ => None,
    }
}

// ── path helpers ──────────────────────────────────────────────────────────

fn basename(word: &str) -> String {
    Path::new(word)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| word.to_string())
}

fn expand_path(raw: &str) -> PathBuf {
    let trimmed = raw.trim().trim_matches('"').trim_matches('\'');
    if trimmed == "~" || trimmed.starts_with("~/") {
        if let Ok(home) = std::env::var("HOME") {
            if !home.trim().is_empty() {
                return PathBuf::from(home).join(trimmed.trim_start_matches("~/"));
            }
        }
    }
    for prefix in ["$HOME", "${HOME}"] {
        if trimmed == prefix || trimmed.starts_with(&format!("{prefix}/")) {
            if let Ok(home) = std::env::var("HOME") {
                if !home.trim().is_empty() {
                    return PathBuf::from(home)
                        .join(trimmed.trim_start_matches(prefix).trim_start_matches('/'));
                }
            }
        }
    }
    PathBuf::from(trimmed)
}

fn lexical_join(base: &Path, path: &Path) -> PathBuf {
    let joined = base.join(path);
    lexical_normalize(&joined)
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut out: Vec<Component> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match out.last() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir) | Some(Component::Prefix(_)) => {}
                _ => out.push(component),
            },
            other => out.push(other),
        }
    }
    if out.is_empty() {
        PathBuf::from(".")
    } else {
        out.iter().collect()
    }
}

fn canonicalize_existing(path: &Path) -> Option<PathBuf> {
    if let Ok(real) = std::fs::canonicalize(path) {
        return Some(real);
    }
    // For a path that does not exist yet, canonicalize the deepest existing
    // ancestor and append the rest. This catches a symlinked parent.
    let mut current = path.to_path_buf();
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    loop {
        if let Ok(real) = std::fs::canonicalize(&current) {
            let mut out = real;
            for part in tail.iter().rev() {
                out.push(part);
            }
            return Some(out);
        }
        let name = current.file_name()?.to_os_string();
        tail.push(name);
        if !current.pop() {
            return None;
        }
    }
}

fn inside_any(path: &Path, roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| {
        let root = canonicalize_existing(root).unwrap_or_else(|| lexical_normalize(root));
        let path = canonicalize_existing(path).unwrap_or_else(|| lexical_normalize(path));
        path.starts_with(&root)
    })
}

fn is_root(path: &Path, roots: &[PathBuf]) -> bool {
    let path = canonicalize_existing(path).unwrap_or_else(|| lexical_normalize(path));
    roots.iter().any(|root| {
        let root = canonicalize_existing(root).unwrap_or_else(|| lexical_normalize(root));
        path == root
    })
}

fn path_escapes(path: &Path) -> bool {
    let mut depth = 0i32;
    for component in path.components() {
        match component {
            Component::ParentDir => {
                depth -= 1;
                if depth < 0 {
                    return true;
                }
            }
            Component::Normal(_) => depth += 1,
            Component::RootDir | Component::Prefix(_) => return true,
            Component::CurDir => {}
        }
    }
    false
}

fn is_absolute_system_path(path: &Path) -> bool {
    let text = path.to_string_lossy();
    let normalized = text.trim_end_matches('/');
    matches!(
        normalized,
        "/" | "/bin"
            | "/boot"
            | "/dev"
            | "/etc"
            | "/home"
            | "/lib"
            | "/lib64"
            | "/opt"
            | "/proc"
            | "/root"
            | "/run"
            | "/sbin"
            | "/srv"
            | "/sys"
            | "/tmp"
            | "/usr"
            | "/var"
    ) || normalized.starts_with("/etc/")
        || normalized.starts_with("/boot/")
        || normalized.starts_with("/dev/")
        || normalized.starts_with("/proc/")
        || normalized.starts_with("/sys/")
        || normalized.starts_with("/usr/")
        || normalized.starts_with("/var/")
        || normalized.starts_with("/home/")
        || normalized.starts_with("/root/")
}

fn is_whole_machine_target(raw: &str, roots: &[PathBuf]) -> bool {
    let expanded = expand_path(raw);
    let text = expanded.to_string_lossy();
    if matches!(text.as_ref(), "/" | "~" | "$HOME" | "${HOME}" | "/*") {
        return true;
    }
    if text.starts_with("/home/")
        || text.starts_with("/usr/")
        || text.starts_with("/etc/")
        || text.starts_with("/var/")
        || text.starts_with("/boot/")
        || text.starts_with("/proc/")
        || text.starts_with("/sys/")
        || text.starts_with("/dev/")
    {
        return true;
    }
    if text == ".." || text.starts_with("../") {
        return true;
    }
    if roots.is_empty() {
        return false;
    }
    // A scan rooted at an ancestor of the workspace reads everything around
    // it, including the workspace and its siblings.
    let base = roots
        .first()
        .map(PathBuf::as_path)
        .unwrap_or(Path::new("."));
    let resolved = if expanded.is_absolute() {
        expanded
    } else {
        lexical_join(base, &expanded)
    };
    roots
        .iter()
        .any(|root| canonicalize_or_normalize(root).starts_with(&resolved) && &resolved != root)
}

fn canonicalize_or_normalize(path: &Path) -> PathBuf {
    canonicalize_existing(path).unwrap_or_else(|| lexical_normalize(path))
}

fn roots_display(roots: &[PathBuf]) -> String {
    roots
        .iter()
        .map(|root| root.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn find_scan_target(segment: &[String]) -> Option<String> {
    // The first non-flag word after `find` is the start path. Flags that take
    // a value (`-name`, `-maxdepth`) mean the next word is not a path.
    let mut skip_next = false;
    for word in segment.iter().skip(1) {
        if skip_next {
            skip_next = false;
            continue;
        }
        if word.starts_with('-') {
            if matches!(
                word.as_str(),
                "-name"
                    | "-iname"
                    | "-path"
                    | "-ipath"
                    | "-maxdepth"
                    | "-mindepth"
                    | "-type"
                    | "-size"
                    | "-mtime"
                    | "-newer"
                    | "-user"
                    | "-group"
                    | "-perm"
                    | "-exec"
                    | "-execdir"
            ) {
                skip_next = true;
            }
            continue;
        }
        return Some(word.clone());
    }
    None
}

fn last_path_argument(segment: &[String]) -> Option<String> {
    let mut skip_next = false;
    let mut last = None;
    for word in segment.iter().skip(1) {
        if skip_next {
            skip_next = false;
            continue;
        }
        if word.starts_with('-') {
            if matches!(word.as_str(), "-e" | "--regexp" | "--include" | "--exclude") {
                skip_next = true;
            }
            continue;
        }
        last = Some(word.clone());
    }
    last.or_else(|| Some(".".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        std::env::temp_dir().join(format!("rb-safety-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn deleting_the_workspace_root_is_refused() {
        let root = root();
        std::fs::create_dir_all(root.join("build")).unwrap();
        let reason = dangerous_command_reason(
            &format!("rm -rf {}", root.display()),
            std::slice::from_ref(&root),
        )
        .expect("root delete must be refused");
        assert!(reason.contains("workspace root"), "{reason}");

        let allowed = dangerous_command_reason(
            &format!("rm -rf {}", root.join("build").display()),
            std::slice::from_ref(&root),
        );
        assert!(
            allowed.is_none(),
            "subfolder delete should be allowed: {allowed:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn deleting_outside_the_workspace_is_refused() {
        let root = root();
        std::fs::create_dir_all(&root).unwrap();
        let outside = std::env::temp_dir().join(format!("rb-outside-{}", uuid::Uuid::new_v4()));
        let reason = dangerous_command_reason(
            &format!("rm -rf {}", outside.display()),
            std::slice::from_ref(&root),
        );
        assert!(reason.is_some());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn destructive_database_commands_are_refused() {
        let root = root();
        std::fs::create_dir_all(&root).unwrap();
        let roots = std::slice::from_ref(&root);
        for command in [
            "psql -c 'DROP DATABASE app'",
            "sqlite3 ravenbot.db 'DROP TABLE bots'",
            "mysql -e 'DELETE FROM users'",
            "redis-cli FLUSHALL",
            "sh -c 'psql -c \"DROP SCHEMA public\"'",
            "mongosh --eval 'db.dropDatabase()'",
            "sh -c 'rm -rf /'",
            "bash -lc 'rm -rf /'",
            "python3 -c \"import os; os.remove('ravenbot.db')\"",
            "node -e \"require('fs').rmSync('/etc', {recursive:true})\"",
        ] {
            assert!(
                dangerous_command_reason(command, roots).is_some(),
                "expected a refusal for {command}"
            );
        }
        for command in [
            "psql -c 'SELECT 1'",
            "sqlite3 app.db 'DELETE FROM users WHERE id = 1'",
            "redis-cli GET key",
            "sh -c 'cargo test'",
            "python3 -c 'print(1)'",
        ] {
            assert!(
                dangerous_command_reason(command, roots).is_none(),
                "did not expect a refusal for {command}"
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn ravenbot_state_files_are_protected_from_writes_and_deletes() {
        let root = root();
        std::fs::create_dir_all(root.join("accounts")).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        let roots = vec![root.clone()];
        for protected in [
            "ravenbot.db",
            "ravenbot.db-wal",
            "accounts.sqlite",
            "fleet.lock",
            "last-session.json",
            "accounts",
            "keys",
            "config",
            "browser-profiles",
            "vault",
        ] {
            let path = root.join(protected);
            assert!(
                protected_path_reason(&path, &roots).is_some(),
                "{protected} must be protected"
            );
            assert!(
                dangerous_command_reason(
                    &format!("rm -rf {}", path.display()),
                    std::slice::from_ref(&root)
                )
                .is_some(),
                "deleting {protected} must be refused"
            );
        }
        assert!(protected_path_reason(&root.join("src/main.rs"), &roots).is_none());
        assert!(protected_path_reason(&root.join("notes/design.md"), &roots).is_none());
        for command in [
            "echo x > ravenbot.db",
            "echo x >> accounts.sqlite",
            "cp template ravenbot.db",
            "truncate -s 0 ravenbot.db",
            "sqlite3 ravenbot.db 'SELECT 1'",
            "cargo test > ravenbot.db",
        ] {
            assert!(
                dangerous_command_reason(command, std::slice::from_ref(&root)).is_some(),
                "expected a protected-write refusal for {command}"
            );
        }
        assert!(
            dangerous_command_reason("echo x > build.log", std::slice::from_ref(&root)).is_none()
        );
        for command in ["mv . /tmp", "mv * /tmp"] {
            assert!(
                dangerous_command_reason(command, std::slice::from_ref(&root)).is_some(),
                "moving the office must be refused: {command}"
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn tool_calls_are_checked_before_the_approval_ladder() {
        let root = root();
        std::fs::create_dir_all(&root).unwrap();
        let roots = vec![root.clone()];
        assert!(dangerous_tool_call_reason(
            "shell_exec",
            &serde_json::json!({"command": "rm -rf /"}),
            &roots
        )
        .is_some());
        assert!(dangerous_tool_call_reason(
            "file_write",
            &serde_json::json!({"path": "ravenbot.db", "content": "x"}),
            &roots
        )
        .is_some());
        assert!(dangerous_tool_call_reason(
            "db_query",
            &serde_json::json!({"sql": "DROP TABLE bots"}),
            &roots
        )
        .is_some());
        assert!(dangerous_tool_call_reason(
            "shell_exec",
            &serde_json::json!({"command": "cargo test"}),
            &roots
        )
        .is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn shell_expansion_in_a_delete_target_is_refused() {
        let root = root();
        std::fs::create_dir_all(&root).unwrap();
        for command in [
            "rm -rf $(pwd)",
            "rm -rf `pwd`",
            "rm -rf $PWD",
            "rm -rf ~other/build",
        ] {
            assert!(
                dangerous_command_reason(command, std::slice::from_ref(&root)).is_some(),
                "{command} must be refused"
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_relative_delete_after_cd_is_judged_from_the_new_directory() {
        let root = root();
        std::fs::create_dir_all(root.join("build/cache")).unwrap();
        let roots = std::slice::from_ref(&root);
        assert!(
            dangerous_command_reason("cd build && rm -rf *", roots).is_none(),
            "a wildcard inside build/ is a legitimate clean"
        );
        assert!(
            dangerous_command_reason("cd build && rm -rf ../..", roots).is_some(),
            "a relative climb out of the new directory must still be refused"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn deleting_git_is_refused_even_inside_the_workspace() {
        let root = root();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        let reason = dangerous_command_reason(
            &format!("rm -rf {}", root.join(".git").display()),
            std::slice::from_ref(&root),
        )
        .expect("deleting .git must be refused");
        assert!(reason.contains(".git"), "{reason}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_relative_cd_out_of_the_workspace_is_refused() {
        let root = root();
        std::fs::create_dir_all(&root).unwrap();
        assert!(dangerous_command_reason("cd ..", std::slice::from_ref(&root)).is_some());
        assert!(
            dangerous_command_reason("cd .. && rm -rf x", std::slice::from_ref(&root)).is_some()
        );
        assert!(dangerous_command_reason("cd build", std::slice::from_ref(&root)).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn destructive_git_and_disk_commands_are_refused() {
        let root = root();
        for command in [
            "git reset --hard",
            "git -c core.pager=cat reset --hard",
            "git clean -fdx",
            "git push --force origin main",
            "git branch -D feature",
            "git filter-branch --all",
            "mkfs.ext4 /dev/sda1",
            "shutdown -h now",
            "systemctl reboot",
            "docker run --privileged ubuntu",
            "docker run -v /:/host ubuntu",
        ] {
            assert!(
                dangerous_command_reason(command, std::slice::from_ref(&root)).is_some(),
                "{command} must be refused"
            );
        }
        assert!(dangerous_command_reason("git status", std::slice::from_ref(&root)).is_none());
        assert!(dangerous_command_reason("docker ps", std::slice::from_ref(&root)).is_none());
    }

    #[test]
    fn docker_host_mounts_outside_the_workspace_are_refused() {
        let root = root();
        std::fs::create_dir_all(root.join("build")).unwrap();
        let roots = std::slice::from_ref(&root);
        assert!(dangerous_command_reason("docker run -v /etc:/host ubuntu", roots).is_some());
        assert!(dangerous_command_reason(
            "docker run --mount=type=bind,source=/etc,target=/host ubuntu",
            roots
        )
        .is_some());
        assert!(dangerous_command_reason(
            &format!(
                "docker run -v {}:/work ubuntu",
                root.join("build").display()
            ),
            roots
        )
        .is_none());
        assert!(dangerous_command_reason("docker run --device=/dev/sda ubuntu", roots).is_some());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn whole_machine_scans_are_refused_but_workspace_scans_are_allowed() {
        let root = root();
        std::fs::create_dir_all(&root).unwrap();
        assert!(wasteful_scan_reason("find / -maxdepth 2", std::slice::from_ref(&root)).is_some());
        assert!(wasteful_scan_reason("grep -r foo /", std::slice::from_ref(&root)).is_some());
        assert!(wasteful_scan_reason("ls -R /", std::slice::from_ref(&root)).is_some());
        assert!(wasteful_scan_reason("find . -maxdepth 1", std::slice::from_ref(&root)).is_none());
        assert!(wasteful_scan_reason("grep -r foo src", std::slice::from_ref(&root)).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_symlink_that_points_outside_is_refused() {
        #[cfg(unix)]
        {
            let root = root();
            std::fs::create_dir_all(&root).unwrap();
            let outside =
                std::env::temp_dir().join(format!("rb-link-target-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&outside).unwrap();
            let link = root.join("escape");
            std::os::unix::fs::symlink(&outside, &link).unwrap();
            let reason = dangerous_command_reason(
                &format!("rm -rf {}", link.display()),
                std::slice::from_ref(&root),
            );
            assert!(reason.is_some(), "symlink escape must be refused");
            let _ = std::fs::remove_file(&link);
            let _ = std::fs::remove_dir_all(&outside);
            let _ = std::fs::remove_dir_all(&root);
        }
    }
}
