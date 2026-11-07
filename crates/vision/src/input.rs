//! Real desktop input injection.
//!
//! Backends are detected at runtime and each capability (pointer, typing, key
//! presses) picks the best available tool:
//!
//! * **X11 / XWayland** — `xdotool` (click, move, scroll, type, key)
//! * **Wayland** — `ydotool` for pointer + `wtype` for typing/keys
//!
//! Honesty contract: when no backend can perform an action, the plan is `None`
//! and the caller gets an explicit error — never a fabricated success. Command
//! *planning* is separated from *execution* so it can be tested without moving
//! the user's real cursor or typing into their session.

use std::path::PathBuf;
use tokio::process::Command;

/// Mouse button for click actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tool {
    Xdotool,
    Ydotool,
    Wtype,
}

impl Tool {
    fn binary(self) -> &'static str {
        match self {
            Tool::Xdotool => "xdotool",
            Tool::Ydotool => "ydotool",
            Tool::Wtype => "wtype",
        }
    }
}

#[derive(Debug, Clone)]
struct Program {
    tool: Tool,
    path: PathBuf,
}

/// Detected input tooling on this machine.
#[derive(Debug, Clone)]
pub struct InputInjector {
    pointer: Option<Program>,
    typer: Option<Program>,
    keys: Option<Program>,
}

impl Default for InputInjector {
    fn default() -> Self {
        Self::detect()
    }
}

impl InputInjector {
    /// Detect the best backends for the current session.
    pub fn detect() -> Self {
        let on_wayland = std::env::var("XDG_SESSION_TYPE")
            .map(|v| v.eq_ignore_ascii_case("wayland"))
            .unwrap_or(false)
            || std::env::var("WAYLAND_DISPLAY").is_ok();
        let has_x_display = std::env::var("DISPLAY").is_ok();

        let xdotool = find("xdotool").map(|p| Program {
            tool: Tool::Xdotool,
            path: p,
        });
        let ydotool = find("ydotool").map(|p| Program {
            tool: Tool::Ydotool,
            path: p,
        });
        let wtype = find("wtype").map(|p| Program {
            tool: Tool::Wtype,
            path: p,
        });

        // Pointer: prefer native ydotool on Wayland, else xdotool.
        let pointer = if on_wayland {
            ydotool.clone().or_else(|| xdotool.clone())
        } else {
            xdotool.clone().or_else(|| ydotool.clone())
        };
        // Typing: wtype is purpose-built for Wayland; xdotool for X11.
        let typer = if on_wayland {
            wtype
                .clone()
                .or_else(|| xdotool.clone())
                .or_else(|| ydotool.clone())
        } else {
            xdotool
                .clone()
                .or_else(|| wtype.clone())
                .or_else(|| ydotool.clone())
        };
        // Key presses: xdotool on X11, wtype on Wayland.
        let keys = if on_wayland {
            wtype
                .clone()
                .or_else(|| xdotool.clone())
                .or_else(|| ydotool.clone())
        } else {
            xdotool
                .clone()
                .or_else(|| wtype.clone())
                .or_else(|| ydotool.clone())
        };

        let _ = has_x_display;
        Self {
            pointer,
            typer,
            keys,
        }
    }

    /// True when at least pointer control is available.
    pub fn is_available(&self) -> bool {
        self.pointer.is_some()
    }

