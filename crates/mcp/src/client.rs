use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub server_id: String,
}

/// Default budgets, overridable via env for slow machines / large npx installs.
fn timeout_from_env(key: &str, default_secs: u64) -> Duration {
    std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(default_secs))
}

fn init_timeout() -> Duration {
    timeout_from_env("RAVENBOT_MCP_INIT_TIMEOUT_SECS", 25)
}
fn list_timeout() -> Duration {
    timeout_from_env("RAVENBOT_MCP_LIST_TIMEOUT_SECS", 25)
}
fn call_timeout() -> Duration {
    timeout_from_env("RAVENBOT_MCP_CALL_TIMEOUT_SECS", 180)
}
fn http_timeout() -> Duration {
    timeout_from_env("RAVENBOT_MCP_HTTP_TIMEOUT_SECS", 60)
}

/// A live stdio MCP session. `kill_on_drop` guarantees the child is reaped on
/// every early-return path, which is what previously leaked `npx`/node processes
/// (and their RAM) whenever initialization timed out.
struct StdioSession {
    child: Child,
    stdin: ChildStdin,
    reader: Lines<BufReader<ChildStdout>>,
    stderr_tail: Arc<Mutex<String>>,
}

impl StdioSession {
    /// Best-effort stderr excerpt for actionable errors.
    fn stderr(&self) -> String {
        let buf = self
            .stderr_tail
            .lock()
            .map(|b| b.clone())
            .unwrap_or_default();
        let buf = buf.trim();
        if buf.is_empty() {
            return String::new();
        }
        let lines: Vec<&str> = buf.lines().rev().take(6).collect();
        let joined = lines.into_iter().rev().collect::<Vec<_>>().join(" | ");
        format!(" — server stderr: {}", joined)
    }

    /// Send one JSON-RPC request and wait for the response with the matching id.
    /// Notifications, log lines, and unrelated responses are skipped.
    async fn request(
        &mut self,
        method: &str,
        params: Value,
        id: u64,
        timeout: Duration,
    ) -> Result<Value, String> {
        let req = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        self.stdin
            .write_all(format!("{}\n", req).as_bytes())
            .await
            .map_err(|e| format!("{}: failed to write request: {}", method, e))?;
        self.stdin
            .flush()
            .await
            .map_err(|e| format!("{}: failed to flush request: {}", method, e))?;

        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Err(format!(
                    "{} timed out after {:?}{}",
                    method,
                    timeout,
                    self.stderr()
                ));
            }
            let line = match tokio::time::timeout(remaining, self.reader.next_line()).await {
                Ok(Ok(Some(line))) => line,
                Ok(Ok(None)) => {
                    return Err(format!(
                        "{}: server closed stdout before responding{}",
                        method,
                        self.stderr()
                    ))
                }
                Ok(Err(e)) => {
                    return Err(format!("{}: read error: {}{}", method, e, self.stderr()))
                }
                Err(_) => {
                    return Err(format!(
                        "{} timed out after {:?}{}",
                        method,
                        timeout,
                        self.stderr()
                    ))
                }
            };
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            // Servers occasionally print non-JSON diagnostics on stdout; ignore.
            let msg: Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            // Notifications carry no id.
            if msg.get("id").is_none() {
                continue;
            }
            if msg.get("id").and_then(|v| v.as_u64()) != Some(id) {
                continue;
            }
            if let Some(err) = msg.get("error") {
                return Err(format!("{} error: {}{}", method, err, self.stderr()));
            }
            return Ok(msg.get("result").cloned().unwrap_or(Value::Null));
        }
    }
}

async fn drain_stderr(mut stderr: ChildStderr, sink: Arc<Mutex<String>>) {
    let mut reader = BufReader::new(&mut stderr);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let trimmed = line.trim_end();
                if !trimmed.is_empty() {
                    tracing::trace!(target: "ravenbot_mcp", "stderr: {}", trimmed);
                }
                if let Ok(mut buf) = sink.lock() {
                    // Keep only the most recent output so the buffer stays bounded.
                    if buf.len() > 8192 {
                        let cut = buf.len() - 4096;
                        buf.drain(..cut);
                    }
                    buf.push_str(&line);
                }
            }
        }
    }
}

