//! Generic prompt-CLI engine.
//!
//! Many agent CLIs share the same shape: run `<command> <args…>` with a prompt,
//! stream text (or JSON lines) to stdout. Instead of a bespoke driver per tool
//! we describe each CLI with a [`CliSpec`] and reuse one engine. Built-in specs
//! cover the common tools; users can add their own with the
//! `RAVENBOT_CLI_ENGINES` env var (JSON array of `CliSpec`).

use crate::process;
use crate::{
    AgentEngine, CancelToken, EngineCallback, EngineCapabilities, EngineError, EngineEvent,
    EngineInfo, EngineOutcome, EngineRequest,
};
use serde_json::Value;
use tokio::io::AsyncWriteExt;

/// Declarative description of a prompt CLI.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CliSpec {
    pub id: String,
    pub display_name: String,
    /// Binary looked up on PATH.
    pub command: String,
    /// Argument template. `{prompt}` is replaced with the prompt (if absent the
    /// prompt is written to stdin); `{cwd}` with the working directory.
    #[serde(default)]
    pub args: Vec<String>,
    /// Flag used to pass a model (e.g. `--model`).
    #[serde(default)]
    pub model_flag: Option<String>,
    /// Flag used to pass a system prompt, when the CLI has one.
    #[serde(default)]
    pub system_flag: Option<String>,
    /// Whether the CLI accepts provider-qualified model ids (`provider/model`).
    #[serde(default)]
    pub accepts_full_model_id: bool,
    /// Optional JSON pointer to assistant text in a JSONL stream
    /// (e.g. `/message/content`).
    #[serde(default)]
    pub json_text_path: Option<String>,
    #[serde(default)]
    pub install_hint: Option<String>,
    #[serde(default)]
    pub sign_in_hint: Option<String>,
}

impl CliSpec {
    /// Prompt actually sent (system instructions are prepended when the CLI has
    /// no dedicated system flag).
    pub fn effective_prompt(&self, req: &EngineRequest) -> String {
        match req.system.as_deref().filter(|s| !s.trim().is_empty()) {
            Some(system) if self.system_flag.is_none() => {
                format!("{}\n\n{}", system.trim(), req.prompt)
            }
            _ => req.prompt.clone(),
        }
    }

    /// Full argv for one turn.
    pub fn build_args(&self, req: &EngineRequest) -> Vec<String> {
        let prompt = self.effective_prompt(req);
        let mut args = Vec::new();
        for arg in &self.args {
            match arg.as_str() {
                "{prompt}" => args.push(prompt.clone()),
                "{cwd}" => {
                    if let Some(cwd) = &req.cwd {
                        args.push(cwd.clone());
                    }
                }
                other => args.push(other.to_string()),
            }
        }
        if let (Some(flag), Some(model)) = (
            self.model_flag.as_deref(),
            req.model.as_deref().filter(|m| !m.trim().is_empty()),
        ) {
            args.push(flag.to_string());
            args.push(model.to_string());
        }
        if let (Some(flag), Some(system)) = (
            self.system_flag.as_deref(),
            req.system.as_deref().filter(|s| !s.trim().is_empty()),
        ) {
            args.push(flag.to_string());
            args.push(system.to_string());
        }
        args.extend(req.extra_args.iter().cloned());
        args
    }

    /// Whether the prompt must be written to stdin (no `{prompt}` placeholder).
    pub fn uses_stdin(&self) -> bool {
        !self.args.iter().any(|a| a == "{prompt}")
    }
}