    /// Human-readable backend summary for the UI.
    pub fn backend_summary(&self) -> String {
        fn name(p: &Option<Program>) -> &'static str {
            match p {
                Some(p) => p.tool.binary(),
                None => "none",
            }
        }
        format!(
            "pointer={} typing={} keys={}",
            name(&self.pointer),
            name(&self.typer),
            name(&self.keys),
        )
    }

    // ── Plan construction (pure, testable) ────────────────────────────────

    fn plan_move(&self, x: f32, y: f32) -> Option<(Tool, Vec<String>)> {
        let p = self.pointer.as_ref()?;
        let (x, y) = (x.round() as i64, y.round() as i64);
        match p.tool {
            Tool::Xdotool => Some((
                p.tool,
                vec!["mousemove".into(), x.to_string(), y.to_string()],
            )),
            // ydotool defaults to relative movement; absolute is required for
            // screen coordinates from a screenshot. A negative layout
            // coordinate needs the two-step sentinel because uinput cannot
            // express a negative absolute axis.
            Tool::Ydotool => {
                if x < 0 || y < 0 {
                    Some((
                        p.tool,
                        vec!["__ydotool_move__".into(), x.to_string(), y.to_string()],
                    ))
                } else {
                    Some((
                        p.tool,
                        vec![
                            "mousemove".into(),
                            "-a".into(),
                            x.to_string(),
                            y.to_string(),
                        ],
                    ))
                }
            }
            Tool::Wtype => None,
        }
    }

    fn plan_click(&self, x: f32, y: f32, button: Button) -> Option<(Tool, Vec<String>)> {
        let p = self.pointer.as_ref()?;
        let (x, y) = (x.round() as i64, y.round() as i64);
        match p.tool {
            Tool::Xdotool => {
                let btn = match button {
                    Button::Left => "1",
                    Button::Middle => "2",
                    Button::Right => "3",
                };
                Some((
                    p.tool,
                    vec![
                        "mousemove".into(),
                        x.to_string(),
                        y.to_string(),
                        "click".into(),
                        btn.into(),
                    ],
                ))
            }
            Tool::Ydotool => {
                // ydotool buttons are a base code plus a down/up mask. 0x00
                // alone is deliberately a no-op, so a click is 0xC0 (left),
                // 0xC1 (right), or 0xC2 (middle).
                let code = match button {
                    Button::Left => "0xC0",
                    Button::Middle => "0xC2",
                    Button::Right => "0xC1",
                };
                // ydotool has no single move+click; the sentinel tells `run`
                // to move first, then click. Keep x/y at indices 1 and 2 so
                // every ydotool sentinel shares the same coordinate shape.
                Some((
                    p.tool,
                    vec![
                        "__ydotool_click__".into(),
                        x.to_string(),
                        y.to_string(),
                        code.into(),
                    ],
                ))
            }
            Tool::Wtype => None,
        }
    }

    fn plan_scroll(
        &self,
        x: f32,
        y: f32,
        delta_x: i32,
        delta_y: i32,
    ) -> Option<(Tool, Vec<String>)> {
        let p = self.pointer.as_ref()?;
        let (x, y) = (x.round() as i64, y.round() as i64);
        if p.tool == Tool::Ydotool {
            if delta_x == 0 && delta_y == 0 {
                return None;
            }
            // Wheel events go to the pointer's current position, so move to
            // the requested point first. The sentinel keeps negative layout
            // coordinates working.
            return Some((
                p.tool,
                vec![
                    "__ydotool_scroll__".into(),
                    x.to_string(),
                    y.to_string(),
                    delta_x.to_string(),
                    delta_y.to_string(),
                ],
            ));
        }
        let mut args = vec!["mousemove".into(), x.to_string(), y.to_string()];
        let repeat = |n: i32, btn: &str, args: &mut Vec<String>| {
            for _ in 0..n.unsigned_abs() {
                args.push("click".into());
                args.push(btn.into());
            }
        };
        // delta_y > 0 = scroll down (button 5), < 0 = up (button 4).
        if delta_y != 0 {
            repeat(delta_y, if delta_y > 0 { "5" } else { "4" }, &mut args);
        }
        if delta_x != 0 {
            repeat(delta_x, if delta_x > 0 { "7" } else { "6" }, &mut args);
        }
        Some((p.tool, args))
    }

    fn plan_type(&self, text: &str) -> Option<(Tool, Vec<String>)> {
        let p = self.typer.as_ref()?;
        match p.tool {
            Tool::Wtype => Some((p.tool, vec!["--".into(), text.into()])),
            Tool::Xdotool => Some((
                p.tool,
                vec![
                    "type".into(),
                    "--clearmodifiers".into(),
                    "--".into(),
                    text.into(),
                ],
            )),
            Tool::Ydotool => Some((p.tool, vec!["type".into(), "--".into(), text.into()])),
        }
    }

    fn plan_key(&self, key: &str) -> Option<(Tool, Vec<String>)> {
        let p = self.keys.as_ref()?;
        match p.tool {
            Tool::Xdotool => Some((
                p.tool,
                vec!["key".into(), "--clearmodifiers".into(), key.into()],
            )),
            Tool::Wtype => Some((p.tool, vec!["-k".into(), key.into()])),
            Tool::Ydotool => {
                // ydotool key takes Linux keycodes, not names. Map the common
                // navigation keys and refuse combinations we cannot encode.
                let code = match key {
                    "Return" | "Enter" => "28",
                    "Tab" => "15",
                    "Escape" | "Esc" => "1",
                    "Backspace" => "14",
                    "Delete" => "111",
                    "space" | "Space" => "57",
                    "Up" | "ArrowUp" => "103",
                    "Down" | "ArrowDown" => "108",
                    "Left" | "ArrowLeft" => "105",
                    "Right" | "ArrowRight" => "106",
                    "Home" => "102",
                    "End" => "107",
                    "PageUp" => "104",
                    "PageDown" => "109",
                    _ => return None,
                };
                Some((p.tool, vec!["key".into(), code.into()]))
            }
        }
    }

    // ── Execution ─────────────────────────────────────────────────────────

    /// Move to a layout coordinate. uinput's absolute axes cannot represent a
    /// negative coordinate, so warp to the closest non-negative point first
    /// and cover the remainder with relative motion. This keeps clicks on a
    /// monitor placed above or left of the origin working.
    async fn move_pointer_to(&self, path: &std::path::Path, x: i64, y: i64) -> Result<(), String> {
        let absolute_x = x.max(0);
        let absolute_y = y.max(0);
        self.exec_path(
            path,
            &[
                "mousemove".into(),
                "-a".into(),
                absolute_x.to_string(),
                absolute_y.to_string(),
            ],
        )
        .await?;
        let delta_x = x - absolute_x;
        let delta_y = y - absolute_y;
        if delta_x != 0 || delta_y != 0 {
            self.exec_path(
                path,
                &[
                    "mousemove".into(),
                    "--".into(),
                    delta_x.to_string(),
                    delta_y.to_string(),
                ],
            )
            .await?;
        }
        Ok(())
    }

    async fn run(&self, plan: (Tool, Vec<String>)) -> Result<(), String> {
        let (tool, args) = plan;
        if let Some(sentinel) = args.first().map(String::as_str) {
            if sentinel.starts_with("__ydotool_") {
                let path = self
                    .pointer
                    .as_ref()
                    .map(|p| p.path.clone())
                    .ok_or("no pointer backend")?;
                let x = args
                    .get(1)
                    .and_then(|value| value.parse::<i64>().ok())
                    .ok_or("ydotool action is missing an x coordinate")?;
                let y = args
                    .get(2)
                    .and_then(|value| value.parse::<i64>().ok())
                    .ok_or("ydotool action is missing a y coordinate")?;
                self.move_pointer_to(&path, x, y).await?;
                match sentinel {
                    "__ydotool_click__" => {
                        let code = args.get(3).ok_or("ydotool click is missing a button")?;
                        return self.exec_path(&path, &["click".into(), code.clone()]).await;
                    }
                    "__ydotool_scroll__" => {
                        let delta_x = args.get(3).ok_or("ydotool scroll is missing delta_x")?;
                        let delta_y = args.get(4).ok_or("ydotool scroll is missing delta_y")?;
                        return self
                            .exec_path(
                                &path,
                                &[
                                    "mousemove".into(),
                                    "-w".into(),
                                    delta_x.clone(),
                                    delta_y.clone(),
                                ],
                            )
                            .await;
                    }
                    // "__ydotool_move__" (and any future move action) has
                    // already been satisfied by move_pointer_to.
                    _ => return Ok(()),
                }
            }
        }

        let path = match tool {
            Tool::Xdotool => self
                .pointer
                .as_ref()
                .or(self.typer.as_ref())
                .or(self.keys.as_ref()),
            Tool::Ydotool => self
                .pointer
                .as_ref()
                .or(self.typer.as_ref())
                .or(self.keys.as_ref()),
            Tool::Wtype => self.typer.as_ref().or(self.keys.as_ref()),
        }
        .map(|p| p.path.clone())
        .ok_or_else(|| format!("{} backend not available", tool.binary()))?;
        self.exec_path(&path, &args).await
    }

    async fn exec_path(&self, path: &std::path::Path, args: &[String]) -> Result<(), String> {
        if std::env::var("RAVENBOT_INPUT_DRY_RUN").as_deref() == Ok("1") {
            tracing::info!(program = %path.display(), ?args, "input dry-run");
            return Ok(());
        }
        let mut cmd = Command::new(path);
        cmd.args(args).kill_on_drop(true);
        let output = cmd
            .output()
            .await
            .map_err(|e| format!("failed to run {}: {}", path.display(), e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "{} failed: {}",
                path.display(),
                stderr.trim().lines().next().unwrap_or("unknown error")
            ));
        }
        Ok(())
    }

    // ── Public actions ────────────────────────────────────────────────────

    pub async fn move_mouse(&self, x: f32, y: f32) -> Result<(), String> {
        self.run(self.plan_move(x, y).ok_or("pointer control unavailable")?)
            .await
    }

    pub async fn click(&self, x: f32, y: f32, button: Button) -> Result<(), String> {
        self.run(
            self.plan_click(x, y, button)
                .ok_or("pointer control unavailable")?,
        )
        .await
    }

    pub async fn scroll(&self, x: f32, y: f32, delta_x: i32, delta_y: i32) -> Result<(), String> {
        self.run(
            self.plan_scroll(x, y, delta_x, delta_y)
                .ok_or("scrolling is not supported by the detected backend")?,
        )
        .await
    }

    pub async fn type_text(&self, text: &str) -> Result<(), String> {
        self.run(
            self.plan_type(text)
                .ok_or("typing is not supported by the detected backend")?,
        )
        .await
    }

    pub async fn press_key(&self, key: &str) -> Result<(), String> {
        self.run(
            self.plan_key(key)
                .ok_or("key presses are not supported by the detected backend")?,
        )
        .await
    }

    /// Plan an action without executing (used by tests and the UI preview).
    pub fn describe_plan(
        &self,
        action: &crate::computer_control::ComputerAction,
    ) -> Option<String> {
        use crate::computer_control::ComputerAction as A;
        let plan = match action {
            A::MoveMouse { x, y } => self.plan_move(*x, *y),
            A::Click { x, y, button } => self.plan_click(*x, *y, map_button(button)),
            A::DoubleClick { x, y } => self.plan_click(*x, *y, Button::Left),
            A::RightClick { x, y } => self.plan_click(*x, *y, Button::Right),
            A::Scroll {
                x,
                y,
                delta_x,
                delta_y,
            } => self.plan_scroll(*x, *y, *delta_x, *delta_y),
            A::TypeText { text } => self.plan_type(text),
            A::PressKey { key } => self.plan_key(key),
            A::Wait { .. } => return Some("wait".to_string()),
        }?;
        Some(format!("{} {}", plan.0.binary(), plan.1.join(" ")))
    }
}