pub struct McpClient {
    pub server_id: String,
    pub config: crate::servers::McpServerConfig,
    pub env: HashMap<String, String>,
}

impl McpClient {
    pub fn new(config: crate::servers::McpServerConfig) -> Self {
        let mut env = HashMap::new();
        for key in &config.env_keys {
            if let Ok(val) = std::env::var(key) {
                if !val.is_empty() {
                    env.insert(key.clone(), val);
                }
            }
        }
        Self { server_id: config.id.clone(), config, env }
    }

    pub fn with_env(config: crate::servers::McpServerConfig, env: HashMap<String, String>) -> Self {
        Self { server_id: config.id.clone(), config, env }
    }

    fn is_builtin(&self) -> bool {
        self.config.command == "ravenbot-builtin" || self.config.command == "builtin"
    }

    /// Global offline switch: never spawn external servers, use synthesized
    /// tool descriptors only (used by tests and air-gapped installs).
    fn offline_mode() -> bool {
        matches!(
            std::env::var("RAVENBOT_MCP_OFFLINE").as_deref(),
            Ok("1") | Ok("true")
        )
    }

    /// Explicit URL for a Cursor-style HTTP (streamable/SSE) MCP server.
    fn http_url(&self) -> Option<String> {
        self.config
            .url
            .clone()
            .filter(|u| !u.trim().is_empty())
    }

    fn uses_http(&self) -> bool {
        self.config.transport == crate::servers::McpTransport::Http || self.http_url().is_some()
    }

    /// List tools — tries the real MCP first; falls back to synthesized
    /// descriptors only when the server cannot be reached.
    pub async fn list_tools(&self) -> Result<Vec<McpTool>, String> {
        match self.list_tools_real().await {
            Ok(tools) if !tools.is_empty() => Ok(tools),
            Ok(_) => Ok(self.synthesized_tools()),
            Err(e) => {
                tracing::debug!(
                    server = %self.server_id,
                    error = %e,
                    "MCP tools/list falling back to synthesized descriptors"
                );
                Ok(self.synthesized_tools())
            }
        }
    }

    async fn list_tools_real(&self) -> Result<Vec<McpTool>, String> {
        if self.is_builtin() {
            return Ok(self.synthesized_tools());
        }
        if Self::offline_mode() {
            return Err("MCP offline mode enabled (RAVENBOT_MCP_OFFLINE)".to_string());
        }
        if self.uses_http() {
            return self.list_tools_http().await;
        }
        self.list_tools_stdio().await
    }

    async fn spawn_stdio(&self) -> Result<StdioSession, String> {
        if self.config.command.trim().is_empty() {
            return Err(format!("Server '{}' has no command configured", self.server_id));
        }
        let mut child = Command::new(&self.config.command)
            .args(&self.config.args)
            .envs(&self.env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                format!(
                    "Failed to spawn `{} {}`: {}",
                    self.config.command,
                    self.config.args.join(" "),
                    e
                )
            })?;

        let stdin = child.stdin.take().ok_or("spawned process has no stdin")?;
        let stdout = child.stdout.take().ok_or("spawned process has no stdout")?;
        let stderr = child.stderr.take().ok_or("spawned process has no stderr")?;
        let stderr_tail = Arc::new(Mutex::new(String::new()));
        tokio::spawn(drain_stderr(stderr, stderr_tail.clone()));

