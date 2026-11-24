//! A small, real Language Server Protocol client for workspace diagnostics.
//!
//! RAVENBOT does not bundle a language server. It detects common editors'
//! servers on `PATH`, speaks the standard LSP over stdio, opens the files a run
//! touched, and returns the published diagnostics. If no server is installed or
//! configured, the feature says so instead of pretending the code is clean.
//!
//! The protocol code is deliberately transport-generic. The production path
//! gives it a child process's stdin/stdout; tests give it an in-memory duplex
//! and a fake server, so framing, initialize, didOpen and publishDiagnostics are
//! exercised without depending on rust-analyzer or pyright being installed.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use ravenbot_db::queries::AppSettingsQueries;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
};
use tokio::process::Command;

/// Files larger than this are not sent to a language server.
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
/// Most diagnostics returned to a model in one response.
const MAX_DIAGNOSTICS: usize = 200;
/// Most files one check will open.
const MAX_FILES: usize = 20;

/// One language server the app knows how to start.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub language_id: String,
    /// File extensions (without a leading dot) this server handles.
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Optional LSP `initializationOptions`.
    #[serde(default)]
    pub initialization_options: Option<Value>,
}

pub type LspConfig = HashMap<String, ServerConfig>;

/// The default servers, tried only when their command is actually installed.
pub fn default_config() -> LspConfig {
    let mut config = LspConfig::new();
    config.insert(
        "rust".to_string(),
        ServerConfig {
            command: "rust-analyzer".to_string(),
            args: Vec::new(),
            language_id: "rust".to_string(),
            extensions: vec!["rs".to_string()],
            initialization_options: None,
        },
    );
    config.insert(
        "python".to_string(),
        ServerConfig {
            command: "pyright-langserver".to_string(),
            args: vec!["--stdio".to_string()],
            language_id: "python".to_string(),
            extensions: vec!["py".to_string(), "pyi".to_string()],
            initialization_options: None,
        },
    );
    config.insert(
        "typescript".to_string(),
        ServerConfig {
            command: "typescript-language-server".to_string(),
            args: vec!["--stdio".to_string()],
            language_id: "typescript".to_string(),
            extensions: vec![
                "ts".to_string(),
                "tsx".to_string(),
                "js".to_string(),
                "jsx".to_string(),
                "mjs".to_string(),
                "cjs".to_string(),
            ],
            initialization_options: None,
        },
    );
    config.insert(
        "go".to_string(),
        ServerConfig {
            command: "gopls".to_string(),
            args: Vec::new(),
            language_id: "go".to_string(),
            extensions: vec!["go".to_string()],
            initialization_options: None,
        },
    );
    config.insert(
        "c".to_string(),
        ServerConfig {
            command: "clangd".to_string(),
            args: Vec::new(),
            language_id: "c".to_string(),
            extensions: vec!["c".to_string(), "h".to_string()],
            initialization_options: None,
        },
    );
    config.insert(
        "cpp".to_string(),
        ServerConfig {
            command: "clangd".to_string(),
            args: Vec::new(),
            language_id: "cpp".to_string(),
            extensions: vec![
                "cc".to_string(),
                "cpp".to_string(),
                "cxx".to_string(),
                "hh".to_string(),
                "hpp".to_string(),
            ],
            initialization_options: None,
        },
    );
    config
}

/// Load `lsp_servers` from app settings, falling back to the defaults.
///
/// The setting replaces the defaults when present. An invalid value is ignored
/// rather than making every run fail before it starts.
pub async fn load_config(pool: &sqlx::SqlitePool) -> LspConfig {
    let Ok(raw) = AppSettingsQueries::get(pool, "lsp_servers").await else {
        return default_config();
    };
    let Some(raw) = raw.filter(|value| !value.trim().is_empty()) else {
        return default_config();
    };
    match serde_json::from_str::<LspConfig>(&raw) {
        Ok(config) if !config.is_empty() => config,
        Ok(_) => default_config(),
        Err(error) => {
            tracing::warn!(%error, "Invalid lsp_servers setting; using defaults");
            default_config()
        }
    }
}