/// Built-in specs for widely used agent CLIs. Only engines whose binary is on
/// PATH become selectable; the rest show an install hint.
pub fn builtin_specs() -> Vec<CliSpec> {
    let spec = |id: &str,
                display_name: &str,
                command: &str,
                args: &[&str],
                model_flag: Option<&str>,
                accepts_full_model_id: bool,
                install_hint: &str,
                sign_in_hint: &str| CliSpec {
        id: id.to_string(),
        display_name: display_name.to_string(),
        command: command.to_string(),
        args: args.iter().map(|s| s.to_string()).collect(),
        model_flag: model_flag.map(str::to_string),
        system_flag: None,
        accepts_full_model_id,
        json_text_path: None,
        install_hint: Some(install_hint.to_string()),
        sign_in_hint: Some(sign_in_hint.to_string()),
    };

    vec![
        spec(
            "opencode",
            "OpenCode",
            "opencode",
            &["run", "{prompt}"],
            Some("--model"),
            true,
            "Install OpenCode: npm install -g opencode-ai",
            "Run `opencode auth login` in a terminal.",
        ),
        spec(
            "gemini",
            "Gemini CLI",
            "gemini",
            &["-p", "{prompt}"],
            Some("--model"),
            true,
            "Install the Gemini CLI: npm install -g @google/gemini-cli",
            "Run `gemini` once and sign in with your Google account.",
        ),
        spec(
            "grok",
            "Grok CLI",
            "grok",
            &["-p", "{prompt}"],
            Some("--model"),
            false,
            "Install the xAI Grok CLI and put `grok` on PATH.",
            "Run `grok login` (or set XAI_API_KEY).",
        ),
        spec(
            "cursor-agent",
            "Cursor Agent",
            "cursor-agent",
            &["-p", "{prompt}"],
            Some("--model"),
            true,
            "Install Cursor CLI: curl https://cursor.com/install -fsS | bash",
            "Run `cursor-agent login`.",
        ),
        spec(
            "qwen",
            "Qwen Code",
            "qwen",
            &["-p", "{prompt}"],
            Some("--model"),
            true,
            "Install Qwen Code: npm install -g @qwen-code/qwen-code",
            "Run `qwen` once and authenticate.",
        ),
        spec(
            "crush",
            "Crush",
            "crush",
            &["run", "{prompt}"],
            Some("--model"),
            true,
            "Install Charm Crush (see charm.sh/crush).",
            "Configure a provider in Crush.",
        ),
        spec(
            "aider",
            "Aider",
            "aider",
            &["--yes", "--no-auto-commits", "--message", "{prompt}"],
            Some("--model"),
            true,
            "Install Aider: python -m pip install aider-chat",
            "Set the provider API key you use with Aider.",
        ),
        spec(
            "antigravity",
            "Antigravity",
            "antigravity",
            &["-p", "{prompt}"],
            Some("--model"),
            true,
            "Install Google Antigravity and put `antigravity` on PATH.",
            "Sign in with your Google account.",
        ),
        spec(
            "command-code",
            "Command Code",
            "command-code",
            &["-p", "{prompt}"],
            Some("--model"),
            true,
            "Install the Command Code CLI and put `command-code` on PATH.",
            "Run `command-code login` or set COMMANDCODE_API_KEY.",
        ),
    ]
}

/// User-defined specs from `RAVENBOT_CLI_ENGINES` (JSON array). Custom entries
/// override built-ins with the same id.
pub fn custom_specs() -> Vec<CliSpec> {
    std::env::var("RAVENBOT_CLI_ENGINES")
        .ok()
        .and_then(|raw| serde_json::from_str::<Vec<CliSpec>>(raw.trim()).ok())
        .unwrap_or_default()
}

/// Built-ins + custom, deduped by id (custom wins).
pub fn all_specs() -> Vec<CliSpec> {
    let mut specs = builtin_specs();
    for custom in custom_specs() {
        if let Some(existing) = specs.iter_mut().find(|s| s.id == custom.id) {
            *existing = custom;
        } else {
            specs.push(custom);
        }
    }
    specs
}

/// A generic engine driven by a [`CliSpec`].
pub struct CliEngine {
    spec: CliSpec,
}

impl CliEngine {
    pub fn new(spec: CliSpec) -> Self {
        Self { spec }
    }

    pub fn spec(&self) -> &CliSpec {
        &self.spec
    }
}