        Ok(StdioSession {
            child,
            stdin,
            reader: BufReader::new(stdout).lines(),
            stderr_tail,
        })
    }

    async fn initialize(&self, session: &mut StdioSession) -> Result<(), String> {
        session
            .request(
                "initialize",
                json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {"name": "ravenbot", "version": env!("CARGO_PKG_VERSION")}
                }),
                1,
                init_timeout(),
            )
            .await?;

        // Required MCP handshake notification (no response expected).
        let notif = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
        session
            .stdin
            .write_all(format!("{}\n", notif).as_bytes())
            .await
            .map_err(|e| format!("failed to send initialized notification: {}", e))?;
        session
            .stdin
            .flush()
            .await
            .map_err(|e| format!("failed to flush initialized notification: {}", e))?;
        Ok(())
    }

    async fn list_tools_stdio(&self) -> Result<Vec<McpTool>, String> {
        let mut session = self.spawn_stdio().await?;
        self.initialize(&mut session).await?;
        let result = session.request("tools/list", json!({}), 2, list_timeout()).await;
        let _ = session.child.kill().await;
        let result = result?;

        let tools_val = result
            .get("tools")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let mut tools = Vec::new();
        for t in &tools_val {
            let name = t.get("name").and_then(|v| v.as_str()).unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            let description = t
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let input_schema = t
                .get("inputSchema")
                .cloned()
                .unwrap_or_else(|| json!({"type": "object"}));
            tools.push(McpTool {
                name: name.to_string(),
                description,
                input_schema,
                server_id: self.server_id.clone(),
            });
        }
        Ok(tools)
    }

    async fn list_tools_http(&self) -> Result<Vec<McpTool>, String> {
        self.http_initialize().await?;
        let result = self.http_rpc("tools/list", json!({}), 2).await?;
        let tools_val = result
            .get("tools")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let mut tools = Vec::new();
        for t in &tools_val {
            let name = t.get("name").and_then(|v| v.as_str()).unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            let description = t
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let input_schema = t
                .get("inputSchema")
                .cloned()
                .unwrap_or_else(|| json!({"type": "object"}));
            tools.push(McpTool {
                name: name.to_string(),
                description,
                input_schema,
                server_id: self.server_id.clone(),
            });
        }
        Ok(tools)
    }

    fn resolve_headers(&self) -> HashMap<String, String> {
        self.config
            .headers
            .iter()
            .map(|(k, v)| {
                let mut out = v.clone();
                for (ek, ev) in &self.env {
                    out = out.replace(&format!("${{{}}}", ek), ev);
                }
                (k.clone(), out)
            })
            .collect()
    }

    async fn http_initialize(&self) -> Result<(), String> {
        self.http_rpc(
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "ravenbot", "version": env!("CARGO_PKG_VERSION")}
            }),
            1,
        )
        .await?;
        // Notifications are fire-and-forget; best effort.
        let _ = self
            .http_post(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
            .await;
        Ok(())
    }

    async fn http_post(&self, body: Value) -> Result<reqwest::Response, String> {
        let url = self.http_url().ok_or("HTTP MCP server is missing a url")?;
        let mut req = reqwest::Client::builder()
            .timeout(http_timeout())
            .build()
            .map_err(|e| e.to_string())?
            .post(&url)
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .json(&body);
        for (k, v) in self.resolve_headers() {
            req = req.header(k, v);
        }
        req.send()
            .await
            .map_err(|e| format!("HTTP {} request failed: {}", url, e))
    }

    async fn http_rpc(&self, method: &str, params: Value, id: u64) -> Result<Value, String> {
        let body = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        let resp = self.http_post(body).await?;
        let status = resp.status();
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let text = resp
            .text()
            .await
            .map_err(|e| format!("failed to read HTTP response: {}", e))?;
        if !status.is_success() {
            return Err(format!(
                "HTTP {} returned {}: {}",
                method,
                status,
                text.chars().take(300).collect::<String>()
            ));
        }

        let msg: Value = if content_type.contains("text/event-stream") {
            text.lines()
                .filter_map(|l| l.strip_prefix("data:"))
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .filter_map(|l| serde_json::from_str::<Value>(l).ok())
                .find(|v| v.get("id").and_then(|i| i.as_u64()) == Some(id))
                .ok_or_else(|| format!("HTTP {}: no matching response in event stream", method))?
        } else {
            serde_json::from_str(&text).map_err(|e| {
                format!(
                    "HTTP {} returned invalid JSON: {} — body: {}",
                    method,
                    e,
                    text.chars().take(300).collect::<String>()
                )
            })?
        };

        if let Some(err) = msg.get("error") {
            return Err(format!("HTTP {} error: {}", method, err));
        }
        Ok(msg.get("result").cloned().unwrap_or(Value::Null))
    }

    pub fn synthesized_tools(&self) -> Vec<McpTool> {
        let tools: Vec<(String, String)> = match self.server_id.as_str() {
            "github" => vec![
                ("github_list_repos".into(), "List repositories and organizations".into()),
                ("github_create_issue".into(), "Create and manage GitHub issues".into()),
                ("github_create_pr".into(), "Open and review pull requests".into()),
                ("github_search_code".into(), "Semantic and literal code search across repos".into()),
            ],
            "gitlab" => vec![
                ("gitlab_list_projects".into(), "List accessible GitLab projects".into()),
                ("gitlab_create_issue".into(), "Create GitLab issues and track milestones".into()),
                ("gitlab_merge_request".into(), "Create and manage Merge Requests".into()),
            ],
            "postgres" => vec![
                ("pg_query".into(), "Execute read/write PostgreSQL queries".into()),
                ("pg_list_tables".into(), "Inspect database schema and tables".into()),
                ("pg_describe_table".into(), "Get column definitions and indexes".into()),
            ],
            "sqlite" => vec![
                ("sqlite_query".into(), "Run query on local SQLite database".into()),
                ("sqlite_list_tables".into(), "List tables and views".into()),
            ],
            "duckdb" => vec![
                ("duckdb_query".into(), "Run analytical SQL (Parquet, CSV, Arrow)".into()),
                ("duckdb_execute".into(), "Execute DuckDB SQL statements".into()),
            ],
            "fetch" => vec![("fetch_url".into(), "Fetch raw HTML/text from any HTTP/HTTPS URL".into())],
            "browserbase" | "puppeteer" | "playwright" => vec![
                ("browser_navigate".into(), "Navigate to web page".into()),
                ("browser_click".into(), "Click elements and interact".into()),
                ("browser_screenshot".into(), "Capture page screenshot".into()),
            ],
            "filesystem" => vec![
                ("read_file".into(), "Read the complete contents of a file".into()),
                ("write_file".into(), "Create a new file or completely overwrite an existing file".into()),
                ("list_directory".into(), "Get a detailed listing of all files and directories in a specific location".into()),
                ("search_files".into(), "Recursively search for files and directories matching a pattern".into()),
            ],
            "git" => vec![
                ("git_status".into(), "Show working tree status".into()),
                ("git_diff".into(), "Show changes".into()),
                ("git_log".into(), "Show commit logs".into()),
            ],
            "docker" => vec![
                ("docker_ps".into(), "List running Docker containers".into()),
                ("docker_logs".into(), "Inspect container stdout/stderr logs".into()),
            ],
            "slack" => vec![
                ("slack_send_message".into(), "Send message to Slack channel".into()),
                ("slack_read_channel".into(), "Read recent channel messages".into()),
            ],
            "notion" => vec![
                ("notion_search".into(), "Search Notion workspace pages and databases".into()),
                ("notion_create_page".into(), "Create Notion document".into()),
            ],
            "stripe" => vec![
                ("stripe_list_charges".into(), "List customer charges and balances".into()),
                ("stripe_create_customer".into(), "Create new Stripe customer".into()),
            ],
            "pinecone" | "qdrant" | "weaviate" | "chroma" => vec![
                ("vector_search".into(), "Query nearest neighbors with vector embeddings".into()),
                ("vector_upsert".into(), "Insert and index vector embeddings".into()),
            ],
            _ => {
                let clean_id = self.server_id.replace('-', "_");
                vec![
                    (format!("{}_execute", clean_id), format!("Execute operations on {}", self.config.name)),
                    (format!("{}_query", clean_id), format!("Query data from {}", self.config.name)),
                ]
            }
        };
        tools
            .into_iter()
            .map(|(n, d)| McpTool {
                name: n,
                description: d,
                input_schema: json!({"type": "object", "properties": {"input": {"type": "string"}}}),
                server_id: self.server_id.clone(),
            })
            .collect()
    }

    /// Probe a server and return an honest result. Unlike `list_tools`, this
    /// surfaces the real transport error so the UI can show what went wrong.
    pub async fn test_connection(&self) -> Result<crate::servers::McpTestResult, String> {
        let start = std::time::Instant::now();
        let has_keys = if self.config.env_keys.is_empty() {
            true
        } else {
            self.config
                .env_keys
                .iter()
                .all(|k| self.env.contains_key(k) && !self.env[k].is_empty())
        };

        if self.is_builtin() {
            return self.test_builtin().await;
        }

        let (success, tools, message) = match self.list_tools_real().await {
            Ok(tools) if !tools.is_empty() => {
                let latency = start.elapsed().as_millis() as u64;
                let count = tools.len();
                (
                    true,
                    tools,
                    format!(
                        "Live MCP server '{}' — discovered {} tool(s) in {}ms.",
                        self.config.name, count, latency
                    ),
                )
            }
            Ok(_) => (
                false,
                self.synthesized_tools(),
                format!(
                    "Server '{}' responded but exposed no tools.",
                    self.config.name
                ),
            ),
            Err(e) => {
                tracing::warn!(server = %self.server_id, error = %e, "MCP connection test failed");
                (
                    false,
                    self.synthesized_tools(),
                    if has_keys {
                        format!("Could not reach '{}': {}", self.config.name, e)
                    } else {
                        format!(
                            "Could not reach '{}': {} (missing env {:?} — add it in Settings → MCP → Env)",
                            self.config.name, e, self.config.env_keys
                        )
                    },
                )
            }
        };

        let latency_ms = start.elapsed().as_millis().max(1) as u64;
        Ok(crate::servers::McpTestResult {
            success,
            server_id: self.server_id.clone(),
            message,
            latency_ms,
            tools,
        })
    }

    pub async fn call_tool(&self, tool_name: &str, args: Value) -> Result<Value, String> {
        // Honest behaviour: never fabricate a success. If the real server
        // cannot be reached or rejects the call, surface a clear error so the
        // agent (and the user) know the connector did NOT run. The previous
        // synthesized fallback returned `status: "unavailable"`/fake success
        // which made the model believe work had been done.
        self.call_tool_real(tool_name, args.clone()).await.map_err(|e| {
            tracing::warn!(
                server = %self.server_id,
                tool = %tool_name,
                error = %e,
                "MCP tool call failed"
            );
            format!(
                "MCP connector '{}' could not run '{}': {}. Check the connector's command/URL and credentials in Settings → Connectors.",
                self.server_id, tool_name, e
            )
        })
    }

    async fn call_tool_real(&self, tool_name: &str, args: Value) -> Result<Value, String> {
        if self.is_builtin() {
            return self.call_tool_builtin(tool_name, args).await;
        }
        if Self::offline_mode() {
            return Err("MCP offline mode enabled (RAVENBOT_MCP_OFFLINE)".to_string());
        }
        if self.uses_http() {
            return self.call_tool_http(tool_name, args).await;
        }
        self.call_tool_stdio(tool_name, args).await
    }

    async fn call_tool_stdio(&self, tool_name: &str, args: Value) -> Result<Value, String> {
        let mut session = self.spawn_stdio().await?;
        self.initialize(&mut session).await?;
        let result = session
            .request(
                "tools/call",
                json!({"name": tool_name, "arguments": args}),
                2,
                call_timeout(),
            )
            .await;
        let _ = session.child.kill().await;
        unwrap_tool_result(result?)
    }

    async fn call_tool_http(&self, tool_name: &str, args: Value) -> Result<Value, String> {
        self.http_initialize().await?;
        let result = self
            .http_rpc(
                "tools/call",
                json!({"name": tool_name, "arguments": args}),
                2,
            )
            .await?;
        unwrap_tool_result(result)
    }

    /// Built-in servers run in-process (no child binary to install). `duckdb`
    /// is backed by the real `duckdb` CLI when present. Any other server that
    /// declares the built-in command is genuinely unimplemented, so it errors
    /// instead of pretending to succeed.
    async fn call_tool_builtin(&self, tool_name: &str, args: Value) -> Result<Value, String> {
        match self.server_id.as_str() {
            "duckdb" => duckdb_call(tool_name, &args).await,
            other => Err(format!(
                "Built-in connector '{}' is not implemented, so '{}' was not executed.",
                other, tool_name
            )),
        }
    }

    async fn test_builtin(&self) -> Result<crate::servers::McpTestResult, String> {
        let start = std::time::Instant::now();
        let (success, message) = match self.server_id.as_str() {
            "duckdb" => match Command::new("duckdb").arg("-version").output().await {
                Ok(o) if o.status.success() => (
                    true,
                    format!(
                        "Built-in DuckDB ready: {}",
                        String::from_utf8_lossy(&o.stdout).trim()
                    ),
                ),
                _ => (
                    false,
                    "Built-in DuckDB needs the `duckdb` CLI on PATH (https://duckdb.org/docs/installation)."
                        .to_string(),
                ),
            },
            _ => (
                true,
                format!("Built-in server '{}' runs in-process.", self.server_id),
            ),
        };
        Ok(crate::servers::McpTestResult {
            success,
            server_id: self.server_id.clone(),
            message,
            latency_ms: start.elapsed().as_millis().max(1) as u64,
            tools: self.synthesized_tools(),
        })
    }
}