pub fn timeout_from_env() -> Duration {
    std::env::var("RAVENBOT_LSP_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map(|seconds| Duration::from_secs(seconds.clamp(3, 60)))
        .unwrap_or(Duration::from_secs(12))
}

/// One diagnostic, flattened for a model or a UI.
#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub path: String,
    /// 1-based line.
    pub line: u64,
    /// 1-based column.
    pub column: u64,
    pub end_line: u64,
    pub end_column: u64,
    pub severity: String,
    pub message: String,
    pub source: Option<String>,
    pub code: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticsReport {
    pub server: String,
    pub root: String,
    pub checked: Vec<String>,
    pub diagnostics: Vec<Diagnostic>,
    pub errors: usize,
    pub warnings: usize,
    pub timed_out: bool,
    pub note: String,
}

impl DiagnosticsReport {
    /// A compact, model-readable failure summary. Empty means no errors.
    pub fn error_lines(&self, limit: usize) -> Vec<String> {
        self.diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == "error")
            .take(limit)
            .map(|diagnostic| {
                format!(
                    "{}:{}:{}: {}",
                    diagnostic.path, diagnostic.line, diagnostic.column, diagnostic.message
                )
            })
            .collect()
    }
}

/// A configured server that is installed and matches a workspace.
pub fn workspace_server(config: &LspConfig, roots: &[PathBuf]) -> Option<(String, ServerConfig)> {
    let mut wanted = HashSet::new();
    for root in roots.iter().take(8) {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten().take(300) {
            let name = entry.file_name().to_string_lossy().to_string();
            match name.as_str() {
                "Cargo.toml" => {
                    wanted.insert("rust");
                }
                "pyproject.toml" | "setup.py" | "requirements.txt" => {
                    wanted.insert("python");
                }
                "package.json" | "tsconfig.json" => {
                    wanted.insert("typescript");
                }
                "go.mod" => {
                    wanted.insert("go");
                }
                "compile_commands.json" | "CMakeLists.txt" => {
                    wanted.insert("c");
                    wanted.insert("cpp");
                }
                _ => {
                    let path = Path::new(&name);
                    let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
                        continue;
                    };
                    let extension = extension.to_ascii_lowercase();
                    for (key, server) in config {
                        if server
                            .extensions
                            .iter()
                            .any(|candidate| candidate.eq_ignore_ascii_case(&extension))
                        {
                            wanted.insert(key.as_str());
                        }
                    }
                }
            }
        }
    }
    // Prefer the explicit manifest language over a stray extension.
    for key in ["rust", "python", "typescript", "go", "cpp", "c"] {
        if !wanted.contains(key) {
            continue;
        }
        let Some(server) = config.get(key) else {
            continue;
        };
        if command_available(&server.command) {
            return Some((key.to_string(), server.clone()));
        }
    }
    None
}

/// Pick the configured server with the most matching extensions.
fn server_for_paths(config: &LspConfig, paths: &[PathBuf]) -> Option<(String, ServerConfig)> {
    let mut scores: HashMap<String, usize> = HashMap::new();
    for path in paths {
        let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
            continue;
        };
        let extension = extension.to_ascii_lowercase();
        for (key, server) in config {
            if server
                .extensions
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(&extension))
            {
                *scores.entry(key.clone()).or_default() += 1;
            }
        }
    }
    let (key, _) = scores
        .into_iter()
        .filter(|(key, _)| {
            config
                .get(key)
                .is_some_and(|server| command_available(&server.command))
        })
        .max_by_key(|(_, score)| *score)?;
    config.get(&key).map(|server| (key, server.clone()))
}

fn language_for(server: &ServerConfig, path: &Path) -> String {
    if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
        match extension.to_ascii_lowercase().as_str() {
            "tsx" => return "typescriptreact".to_string(),
            "jsx" => return "javascriptreact".to_string(),
            "js" | "mjs" | "cjs" => return "javascript".to_string(),
            "h" if server.language_id == "cpp" => return "cpp".to_string(),
            _ => {}
        }
    }
    server.language_id.clone()
}

fn command_available(command: &str) -> bool {
    if command.trim().is_empty() {
        return false;
    }
    let path = Path::new(command);
    if path.components().count() > 1 {
        return path.is_file();
    }
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|directory| {
        let candidate = directory.join(command);
        #[allow(unused_mut)]
        let mut found = candidate.is_file();
        #[cfg(windows)]
        {
            found = found
                || directory.join(format!("{command}.exe")).is_file()
                || directory.join(format!("{command}.cmd")).is_file();
        }
        found
    })
}

