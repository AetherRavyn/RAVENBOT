//! `browser_dom` — operate the page in the desktop browser dock.
//!
//! The page is remote content running in its own origin. Scripts are evaluated
//! there, never in the app origin, and the evaluator is installed by the Tauri
//! shell only on platforms that expose a result-bearing bridge. A headless run
//! reports that DOM actions are unavailable instead of pretending they worked.
//!
//! The skill is high-risk as a whole: the same tool can click, submit, reload
//! and run page JavaScript. Read-only inspection is documented per action, but
//! a run should not treat the tool as a bypass around the approval policy.
//! `run_steps` reuses the same builders, stops on the first failure and refuses
//! `open`, `eval` and nested `run_steps`.

use async_trait::async_trait;
use ravenbot_core::Permission;
use serde_json::Value;

use crate::traits::{Skill, SkillContext, SkillError, SkillResult, SkillRisk};

pub struct BrowserDomSkill;

impl BrowserDomSkill {
    pub fn new() -> Self {
        Self
    }
}

impl Default for BrowserDomSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Skill for BrowserDomSkill {
    fn id(&self) -> &str {
        "browser_dom"
    }

    fn name(&self) -> &str {
        "Browser DOM"
    }

    fn description(&self) -> &str {
        "Operate the page open in the desktop browser dock. Start with action=snapshot to get selectors, then click, fill, press, hover, scroll, highlight, select_option, set_checked, text, html, back, forward, reload, wait, wait_for, run_steps, or eval. text and html accept an optional selector; without one they read the whole page. wait_for polls a selector/text/url until it matches. run_steps executes a bounded list of the same actions and stops on the first failure. Never guess a selector: snapshot first."
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn required_permissions(&self) -> Vec<Permission> {
        vec![
            Permission::InputControl,
            Permission::Network {
                domains: vec!["*".to_string()],
            },
        ]
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "snapshot | click | fill | press | hover | scroll | highlight | select_option | set_checked | text | html | back | forward | reload | wait | wait_for | run_steps | eval | open",
                    "enum": ["snapshot", "click", "fill", "press", "hover", "scroll", "highlight", "select_option", "set_checked", "text", "html", "back", "forward", "reload", "wait", "wait_for", "run_steps", "eval", "open"]
                },
                "selector": {
                    "type": "string",
                    "description": "CSS selector from a snapshot (click, fill, press, hover, scroll, highlight, select_option, set_checked, text, html, wait_for)"
                },
                "text": {
                    "type": "string",
                    "description": "Text to fill into the selected field (fill), or text to wait for (wait_for)"
                },
                "key": {
                    "type": "string",
                    "description": "Key name for press, e.g. Enter, Escape, Tab, ArrowDown"
                },
                "modifiers": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Modifier keys for press: Ctrl, Alt, Shift, Meta"
                },
                "delta_y": {
                    "type": "integer",
                    "description": "Pixels to scroll; positive scrolls down (default 600)"
                },
                "milliseconds": {
                    "type": "integer",
                    "description": "Wait duration (wait), max 30000"
                },
                "timeout_ms": {
                    "type": "integer",
                    "minimum": 0,
                    "maximum": 30000,
                    "description": "How long wait_for polls before failing (default 5000, max 30000)"
                },
                "values": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Option values or visible labels to select (select_option)"
                },
                "checked": {
                    "type": "boolean",
                    "description": "Desired state for a checkbox or radio (set_checked)"
                },
                "wait_state": {
                    "type": "string",
                    "enum": ["visible", "hidden", "attached"],
                    "description": "Selector state for wait_for. Default visible."
                },
                "steps": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 200,
                    "items": {
                        "type": "object",
                        "additionalProperties": true
                    },
                    "description": "run_steps: objects with action plus that action's fields. Allowed: snapshot, click, fill, press, hover, scroll, highlight, select_option, set_checked, text, html, back, forward, reload, wait, wait_for. Stops on the first failure."
                },
                "script": {
                    "type": "string",
                    "description": "JavaScript to evaluate in the page origin (eval). Not allowed inside run_steps."
                },
                "url": {
                    "type": "string",
                    "description": "http(s) URL to open in the dock (open), or a substring to wait for (wait_for)"
                }
            },
            "required": ["action"]
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
        let action = arguments
            .get("action")
            .and_then(Value::as_str)
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'action' field".to_string()))?
            .trim()
            .to_ascii_lowercase();

        let selector = arguments
            .get("selector")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());

        if !ravenbot_core::browser_dock_eval_available() && action != "open" && action != "wait" {
            // `open` only needs the opener; `wait` touches nothing. Every other
            // action needs the result-bearing evaluator.
            return Ok(SkillResult::failure(
                "Browser DOM actions are not available in this process. Run RAVENBOT as the desktop app on a platform with a browser bridge, or use browser_navigate for a headless text fetch.",
            ));
        }

        match action.as_str() {
            "open" => {
                let url = arguments
                    .get("url")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        SkillError::InvalidArguments("open requires a 'url'".to_string())
                    })?;
                let url = crate::builtin::browser_open::validate_http_url(url)
                    .map_err(SkillError::InvalidArguments)?;
                if !ravenbot_core::request_browser_dock(url.clone()) {
                    return Ok(SkillResult::failure(
                        "No browser dock is available in this process.",
                    ));
                }
                activity(context, "open", &url, true);
                Ok(SkillResult::success(serde_json::json!({
                    "action": "open",
                    "url": url
                })))
            }
            "wait" => {
                let ms = arguments
                    .get("milliseconds")
                    .and_then(Value::as_u64)
                    .unwrap_or(500)
                    .min(30_000);
                tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
                activity(context, "wait", &format!("{ms}ms"), true);
                Ok(SkillResult::success(serde_json::json!({
                    "action": "wait",
                    "milliseconds": ms
                })))
            }
            "wait_for" => {
                let value = wait_for_condition(&arguments).await;
                match value {
                    Ok(value) => {
                        let matched = value
                            .get("matched")
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                        let detail = arguments
                            .get("selector")
                            .and_then(Value::as_str)
                            .or_else(|| arguments.get("text").and_then(Value::as_str))
                            .or_else(|| arguments.get("url").and_then(Value::as_str))
                            .unwrap_or("condition");
                        activity(context, "wait_for", detail, matched);
                        if matched {
                            Ok(SkillResult::success(value))
                        } else {
                            let message = value
                                .get("error")
                                .and_then(Value::as_str)
                                .unwrap_or("The page condition did not appear before the timeout.");
                            Ok(SkillResult::failure(message))
                        }
                    }
                    Err(error) => {
                        activity(context, "wait_for", "condition", false);
                        Ok(SkillResult::failure(error))
                    }
                }
            }
            "run_steps" => run_steps(context, &arguments).await,
            "eval" => {
                let script = arguments
                    .get("script")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| {
                        SkillError::InvalidArguments("eval requires a 'script'".to_string())
                    })?;
                if script.len() > 256 * 1024 {
                    return Err(SkillError::InvalidArguments(
                        "The script is larger than the 256KB limit.".to_string(),
                    ));
                }
                let result = evaluate(script.to_string()).await;
                match result {
                    Ok(value) => {
                        activity(context, "eval", "script", true);
                        Ok(SkillResult::success(serde_json::json!({
                            "action": "eval",
                            "result": value
                        })))
                    }
                    Err(error) => {
                        activity(context, "eval", "script", false);
                        Ok(SkillResult::failure(error))
                    }
                }
            }
            _ => {
                let script = build_script(&action, &arguments, selector)?;
                let result = evaluate(script).await;
                match result {
                    Ok(value) => {
                        let failed = value
                            .get("ok")
                            .and_then(Value::as_bool)
                            .map(|ok| !ok)
                            .unwrap_or(false);
                        let detail = selector.unwrap_or(action.as_str()).to_string();
                        activity(context, &action, &detail, !failed);
                        if failed {
                            let message = value
                                .get("error")
                                .and_then(Value::as_str)
                                .unwrap_or("The page action failed.");
                            Ok(SkillResult::failure(message))
                        } else {
                            Ok(SkillResult::success(value))
                        }
                    }
                    Err(error) => {
                        activity(context, &action, selector.unwrap_or(action.as_str()), false);
                        Ok(SkillResult::failure(error))
                    }
                }
            }
        }
    }
}