/// Normalize an MCP `tools/call` result: unwrap `content[0].text`, parse JSON
/// when possible, otherwise return the structured result.
fn unwrap_tool_result(result: Value) -> Result<Value, String> {
    let content_text = result
        .get("content")
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            arr.iter()
                .find_map(|c| c.get("text").and_then(|t| t.as_str()))
        });
    if let Some(text) = content_text {
        if let Ok(json) = serde_json::from_str::<Value>(text) {
            return Ok(json);
        }
        return Ok(Value::String(text.to_string()));
    }
    Ok(result)
}

/// Execute DuckDB SQL through the real CLI. Errors clearly if it is not
/// installed rather than reporting a fake success.
async fn duckdb_call(tool_name: &str, args: &Value) -> Result<Value, String> {
    let sql = args
        .get("sql")
        .or_else(|| args.get("query"))
        .and_then(|v| v.as_str())
        .ok_or("duckdb: 'sql' argument is required")?;

    let output = Command::new("duckdb")
        .arg("-json")
        .arg("-c")
        .arg(sql)
        .output()
        .await
        .map_err(|e| {
            format!(
                "duckdb CLI not available ({}). Install DuckDB or use the 'sqlite' connector instead.",
                e
            )
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        return Err(format!("duckdb failed: {}", stderr.trim()));
    }
    let parsed: Value = serde_json::from_str(stdout.trim()).unwrap_or(Value::String(stdout));
    Ok(json!({
        "server": "duckdb",
        "tool": tool_name,
        "status": "success",
        "rows": parsed,
        "live": true
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::servers::McpServerConfig;

    fn cfg(id: &str) -> McpServerConfig {
        crate::servers::all_servers()
            .into_iter()
            .find(|s| s.id == id)
            .expect("built-in server exists")
    }

    #[test]
    fn synthesized_tools_are_stable_for_known_servers() {
        let client = McpClient::new(cfg("git"));
        let names: Vec<String> = client.synthesized_tools().into_iter().map(|t| t.name).collect();
        assert!(names.contains(&"git_status".to_string()));
        assert_eq!(client.server_id, "git");
    }

    #[test]
    fn unwrap_reads_text_content() {
        let v = json!({"content": [{"type": "text", "text": "{\"ok\": true}"}]});
        assert_eq!(unwrap_tool_result(v).unwrap(), json!({"ok": true}));
        let v = json!({"content": [{"type": "text", "text": "plain"}]});
        assert_eq!(unwrap_tool_result(v).unwrap(), json!("plain"));
    }

    #[tokio::test]
    async fn unreachable_server_reports_failure_not_fake_success() {
        let mut config = cfg("git");
        config.command = "ravenbot-definitely-not-a-real-binary".into();
        config.args = vec![];
        let client = McpClient::new(config);
        let result = client.test_connection().await.unwrap();
        assert!(!result.success);
        assert!(result.message.contains("Could not reach"));
        // Tools are still offered as synthesized descriptors for the model.
        assert!(!result.tools.is_empty());
    }

    #[tokio::test]
    async fn unreachable_tool_call_errors_instead_of_faking_success() {
        std::env::set_var("RAVENBOT_MCP_OFFLINE", "1");
        let mut config = cfg("github");
        config.command = "ravenbot-definitely-not-a-real-binary".into();
        config.args = vec![];
        let client = McpClient::new(config);
        let result = client.call_tool("github_create_issue", json!({})).await;
        assert!(
            result.is_err(),
            "an unreachable connector must error, never fabricate success: {result:?}"
        );
    }
}