fn path_to_uri(path: &Path) -> Result<String, String> {
    url::Url::from_file_path(path)
        .map(|url| url.to_string())
        .map_err(|_| format!("Could not convert {} to a file URI.", path.display()))
}

fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let url = url::Url::parse(uri).ok()?;
    url.to_file_path().ok()
}

async fn resolve_files(
    roots: &[PathBuf],
    requested: &[String],
) -> Result<(Vec<PathBuf>, Vec<PathBuf>), String> {
    let mut canonical_roots = Vec::new();
    for root in roots {
        if let Ok(root) = tokio::fs::canonicalize(root).await {
            if root.is_dir() {
                canonical_roots.push(root);
            }
        }
    }
    if canonical_roots.is_empty() {
        return Err("The workspace folder does not exist.".to_string());
    }
    let mut files = Vec::new();
    let mut skipped = Vec::new();
    for raw in requested.iter().take(MAX_FILES) {
        let requested_path = Path::new(raw);
        let candidates: Vec<PathBuf> = if requested_path.is_absolute() {
            vec![requested_path.to_path_buf()]
        } else {
            canonical_roots
                .iter()
                .map(|root| root.join(requested_path))
                .collect()
        };
        let mut found = None;
        for candidate in candidates {
            let Ok(canonical) = tokio::fs::canonicalize(&candidate).await else {
                continue;
            };
            if canonical.is_file()
                && canonical_roots
                    .iter()
                    .any(|root| canonical.starts_with(root))
            {
                found = Some(canonical);
                break;
            }
        }
        match found {
            Some(path) => files.push(path),
            None => skipped.push(PathBuf::from(raw)),
        }
    }
    files.sort();
    files.dedup();
    if files.is_empty() {
        return Err("No existing workspace files were named for diagnostics.".to_string());
    }
    Ok((files, skipped))
}

async fn read_file_for_lsp(path: &Path) -> Result<String, String> {
    let meta = tokio::fs::metadata(path)
        .await
        .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(format!(
            "{} is too large to send to a language server.",
            path.display()
        ));
    }
    tokio::fs::read_to_string(path)
        .await
        .map_err(|error| format!("Could not read {}: {error}", path.display()))
}