fn activity(context: &SkillContext, action: &str, detail: &str, ok: bool) {
    ravenbot_core::emit_agent_activity("browser", context.bot_id, action, detail, ok);
}

/// Run a page script through the shell's evaluator off the async worker.
async fn evaluate(script: String) -> Result<Value, String> {
    let raw = tokio::task::spawn_blocking(move || ravenbot_core::evaluate_browser_dock(&script))
        .await
        .map_err(|error| format!("The browser evaluation task failed: {error}"))??;
    let Some(raw) = raw else {
        return Err(
            "No browser DOM bridge is installed in this process. Run RAVENBOT as the desktop app."
                .to_string(),
        );
    };
    Ok(serde_json::from_str(&raw).unwrap_or(Value::String(raw)))
}

fn js_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn optional_string<'a>(arguments: &'a Value, key: &str) -> Option<&'a str> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn build_wait_probe(
    selector: Option<&str>,
    text: Option<&str>,
    url: Option<&str>,
    state: &str,
) -> String {
    let selector = selector
        .map(js_string)
        .unwrap_or_else(|| "null".to_string());
    let text = text.map(js_string).unwrap_or_else(|| "null".to_string());
    let url = url.map(js_string).unwrap_or_else(|| "null".to_string());
    let state = js_string(state);
    format!(
        r##"
(() => {{
  const selector = {selector};
  const wantedText = {text};
  const wantedUrl = {url};
  const state = {state};
  const visible = (el) => {{
    const rect = el.getBoundingClientRect();
    const style = getComputedStyle(el);
    return rect.width > 1 && rect.height > 1 && style.visibility !== "hidden" && style.display !== "none";
  }};
  let matched = true;
  let found = null;
  if (selector) {{
    const el = document.querySelector(selector);
    found = !!el;
    if (!el) matched = state === "hidden";
    else if (state === "hidden") matched = !visible(el);
    else if (state === "visible") matched = visible(el);
  }}
  if (matched && wantedText) matched = (document.body?.innerText || "").includes(wantedText);
  if (matched && wantedUrl) matched = location.href.includes(wantedUrl);
  return {{ ok: true, action: "wait_for", matched, selectorFound: found, url: location.href, title: document.title }};
}})()
"##
    )
}