fn map_button(button: &crate::computer_control::MouseButton) -> Button {
    match button {
        crate::computer_control::MouseButton::Left => Button::Left,
        crate::computer_control::MouseButton::Middle => Button::Middle,
        crate::computer_control::MouseButton::Right => Button::Right,
    }
}

fn find(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::computer_control::{ComputerAction, MouseButton};

    fn x11_injector() -> InputInjector {
        let xdotool = find("xdotool").map(|p| Program {
            tool: Tool::Xdotool,
            path: p,
        });
        InputInjector {
            pointer: xdotool.clone(),
            typer: xdotool.clone(),
            keys: xdotool,
        }
    }

    #[test]
    fn xdotool_click_plan_is_correct() {
        let injector = x11_injector();
        if injector.pointer.is_none() {
            return; // xdotool not installed on this host
        }
        let plan = injector.plan_click(100.0, 200.0, Button::Right).unwrap();
        assert_eq!(plan.0, Tool::Xdotool);
        assert_eq!(plan.1, vec!["mousemove", "100", "200", "click", "3"]);
    }

    #[test]
    fn xdotool_type_and_key_plans_are_correct() {
        let injector = x11_injector();
        if injector.typer.is_none() {
            return;
        }
        let typeplan = injector.plan_type("hello world").unwrap();
        assert_eq!(typeplan.1.join(" "), "type --clearmodifiers -- hello world");
        let keyplan = injector.plan_key("Return").unwrap();
        assert_eq!(keyplan.1, vec!["key", "--clearmodifiers", "Return"]);
    }

    #[test]
    fn scroll_maps_direction_to_buttons() {
        let injector = x11_injector();
        if injector.pointer.is_none() {
            return;
        }
        let down = injector.plan_scroll(0.0, 0.0, 0, 3).unwrap();
        assert_eq!(down.1.iter().filter(|a| *a == "click").count(), 3);
        assert!(down.1.contains(&"5".to_string()));
        let up = injector.plan_scroll(0.0, 0.0, 0, -2).unwrap();
        assert!(up.1.contains(&"4".to_string()));
    }

    #[test]
    fn describe_plan_covers_actions() {
        let injector = x11_injector();
        if injector.pointer.is_none() {
            return;
        }
        let desc = injector.describe_plan(&ComputerAction::Click {
            x: 5.0,
            y: 6.0,
            button: MouseButton::Left,
        });
        assert!(desc.unwrap().contains("mousemove 5 6 click 1"));
    }

    fn ydotool_injector() -> InputInjector {
        let tool = Program {
            tool: Tool::Ydotool,
            path: PathBuf::from("ydotool"),
        };
        InputInjector {
            pointer: Some(tool.clone()),
            typer: Some(tool.clone()),
            keys: Some(tool),
        }
    }

    #[test]
    fn ydotool_uses_absolute_coordinates_and_click_masks() {
        let injector = ydotool_injector();
        assert_eq!(
            injector.plan_move(10.0, 20.0).unwrap().1,
            vec!["mousemove", "-a", "10", "20"]
        );
        // 0x00 is a ydotool no-op: a real click needs the down+up mask.
        assert_eq!(
            injector.plan_click(10.0, 20.0, Button::Left).unwrap().1,
            vec!["__ydotool_click__", "10", "20", "0xC0"]
        );
        assert_eq!(
            injector.plan_click(10.0, 20.0, Button::Right).unwrap().1[3],
            "0xC1"
        );
        assert_eq!(
            injector.plan_click(10.0, 20.0, Button::Middle).unwrap().1[3],
            "0xC2"
        );
        // A negative y cannot be an absolute uinput axis; the plan switches
        // to the two-step move sentinel.
        assert_eq!(
            injector.plan_move(10.0, -50.0).unwrap().1,
            vec!["__ydotool_move__", "10", "-50"]
        );
    }

    #[test]
    fn ydotool_wheel_and_simple_keys_are_planned_honestly() {
        let injector = ydotool_injector();
        assert_eq!(
            injector.plan_scroll(5.0, 6.0, 0, 3).unwrap().1,
            vec!["__ydotool_scroll__", "5", "6", "0", "3"]
        );
        assert_eq!(injector.plan_key("Return").unwrap().1, vec!["key", "28"]);
        assert!(injector.plan_key("ctrl+c").is_none());
    }

    #[test]
    fn no_backend_reports_unavailable() {
        let injector = InputInjector {
            pointer: None,
            typer: None,
            keys: None,
        };
        assert!(!injector.is_available());
        assert!(injector.plan_click(0.0, 0.0, Button::Left).is_none());
        assert!(injector.plan_type("x").is_none());
    }

    #[test]
    fn detection_reports_a_backend_on_this_host() {
        let injector = InputInjector::detect();
        // Either a backend exists or the machine is headless; both are valid,
        // but the summary must always be well-formed.
        assert!(injector.backend_summary().starts_with("pointer="));
        eprintln!("detected input backend: {}", injector.backend_summary());
    }
}