/// Run the selected language server and return its published diagnostics.
pub async fn diagnose(
    config: &LspConfig,
    roots: &[PathBuf],
    requested: &[String],
    timeout: Duration,
) -> Result<DiagnosticsReport, String> {
    let (files, skipped) = resolve_files(roots, requested).await?;
    let (server_name, server) = server_for_paths(config, &files).ok_or_else(|| {
        format!(
            "No installed language server is configured for: {}",
            files
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;
    let mut opened = Vec::new();
    let mut skipped_files = skipped;
    for path in files {
        match read_file_for_lsp(&path).await {
            Ok(text) => {
                let language_id = language_for(&server, &path);
                opened.push((path, text, language_id));
            }
            Err(_) => skipped_files.push(path),
        }
    }
    if opened.is_empty() {
        return Err("None of the named files could be read for diagnostics.".to_string());
    }
    let root = roots
        .iter()
        .find(|root| opened[0].0.starts_with(root))
        .cloned()
        .or_else(|| roots.first().cloned())
        .ok_or_else(|| "The workspace root is missing.".to_string())?;
    let root = tokio::fs::canonicalize(&root).await.unwrap_or(root);
    let mut child = Command::new(&server.command)
        .args(&server.args)
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| {
            format!(
                "Could not start the {} language server '{}': {error}",
                server_name, server.command
            )
        })?;
    let Some(stdin) = child.stdin.take() else {
        return Err("The language server has no input pipe.".to_string());
    };
    let Some(stdout) = child.stdout.take() else {
        return Err("The language server has no output pipe.".to_string());
    };
    let result = tokio::time::timeout(
        timeout + Duration::from_secs(2),
        run_session(stdout, stdin, &root, &opened, &server, timeout),
    )
    .await;
    let _ = child.kill().await;
    match result {
        Err(_) => Err(format!(
            "The {} language server did not finish within {}s.",
            server_name,
            timeout.as_secs()
        )),
        Ok(inner) => {
            let mut report = inner?;
            if !skipped_files.is_empty() {
                report.note.push_str(&format!(
                    " {} file(s) were skipped because they were missing, too large, or not text.",
                    skipped_files.len()
                ));
            }
            Ok(report)
        }
    }
}

/// Run an LSP session over any reader/writer pair.
async fn run_session<R, W>(
    reader: R,
    mut writer: W,
    root: &Path,
    files: &[(PathBuf, String, String)],
    server: &ServerConfig,
    timeout: Duration,
) -> Result<DiagnosticsReport, String>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut reader = BufReader::new(reader);
    let deadline = Instant::now() + timeout;
    let root_uri = path_to_uri(root)?;
    let root_name = root
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("workspace");
    let mut initialize_params = json!({
        "processId": std::process::id(),
        "clientInfo": { "name": "RAVENBOT", "version": env!("CARGO_PKG_VERSION") },
        "rootPath": root.display().to_string(),
        "rootUri": &root_uri,
        "capabilities": {
            "textDocument": { "publishDiagnostics": { "relatedInformation": false } },
            "workspace": { "workspaceFolders": true, "configuration": true }
        },
        "workspaceFolders": [{ "uri": &root_uri, "name": root_name }]
    });
    if let Some(options) = &server.initialization_options {
        initialize_params["initializationOptions"] = options.clone();
    }
    write_frame(
        &mut writer,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": initialize_params
        }),
    )
    .await?;

    // Wait for initialize. Servers may ask configuration questions first.
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(
                "The language server did not answer initialize before the timeout.".to_string(),
            );
        }
        let message = match tokio::time::timeout(remaining, read_frame(&mut reader)).await {
            Err(_) => {
                return Err(
                    "The language server did not answer initialize before the timeout.".to_string(),
                )
            }
            Ok(Err(error)) => return Err(error),
            Ok(Ok(message)) => message,
        };
        if message.get("id") == Some(&json!(1)) && message.get("method").is_none() {
            if let Some(error) = message.get("error") {
                return Err(format!(
                    "The language server rejected initialize: {}",
                    error
                ));
            }
            break;
        }
        respond_to_server_request(&mut writer, &message, &root_uri, root_name).await?;
    }

    write_frame(
        &mut writer,
        &json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }),
    )
    .await?;
    write_frame(
        &mut writer,
        &json!({
            "jsonrpc": "2.0",
            "method": "workspace/didChangeConfiguration",
            "params": { "settings": {} }
        }),
    )
    .await?;

    let mut opened: HashSet<PathBuf> = HashSet::new();
    for (path, text, language_id) in files {
        let uri = path_to_uri(path)?;
        opened.insert(path.clone());
        write_frame(
            &mut writer,
            &json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didOpen",
                "params": {
                    "textDocument": {
                        "uri": uri,
                        "languageId": language_id,
                        "version": 1,
                        "text": text
                    }
                }
            }),
        )
        .await?;
    }

    let mut received: HashSet<PathBuf> = HashSet::new();
    let mut by_path: HashMap<PathBuf, Vec<Diagnostic>> = HashMap::new();
    let mut timed_out = false;
    let mut quiet_until: Option<Instant> = None;
    loop {
        let now = Instant::now();
        let stopping_at = quiet_until.unwrap_or(deadline);
        if now >= deadline {
            timed_out = true;
            break;
        }
        if now >= stopping_at {
            break;
        }
        let remaining = stopping_at.min(deadline).saturating_duration_since(now);
        let message = match tokio::time::timeout(remaining, read_frame(&mut reader)).await {
            Err(_) => {
                if received.len() < opened.len() {
                    timed_out = true;
                }
                break;
            }
            Ok(Err(error)) => {
                if received.is_empty() {
                    return Err(error);
                }
                timed_out = true;
                break;
            }
            Ok(Ok(message)) => message,
        };
        if message.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics")
        {
            if let Some(params) = message.get("params") {
                if let Some((path, diagnostics)) = parse_publish(params).await {
                    if opened.contains(&path) {
                        received.insert(path.clone());
                        by_path.insert(path, diagnostics);
                        if received.len() == opened.len() && quiet_until.is_none() {
                            quiet_until = Some(Instant::now() + Duration::from_millis(300));
                        }
                    }
                }
            }
            continue;
        }
        respond_to_server_request(&mut writer, &message, &root_uri, root_name).await?;
    }

    let _ = write_frame(
        &mut writer,
        &json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown", "params": {} }),
    )
    .await;
    let _ = write_frame(
        &mut writer,
        &json!({ "jsonrpc": "2.0", "method": "exit", "params": {} }),
    )
    .await;

    // The cap must not hide errors behind a wall of warnings: keep errors
    // first, then present the kept set in file/line order.
    let mut diagnostics: Vec<Diagnostic> = by_path.into_values().flatten().collect();
    diagnostics.sort_by_key(|diagnostic| match diagnostic.severity.as_str() {
        "error" => 0,
        "warning" => 1,
        _ => 2,
    });
    diagnostics.truncate(MAX_DIAGNOSTICS);
    diagnostics.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then(a.line.cmp(&b.line))
            .then(a.column.cmp(&b.column))
    });
    let errors = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == "error")
        .count();
    let warnings = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == "warning")
        .count();
    let note = if timed_out {
        "The language server did not publish diagnostics for every opened file before the timeout."
            .to_string()
    } else if diagnostics.is_empty() {
        "No diagnostics were reported.".to_string()
    } else {
        String::new()
    };
    Ok(DiagnosticsReport {
        server: server.command.clone(),
        root: root.display().to_string(),
        checked: opened
            .into_iter()
            .map(|path| path.display().to_string())
            .collect(),
        diagnostics,
        errors,
        warnings,
        timed_out,
        note,
    })
}