async fn wait_for_condition(arguments: &Value) -> Result<Value, String> {
    let selector = optional_string(arguments, "selector");
    let text = optional_string(arguments, "text");
    let url = optional_string(arguments, "url");
    if selector.is_none() && text.is_none() && url.is_none() {
        return Err(
            "wait_for needs a selector, text or url condition; refusing to wait blindly."
                .to_string(),
        );
    }
    if selector.is_some_and(|value| value.len() > 2_000) {
        return Err("The wait_for selector is longer than 2,000 characters.".to_string());
    }
    if text.is_some_and(|value| value.len() > 2_000) {
        return Err("The wait_for text is longer than 2,000 characters.".to_string());
    }
    if url.is_some_and(|value| value.len() > 2_048) {
        return Err("The wait_for URL is longer than 2,048 characters.".to_string());
    }
    let state = optional_string(arguments, "wait_state").unwrap_or("visible");
    if !["visible", "hidden", "attached"].contains(&state) {
        return Err("wait_state must be visible, hidden or attached.".to_string());
    }
    let timeout_ms = arguments
        .get("timeout_ms")
        .and_then(Value::as_u64)
        .unwrap_or(5_000)
        .min(30_000);
    let probe = build_wait_probe(selector, text, url, state);
    let started = std::time::Instant::now();
    loop {
        let value = evaluate(probe.clone()).await?;
        let matched = value
            .get("matched")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let elapsed = started.elapsed().as_millis() as u64;
        if matched {
            return Ok(serde_json::json!({
                "ok": true,
                "action": "wait_for",
                "matched": true,
                "waited_ms": elapsed,
                "url": value.get("url").cloned().unwrap_or(Value::Null),
                "title": value.get("title").cloned().unwrap_or(Value::Null),
            }));
        }
        if elapsed >= timeout_ms {
            return Ok(serde_json::json!({
                "ok": false,
                "action": "wait_for",
                "matched": false,
                "waited_ms": elapsed,
                "error": format!("The page condition did not appear within {timeout_ms} ms."),
                "url": value.get("url").cloned().unwrap_or(Value::Null),
                "title": value.get("title").cloned().unwrap_or(Value::Null),
            }));
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}

const STEP_ACTIONS: &[&str] = &[
    "snapshot",
    "click",
    "fill",
    "press",
    "hover",
    "scroll",
    "highlight",
    "select_option",
    "set_checked",
    "text",
    "html",
    "back",
    "forward",
    "reload",
    "wait",
    "wait_for",
];

async fn run_steps(context: &SkillContext, arguments: &Value) -> Result<SkillResult, SkillError> {
    let steps = arguments
        .get("steps")
        .and_then(Value::as_array)
        .filter(|steps| !steps.is_empty())
        .ok_or_else(|| {
            SkillError::InvalidArguments(
                "run_steps requires a non-empty 'steps' array.".to_string(),
            )
        })?;
    if steps.len() > 200 {
        return Err(SkillError::InvalidArguments(
            "run_steps accepts at most 200 steps.".to_string(),
        ));
    }
    if serde_json::to_string(steps)
        .map(|value| value.len())
        .unwrap_or(0)
        > 20_000
    {
        return Err(SkillError::InvalidArguments(
            "The run_steps payload is larger than 20,000 characters.".to_string(),
        ));
    }

    let total = steps.len();
    let started = std::time::Instant::now();
    let mut last = Value::Null;
    for (index, step) in steps.iter().enumerate() {
        if started.elapsed().as_millis() >= 120_000 {
            activity(context, "run_steps", &format!("step {}", index + 1), false);
            return Ok(SkillResult::failure(format!(
                "Step {} would exceed the 120-second run_steps budget.",
                index + 1
            )));
        }
        let action = step
            .get("action")
            .and_then(Value::as_str)
            .map(|value| value.trim().to_ascii_lowercase())
            .ok_or_else(|| {
                SkillError::InvalidArguments(format!("Step {} has no action.", index + 1))
            })?;
        if !STEP_ACTIONS.contains(&action.as_str()) {
            activity(context, "run_steps", &format!("step {}", index + 1), false);
            return Ok(SkillResult::failure(format!(
                "Step {} uses `{action}`, which is not allowed inside run_steps.",
                index + 1
            )));
        }
        if action == "wait" {
            let ms = step
                .get("milliseconds")
                .and_then(Value::as_u64)
                .unwrap_or(500)
                .min(30_000);
            tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
            last = serde_json::json!({"ok": true, "action": "wait", "milliseconds": ms});
            continue;
        }
        if action == "wait_for" {
            let value = wait_for_condition(step)
                .await
                .map_err(SkillError::Execution)?;
            if value.get("matched").and_then(Value::as_bool) != Some(true) {
                let message = value
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("The page condition did not appear.");
                activity(context, "run_steps", &format!("step {}", index + 1), false);
                return Ok(SkillResult::failure(format!(
                    "Step {} (wait_for) failed: {message}",
                    index + 1
                )));
            }
            last = value;
            continue;
        }
        let selector = optional_string(step, "selector");
        let script = build_script(&action, step, selector)?;
        let value = evaluate(script).await.map_err(SkillError::Execution)?;
        let failed = value
            .get("ok")
            .and_then(Value::as_bool)
            .map(|ok| !ok)
            .unwrap_or(false);
        if failed {
            let message = value
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("The page action failed.");
            activity(context, "run_steps", &format!("step {}", index + 1), false);
            return Ok(SkillResult::failure(format!(
                "Step {} ({action}) failed: {message}",
                index + 1
            )));
        }
        last = value;
    }

    if started.elapsed().as_millis() > 120_000 {
        activity(context, "run_steps", &format!("{total}/{total}"), false);
        return Ok(SkillResult::failure(
            "The run_steps budget of 120 seconds was exceeded.".to_string(),
        ));
    }
    activity(context, "run_steps", &format!("{total}/{total}"), true);
    Ok(SkillResult::success(serde_json::json!({
        "ok": true,
        "action": "run_steps",
        "completed": total,
        "last": last
    })))
}

fn build_script(
    action: &str,
    arguments: &Value,
    selector: Option<&str>,
) -> Result<String, SkillError> {
    let selector_literal = selector.map(js_string);
    match action {
        "snapshot" => Ok(r##"
(() => {
  const seen = new Set();
  const out = [];
  const visible = (el) => {
    const rect = el.getBoundingClientRect();
    const style = getComputedStyle(el);
    return rect.width > 1 && rect.height > 1 && style.visibility !== "hidden" && style.display !== "none";
  };
  const uniqueSelector = (el) => {
    if (el.id) {
      const byId = document.getElementById(el.id);
      if (byId === el) return "#" + CSS.escape(el.id);
    }
    const parts = [];
    let node = el;
    while (node && node.nodeType === 1 && node !== document.documentElement) {
      let part = node.tagName.toLowerCase();
      if (node.id) { parts.unshift("#" + CSS.escape(node.id)); break; }
      const parent = node.parentElement;
      if (parent) {
        const siblings = Array.from(parent.children).filter((c) => c.tagName === node.tagName);
        if (siblings.length > 1) part += ":nth-of-type(" + (siblings.indexOf(node) + 1) + ")";
      }
      parts.unshift(part);
      node = node.parentElement;
    }
    return parts.join(" > ");
  };
  const add = (el) => {
    if (out.length >= 200 || seen.has(el)) return;
    if (!visible(el)) return;
    seen.add(el);
    const tag = el.tagName.toLowerCase();
    const text = (el.innerText || el.getAttribute("aria-label") || el.value || "").trim().replace(/\s+/g, " ").slice(0, 160);
    out.push({
      tag,
      selector: uniqueSelector(el),
      text,
      name: el.getAttribute("name") || "",
      type: el.getAttribute("type") || "",
      placeholder: el.getAttribute("placeholder") || "",
      href: el.getAttribute("href") || "",
      disabled: !!el.disabled,
      checked: el.checked ?? null
    });
  };
  document.querySelectorAll('a[href],button,input,textarea,select,[contenteditable="true"],[role="button"],[role="link"],[role="tab"]').forEach(add);
  return { ok: true, action: "snapshot", url: location.href, title: document.title, count: out.length, elements: out };
})()
"##.to_string()),
        "click" => {
            let selector = selector_literal.ok_or_else(|| {
                SkillError::InvalidArguments("click requires a 'selector'".to_string())
            })?;
            Ok(format!(r##"
(() => {{
  const el = document.querySelector({selector});
  if (!el) return {{ ok: false, error: "No element matches the selector." }};
  el.scrollIntoView({{ block: "center", inline: "center" }});
  el.style.outline = "3px solid #79b8ff";
  setTimeout(() => {{ el.style.outline = ""; }}, 600);
  el.click();
  return {{ ok: true, action: "click", selector: {selector}, tag: el.tagName.toLowerCase() }};
}})()
"##))
        }
        "fill" => {
            let selector = selector_literal.ok_or_else(|| {
                SkillError::InvalidArguments("fill requires a 'selector'".to_string())
            })?;
            let text = arguments
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let text = js_string(text);
            Ok(format!(r##"
(() => {{
  const el = document.querySelector({selector});
  if (!el) return {{ ok: false, error: "No element matches the selector." }};
  el.scrollIntoView({{ block: "center", inline: "center" }});
  el.focus();
  if (el.isContentEditable) {{
    el.textContent = {text};
  }} else {{
    const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    const setter = Object.getOwnPropertyDescriptor(proto, "value")?.set;
    if (setter) setter.call(el, {text}); else el.value = {text};
  }}
  el.dispatchEvent(new Event("input", {{ bubbles: true }}));
  el.dispatchEvent(new Event("change", {{ bubbles: true }}));
  el.style.outline = "3px solid #9ae6b4";
  setTimeout(() => {{ el.style.outline = ""; }}, 600);
  return {{ ok: true, action: "fill", selector: {selector} }};
}})()
"##))
        }
        "press" => {
            let key = arguments
                .get("key")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    SkillError::InvalidArguments("press requires a 'key'".to_string())
                })?;
            let key = js_string(key);
            let selector = selector_literal.unwrap_or_else(|| "null".to_string());
            let modifiers = arguments
                .get("modifiers")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(js_string)
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .unwrap_or_default();
            Ok(format!(r##"
(() => {{
  const el = {selector} ? document.querySelector({selector}) : (document.activeElement || document.body);
  if (!el) return {{ ok: false, error: "No element matches the selector." }};
  el.focus();
  const mods = [{modifiers}].map((m) => m.toLowerCase());
  const init = {{
    key: {key},
    bubbles: true,
    cancelable: true,
    ctrlKey: mods.includes("ctrl") || mods.includes("control"),
    altKey: mods.includes("alt"),
    shiftKey: mods.includes("shift"),
    metaKey: mods.includes("meta") || mods.includes("cmd") || mods.includes("command")
  }};
  el.dispatchEvent(new KeyboardEvent("keydown", init));
  el.dispatchEvent(new KeyboardEvent("keypress", init));
  el.dispatchEvent(new KeyboardEvent("keyup", init));
  return {{ ok: true, action: "press", key: {key}, synthetic: true }};
}})()
"##))
        }
        "scroll" => {
            let selector = selector_literal.unwrap_or_else(|| "null".to_string());
            let delta = arguments
                .get("delta_y")
                .and_then(Value::as_i64)
                .unwrap_or(600);
            Ok(format!(r##"
(() => {{
  const el = {selector} ? document.querySelector({selector}) : null;
  if ({selector} && !el) return {{ ok: false, error: "No element matches the selector." }};
  if (el) el.scrollIntoView({{ block: "center", inline: "nearest" }});
  else window.scrollBy({{ top: {delta}, left: 0, behavior: "smooth" }});
  return {{ ok: true, action: "scroll", x: window.scrollX, y: window.scrollY }};
}})()
"##))
        }
        "highlight" => {
            let selector = selector_literal.ok_or_else(|| {
                SkillError::InvalidArguments("highlight requires a 'selector'".to_string())
            })?;
            Ok(format!(r##"
(() => {{
  const el = document.querySelector({selector});
  if (!el) return {{ ok: false, error: "No element matches the selector." }};
  el.scrollIntoView({{ block: "center", inline: "center" }});
  const previous = el.style.outline;
  el.style.outline = "3px solid #ffd591";
  setTimeout(() => {{ el.style.outline = previous; }}, 1200);
  return {{ ok: true, action: "highlight", selector: {selector} }};
}})()
"##))
        }
        "hover" => {
            let selector = selector_literal.ok_or_else(|| {
                SkillError::InvalidArguments("hover requires a 'selector'".to_string())
            })?;
            Ok(format!(r##"
(() => {{
  const el = document.querySelector({selector});
  if (!el) return {{ ok: false, error: "No element matches the selector." }};
  el.scrollIntoView({{ block: "center", inline: "center" }});
  const fire = (type) => {{
    try {{ el.dispatchEvent(new PointerEvent(type, {{ bubbles: true }})); }}
    catch {{ el.dispatchEvent(new MouseEvent(type, {{ bubbles: true }})); }}
  }};
  ["pointerover", "mouseover", "pointerenter", "mouseenter", "mousemove"].forEach(fire);
  el.style.outline = "3px solid #c4b5fd";
  setTimeout(() => {{ el.style.outline = ""; }}, 600);
  return {{ ok: true, action: "hover", selector: {selector}, tag: el.tagName.toLowerCase() }};
}})()
"##))
        }
        "select_option" => {
            let selector = selector_literal.ok_or_else(|| {
                SkillError::InvalidArguments("select_option requires a 'selector'".to_string())
            })?;
            let values = arguments
                .get("values")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(js_string)
                        .collect::<Vec<_>>()
                })
                .or_else(|| {
                    arguments
                        .get("value")
                        .and_then(Value::as_str)
                        .map(|value| vec![js_string(value)])
                })
                .filter(|values| !values.is_empty())
                .ok_or_else(|| {
                    SkillError::InvalidArguments(
                        "select_option requires a 'values' array or a 'value'.".to_string(),
                    )
                })?;
            if values.len() > 100 {
                return Err(SkillError::InvalidArguments(
                    "select_option accepts at most 100 values.".to_string(),
                ));
            }
            let list = values.join(",");
            Ok(format!(r##"
(() => {{
  const el = document.querySelector({selector});
  if (!el) return {{ ok: false, error: "No element matches the selector." }};
  if (!(el instanceof HTMLSelectElement)) return {{ ok: false, error: "The selector is not a <select> element." }};
  const wanted = [{list}];
  if (!el.multiple && wanted.length > 1) return {{ ok: false, error: "A single-select accepts one value." }};
  const options = Array.from(el.options);
  const chosen = [];
  for (const want of wanted) {{
    const match = options.find((option) => option.value === want || (option.text || "").trim() === want);
    if (!match) return {{ ok: false, error: "No option matches " + JSON.stringify(want) + "." }};
    if (!el.multiple) {{
      const setter = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value")?.set;
      if (setter) setter.call(el, match.value); else el.value = match.value;
      chosen.push(match.value);
      break;
    }}
    match.selected = true;
    chosen.push(match.value);
  }}
  el.dispatchEvent(new Event("input", {{ bubbles: true }}));
  el.dispatchEvent(new Event("change", {{ bubbles: true }}));
  return {{ ok: true, action: "select_option", selector: {selector}, values: chosen }};
}})()
"##))
        }
        "set_checked" => {
            let selector = selector_literal.ok_or_else(|| {
                SkillError::InvalidArguments("set_checked requires a 'selector'".to_string())
            })?;
            let checked = arguments
                .get("checked")
                .and_then(Value::as_bool)
                .ok_or_else(|| {
                    SkillError::InvalidArguments(
                        "set_checked requires a boolean 'checked'.".to_string(),
                    )
                })?;
            Ok(format!(r##"
(() => {{
  const el = document.querySelector({selector});
  if (!el) return {{ ok: false, error: "No element matches the selector." }};
  if (!(el instanceof HTMLInputElement) || (el.type !== "checkbox" && el.type !== "radio")) {{
    return {{ ok: false, error: "The selector is not a checkbox or radio." }};
  }}
  const wanted = {checked};
  if (el.type === "radio" && !wanted) {{
    el.checked = false;
    el.dispatchEvent(new Event("input", {{ bubbles: true }}));
    el.dispatchEvent(new Event("change", {{ bubbles: true }}));
  }} else if (el.checked !== wanted) {{
    el.click();
  }}
  return {{ ok: true, action: "set_checked", selector: {selector}, checked: el.checked }};
}})()
"##))
        }
        "text" => {
            let selector = selector_literal.unwrap_or_else(|| "null".to_string());
            Ok(format!(r##"
(() => {{
  const el = {selector} ? document.querySelector({selector}) : document.body;
  if (!el) return {{ ok: false, error: "No element matches the selector." }};
  return {{ ok: true, action: "text", url: location.href, title: document.title, text: (el.innerText || "").slice(0, 20000) }};
}})()
"##))
        }
        "html" => {
            let selector = selector_literal.unwrap_or_else(|| "null".to_string());
            Ok(format!(r##"
(() => {{
  const el = {selector} ? document.querySelector({selector}) : document.documentElement;
  if (!el) return {{ ok: false, error: "No element matches the selector." }};
  return {{ ok: true, action: "html", url: location.href, title: document.title, html: el.outerHTML.slice(0, 50000) }};
}})()
"##))
        }
        "back" => Ok(r##"
(() => { history.back(); return { ok: true, action: "back" }; })()
"##.to_string()),
        "forward" => Ok(r##"
(() => { history.forward(); return { ok: true, action: "forward" }; })()
"##.to_string()),
        "reload" => Ok(r##"
(() => { location.reload(); return { ok: true, action: "reload" }; })()
"##.to_string()),
        other => Err(SkillError::InvalidArguments(format!(
            "Unknown browser action '{other}'. Use snapshot, click, fill, press, hover, scroll, highlight, select_option, set_checked, text, html, back, forward, reload, wait, wait_for, run_steps, eval, or open."
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_script_is_built_without_unformatted_rust_braces() {
        let cases = [
            ("click", serde_json::json!({}), Some("#submit")),
            (
                "fill",
                serde_json::json!({"text": "a \"quoted\" value"}),
                Some("input[name=q]"),
            ),
            (
                "press",
                serde_json::json!({"key": "Enter", "modifiers": ["Ctrl"]}),
                None,
            ),
            ("scroll", serde_json::json!({"delta_y": 400}), None),
            ("highlight", serde_json::json!({}), Some("a")),
            ("hover", serde_json::json!({}), Some("button")),
            (
                "select_option",
                serde_json::json!({"values": ["open", "closed"]}),
                Some("select[name=status]"),
            ),
            (
                "set_checked",
                serde_json::json!({"checked": true}),
                Some("input[type=checkbox]"),
            ),
        ];
        for (action, arguments, selector) in cases {
            let script = build_script(action, &arguments, selector).expect(action);
            assert!(
                !script.contains("{{"),
                "{action}: unformatted braces: {script}"
            );
            assert!(
                !script.contains("}}"),
                "{action}: unformatted braces: {script}"
            );
        }
    }

    #[test]
    fn select_option_and_set_checked_build_the_expected_values() {
        let select = build_script(
            "select_option",
            &serde_json::json!({"values": ["open", "closed"]}),
            Some("#status"),
        )
        .expect("select");
        assert!(
            select.contains("const wanted = [\"open\",\"closed\"]"),
            "{select}"
        );
        assert!(select.contains("HTMLSelectElement"), "{select}");
        assert!(select.contains("new Event(\"change\""), "{select}");

        let check = build_script(
            "set_checked",
            &serde_json::json!({"checked": true}),
            Some("#agree"),
        )
        .expect("checked");
        assert!(check.contains("const wanted = true"), "{check}");
        assert!(check.contains("el.type !== \"checkbox\""), "{check}");
    }

    #[test]
    fn wait_probe_carries_all_conditions() {
        let probe = build_wait_probe(Some("#done"), Some("Finished"), Some("/result"), "visible");
        assert!(probe.contains("const selector = \"#done\""), "{probe}");
        assert!(probe.contains("const wantedText = \"Finished\""), "{probe}");
        assert!(probe.contains("const wantedUrl = \"/result\""), "{probe}");
        assert!(probe.contains("const state = \"visible\""), "{probe}");
        assert!(!probe.contains("{{"), "{probe}");
    }

    #[tokio::test]
    async fn wait_for_refuses_a_blind_wait() {
        let error = wait_for_condition(&serde_json::json!({}))
            .await
            .expect_err("blind wait");
        assert!(error.contains("refusing to wait blindly"));
    }

    #[tokio::test]
    async fn run_steps_rejects_an_unbounded_or_forbidden_list() {
        let context = SkillContext::with_default_tier(
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
        );
        let too_many = serde_json::json!({
            "steps": vec![serde_json::json!({"action": "snapshot"}); 201]
        });
        let error = run_steps(&context, &too_many).await.expect_err("too many");
        assert!(error.to_string().contains("at most 200"));

        let forbidden = serde_json::json!({
            "steps": [{"action": "eval", "script": "alert(1)"}]
        });
        let result = run_steps(&context, &forbidden).await.expect("result");
        assert!(!result.success);
        assert!(result
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("not allowed"));
    }

    #[test]
    fn text_and_html_scripts_return_an_object() {
        for action in ["text", "html"] {
            let script = build_script(action, &serde_json::json!({}), None).expect(action);
            assert!(
                script.contains("return {"),
                "{action}: the IIFE must return its result object: {script}"
            );
            assert!(
                script.contains("ok: true"),
                "{action}: the result object must carry ok:true: {script}"
            );
        }
    }

    #[test]
    fn text_and_html_honour_an_optional_selector() {
        let text = build_script("text", &serde_json::json!({}), Some("main article"))
            .expect("text selector");
        assert!(
            text.contains("document.querySelector(\"main article\")"),
            "{text}"
        );
        let html =
            build_script("html", &serde_json::json!({}), Some("#app")).expect("html selector");
        assert!(html.contains("document.querySelector(\"#app\")"), "{html}");
    }

    #[test]
    fn a_selector_is_json_escaped_before_it_reaches_the_page() {
        let script = build_script(
            "click",
            &serde_json::json!({}),
            Some("a[href=\"x\"]\n'); alert(1); //"),
        )
        .unwrap();
        assert!(script.contains("a[href=\\\"x\\\"]"), "{script}");
        assert!(
            !script.contains("alert(1); //\n"),
            "the selector must stay a string literal"
        );
    }

    #[test]
    fn unknown_actions_are_refused() {
        assert!(build_script("teleport", &serde_json::json!({}), None).is_err());
    }
}