#[async_trait::async_trait]
impl AgentEngine for CliEngine {
    fn id(&self) -> &str {
        &self.spec.id
    }
    fn display_name(&self) -> &str {
        &self.spec.display_name
    }
    fn command(&self) -> &str {
        &self.spec.command
    }
    fn install_hint(&self) -> &str {
        self.spec.install_hint.as_deref().unwrap_or("Install this CLI and put it on PATH.")
    }
    fn sign_in_hint(&self) -> &str {
        self.spec.sign_in_hint.as_deref().unwrap_or("Sign in with the CLI.")
    }
    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            resume: false,
            images: false,
            effort: false,
            interrupt: true,
            accepts_full_model_id: self.spec.accepts_full_model_id,
        }
    }

    async fn detect(&self) -> EngineInfo {
        let available = process::find_binary(self.command()).is_some();
        let version = if available {
            process::version_of(self.command(), &["--version"]).await
        } else {
            None
        };
        EngineInfo {
            id: self.id().to_string(),
            display_name: self.display_name().to_string(),
            command: self.command().to_string(),
            available,
            version,
            install_hint: Some(self.install_hint().to_string()),
            sign_in_hint: Some(self.sign_in_hint().to_string()),
            models: Vec::new(),
        }
    }

    async fn run(
        &self,
        req: EngineRequest,
        on_event: EngineCallback,
        cancel: CancelToken,
    ) -> Result<EngineOutcome, EngineError> {
        if process::find_binary(self.command()).is_none() {
            return Err(EngineError::missing_cli(self.command(), self.install_hint()));
        }

        let args = self.spec.build_args(&req);
        let mut child =
            process::spawn_with_stdin(self.command(), &args, req.cwd.as_deref(), &req.env)?;

        if self.spec.uses_stdin() {
            if let Some(mut stdin) = child.stdin.take() {
                stdin
                    .write_all(self.spec.effective_prompt(&req).as_bytes())
                    .await
                    .map_err(|_| {
                        EngineError::spawn(format!("failed to write prompt to {}", self.command()))
                    })?;
                let _ = stdin.flush().await;
            }
        }

        let mut text = String::new();
        let json_path = self.spec.json_text_path.clone();
        let result = process::stream_lines(
            child,
            |line| {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    return;
                }
                // Optionally pull assistant text out of a JSONL stream.
                if let Some(path) = json_path.as_deref() {
                    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
                        if let Some(extracted) = value.pointer(path).and_then(|v| v.as_str()) {
                            text.push_str(extracted);
                            on_event(EngineEvent::TextDelta(extracted.to_string()));
                            return;
                        }
                    }
                }
                text.push_str(trimmed);
                text.push('\n');
                on_event(EngineEvent::TextDelta(format!("{}\n", trimmed)));
            },
            &cancel,
        )
        .await;

        match result {
            Ok(()) => {
                let mut outcome = EngineOutcome {
                    ok: true,
                    final_text: text,
                    ..Default::default()
                };
                if outcome.final_text.trim().is_empty() {
                    outcome.final_text = "(the engine returned no output)".to_string();
                }
                Ok(outcome)
            }
            Err(e) => {
                // Turn a sign-in failure into an actionable setup error.
                let msg = e.message.to_lowercase();
                if msg.contains("login")
                    || msg.contains("not logged in")
                    || msg.contains("unauthorized")
                    || msg.contains("authentication")
                    || msg.contains("api key")
                {
                    Err(EngineError::auth(format!(
                        "{} is not signed in. {}",
                        self.display_name(),
                        self.sign_in_hint()
                    )))
                } else {
                    Err(e)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_spec() -> CliSpec {
        CliSpec {
            id: "demo".into(),
            display_name: "Demo CLI".into(),
            command: "demo".into(),
            args: vec!["run".into(), "{prompt}".into()],
            model_flag: Some("--model".into()),
            system_flag: None,
            accepts_full_model_id: true,
            json_text_path: None,
            install_hint: None,
            sign_in_hint: None,
        }
    }

    #[test]
    fn builtin_specs_cover_major_clis_and_are_unique() {
        let specs = builtin_specs();
        for expected in ["opencode", "gemini", "grok", "cursor-agent", "aider", "antigravity"] {
            assert!(specs.iter().any(|s| s.id == expected), "missing spec {expected}");
        }
        let mut ids: Vec<&str> = specs.iter().map(|s| s.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), specs.len(), "duplicate engine ids");
    }

    #[test]
    fn build_args_substitutes_prompt_and_model() {
        let spec = demo_spec();
        let req = EngineRequest {
            prompt: "hello world".into(),
            system: Some("be terse".into()),
            model: Some("provider/model-x".into()),
            ..Default::default()
        };
        let args = spec.build_args(&req);
        // system prepended when there is no system flag
        assert!(args.iter().any(|a| a.contains("hello world") && a.contains("be terse")));
        assert!(args.contains(&"--model".to_string()));
        assert!(args.contains(&"provider/model-x".to_string()));
        assert!(!spec.uses_stdin());
    }

    #[test]
    fn stdin_mode_when_no_prompt_placeholder() {
        let mut spec = demo_spec();
        spec.args = vec!["chat".into()];
        assert!(spec.uses_stdin());
    }

    #[test]
    fn custom_specs_override_builtins_by_id() {
        std::env::set_var(
            "RAVENBOT_CLI_ENGINES",
            r#"[{"id":"opencode","display_name":"My OpenCode","command":"my-oc","args":["go","{prompt}"]}]"#,
        );
        let specs = all_specs();
        let oc = specs.iter().find(|s| s.id == "opencode").unwrap();
        assert_eq!(oc.command, "my-oc");
        assert_eq!(oc.display_name, "My OpenCode");
        std::env::remove_var("RAVENBOT_CLI_ENGINES");
    }
}