async fn respond_to_server_request<W: AsyncWrite + Unpin>(
    writer: &mut W,
    message: &Value,
    root_uri: &str,
    root_name: &str,
) -> Result<(), String> {
    let Some(id) = message.get("id") else {
        return Ok(());
    };
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        return Ok(());
    };
    let result = match method {
        "workspace/configuration" => {
            let count = message
                .get("params")
                .and_then(|params| params.get("items"))
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0);
            Value::Array(vec![Value::Null; count])
        }
        "workspace/workspaceFolders" => {
            json!([{ "uri": root_uri, "name": root_name }])
        }
        _ => Value::Null,
    };
    write_frame(
        writer,
        &json!({ "jsonrpc": "2.0", "id": id, "result": result }),
    )
    .await
}

async fn parse_publish(params: &Value) -> Option<(PathBuf, Vec<Diagnostic>)> {
    let uri = params.get("uri")?.as_str()?;
    let path = uri_to_path(uri)?;
    let path = tokio::fs::canonicalize(&path).await.unwrap_or(path);
    let mut diagnostics = Vec::new();
    for item in params.get("diagnostics")?.as_array()? {
        let Some(message) = item.get("message").and_then(Value::as_str) else {
            continue;
        };
        let start = item.get("range").and_then(|range| range.get("start"));
        let end = item.get("range").and_then(|range| range.get("end"));
        let line = start
            .and_then(|value| value.get("line"))
            .and_then(Value::as_u64)
            .unwrap_or(0)
            + 1;
        let column = start
            .and_then(|value| value.get("character"))
            .and_then(Value::as_u64)
            .unwrap_or(0)
            + 1;
        let end_line = end
            .and_then(|value| value.get("line"))
            .and_then(Value::as_u64)
            .unwrap_or(line.saturating_sub(1))
            + 1;
        let end_column = end
            .and_then(|value| value.get("character"))
            .and_then(Value::as_u64)
            .unwrap_or(column.saturating_sub(1))
            + 1;
        let severity = match item.get("severity").and_then(Value::as_u64) {
            Some(1) => "error",
            Some(2) => "warning",
            Some(4) => "hint",
            _ => "information",
        };
        diagnostics.push(Diagnostic {
            path: path.display().to_string(),
            line,
            column,
            end_line,
            end_column,
            severity: severity.to_string(),
            message: message.trim().to_string(),
            source: item
                .get("source")
                .and_then(Value::as_str)
                .map(str::to_string),
            code: item.get("code").map(|code| match code {
                Value::String(value) => value.clone(),
                other => other.to_string(),
            }),
        });
    }
    Some((path, diagnostics))
}

async fn write_frame<W: AsyncWrite + Unpin>(writer: &mut W, value: &Value) -> Result<(), String> {
    let body = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    writer
        .write_all(header.as_bytes())
        .await
        .map_err(|error| error.to_string())?;
    writer
        .write_all(&body)
        .await
        .map_err(|error| error.to_string())?;
    writer.flush().await.map_err(|error| error.to_string())
}

async fn read_frame<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<Value, String> {
    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        let read = reader
            .read_line(&mut line)
            .await
            .map_err(|error| error.to_string())?;
        if read == 0 {
            return Err("The language server closed its output.".to_string());
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            content_length = value.trim().parse::<usize>().ok();
        }
    }
    let length = content_length
        .ok_or_else(|| "The language server sent a frame without Content-Length.".to_string())?;
    if length > 8 * 1024 * 1024 {
        return Err("The language server sent an oversized frame.".to_string());
    }
    let mut body = vec![0u8; length];
    reader
        .read_exact(&mut body)
        .await
        .map_err(|error| error.to_string())?;
    serde_json::from_slice(&body).map_err(|error| format!("Invalid LSP JSON: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;
    use uuid::Uuid;

    #[test]
    fn the_default_config_has_the_expected_extensions() {
        let config = default_config();
        assert!(config["rust"].extensions.contains(&"rs".to_string()));
        assert!(config["typescript"].extensions.contains(&"tsx".to_string()));
        assert_eq!(config["python"].args, vec!["--stdio".to_string()]);
    }

    #[test]
    fn a_config_extension_chooses_the_server_for_changed_files() {
        let config = default_config();
        let files = vec![PathBuf::from("src/main.rs")];
        // The default rust server is only selected when its command exists.
        // If rust-analyzer is not installed this assertion is not meaningful.
        if command_available("rust-analyzer") {
            let (name, _) = server_for_paths(&config, &files).expect("rust server");
            assert_eq!(name, "rust");
        }
        assert!(config["python"].extensions.contains(&"py".to_string()));
    }

    #[test]
    fn error_lines_are_capped_and_only_include_errors() {
        let report = DiagnosticsReport {
            server: "fake".to_string(),
            root: "/tmp".to_string(),
            checked: vec!["a.rs".to_string()],
            diagnostics: vec![
                Diagnostic {
                    path: "/tmp/a.rs".to_string(),
                    line: 2,
                    column: 5,
                    end_line: 2,
                    end_column: 9,
                    severity: "error".to_string(),
                    message: "mismatched types".to_string(),
                    source: Some("fake".to_string()),
                    code: None,
                },
                Diagnostic {
                    path: "/tmp/a.rs".to_string(),
                    line: 4,
                    column: 1,
                    end_line: 4,
                    end_column: 2,
                    severity: "warning".to_string(),
                    message: "unused import".to_string(),
                    source: Some("fake".to_string()),
                    code: None,
                },
            ],
            errors: 1,
            warnings: 1,
            timed_out: false,
            note: String::new(),
        };
        assert_eq!(
            report.error_lines(5),
            vec!["/tmp/a.rs:2:5: mismatched types".to_string()]
        );
        assert!(report.error_lines(0).is_empty());
    }

    #[test]
    fn a_workspace_manifest_selects_an_installed_server() {
        let dir = std::env::temp_dir().join(format!("ravenbot-lsp-detect-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        let fake = dir.join("rust-analyzer");
        std::fs::write(&fake, "").unwrap();
        let mut config = default_config();
        config.get_mut("rust").unwrap().command = fake.display().to_string();
        let found = workspace_server(&config, std::slice::from_ref(&dir)).expect("rust server");
        assert_eq!(found.0, "rust");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn frames_round_trip_over_a_duplex() {
        let (mut client, server) = duplex(64 * 1024);
        let message = json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} });
        write_frame(&mut client, &message).await.expect("write");
        drop(client);
        let mut reader = BufReader::new(server);
        let got = read_frame(&mut reader).await.expect("read");
        assert_eq!(got["method"], "initialized");
    }

    #[tokio::test]
    async fn a_publish_notification_becomes_a_typed_diagnostic() {
        let dir = std::env::temp_dir().join(format!("ravenbot-lsp-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("main.rs");
        std::fs::write(&file, "fn main() {}\n").unwrap();
        let uri = path_to_uri(&file).expect("uri");
        let params = json!({
            "uri": uri,
            "diagnostics": [{
                "range": {
                    "start": { "line": 0, "character": 3 },
                    "end": { "line": 0, "character": 7 }
                },
                "severity": 1,
                "message": "mismatched types",
                "source": "rust-analyzer"
            }]
        });
        let (path, diagnostics) = parse_publish(&params).await.expect("publish");
        assert_eq!(path, tokio::fs::canonicalize(&file).await.unwrap());
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, 1);
        assert_eq!(diagnostics[0].column, 4);
        assert_eq!(diagnostics[0].severity, "error");
        assert_eq!(diagnostics[0].message, "mismatched types");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    #[ignore = "requires clangd on PATH; validates a real language server"]
    async fn clangd_reports_a_real_error() {
        if !command_available("clangd") {
            return;
        }
        let dir = std::env::temp_dir().join(format!("ravenbot-lsp-clangd-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("main.c"),
            "int main(void) { return missing_symbol; }\n",
        )
        .unwrap();
        let mut config = LspConfig::new();
        config.insert(
            "c".to_string(),
            ServerConfig {
                command: "clangd".to_string(),
                args: Vec::new(),
                language_id: "c".to_string(),
                extensions: vec!["c".to_string()],
                initialization_options: None,
            },
        );
        let report = diagnose(
            &config,
            std::slice::from_ref(&dir),
            &["main.c".to_string()],
            Duration::from_secs(20),
        )
        .await
        .expect("clangd diagnostics");
        assert!(
            report.errors > 0,
            "clangd should report the missing symbol: {report:?}"
        );
        assert!(
            report.diagnostics.iter().any(|diagnostic| diagnostic
                .message
                .contains("missing_symbol")
                || diagnostic
                    .message
                    .to_ascii_lowercase()
                    .contains("undeclared")),
            "{report:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_full_session_initializes_opens_and_collects() {
        let dir = std::env::temp_dir().join(format!("ravenbot-lsp-session-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("main.rs");
        std::fs::write(&file, "fn main() { missing(); }\n").unwrap();
        let uri = path_to_uri(&file).expect("uri");
        let file_text = std::fs::read_to_string(&file).unwrap();

        let (client_reader, server_writer) = duplex(64 * 1024);
        let (server_reader, client_writer) = duplex(64 * 1024);
        let fake_uri = uri.clone();
        let fake = tokio::spawn(async move {
            let mut reader = BufReader::new(server_reader);
            let mut writer = server_writer;
            let initialize = read_frame(&mut reader).await.expect("initialize");
            assert_eq!(initialize["method"], "initialize");
            write_frame(
                &mut writer,
                &json!({
                    "jsonrpc": "2.0",
                    "id": initialize["id"],
                    "result": { "capabilities": {} }
                }),
            )
            .await
            .expect("initialize response");
            let _initialized = read_frame(&mut reader).await.expect("initialized");
            let _configuration = read_frame(&mut reader).await.expect("configuration");
            let did_open = read_frame(&mut reader).await.expect("didOpen");
            assert_eq!(did_open["method"], "textDocument/didOpen");
            write_frame(
                &mut writer,
                &json!({
                    "jsonrpc": "2.0",
                    "method": "textDocument/publishDiagnostics",
                    "params": {
                        "uri": fake_uri,
                        "diagnostics": [{
                            "range": {
                                "start": { "line": 0, "character": 11 },
                                "end": { "line": 0, "character": 20 }
                            },
                            "severity": 1,
                            "message": "cannot find function `missing`",
                            "source": "fake-lsp"
                        }]
                    }
                }),
            )
            .await
            .expect("publish");
            let _shutdown = read_frame(&mut reader).await.expect("shutdown");
            let _exit = read_frame(&mut reader).await.expect("exit");
        });

        let server = ServerConfig {
            command: "fake-lsp".to_string(),
            args: Vec::new(),
            language_id: "rust".to_string(),
            extensions: vec!["rs".to_string()],
            initialization_options: None,
        };
        let report = run_session(
            client_reader,
            client_writer,
            &dir,
            &[(file.clone(), file_text, "rust".to_string())],
            &server,
            Duration::from_secs(5),
        )
        .await
        .expect("session");
        assert_eq!(report.errors, 1);
        assert_eq!(
            report.diagnostics[0].message,
            "cannot find function `missing`"
        );
        assert!(!report.timed_out);
        fake.await.expect("fake server");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
