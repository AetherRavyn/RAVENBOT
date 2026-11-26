# Computer use

`computer_control` lets an agent operate the machine that RAVENBOT is running
on. It is deliberately separate from the browser dock: a page action is not a
desktop action, and neither is a sandbox.

The contract is the same as the rest of the harness: **do the real thing or
return an explicit error**. A missing backend never becomes a fabricated
success.

## Actions

| Action | What it does |
|---|---|
| `windows` | List open windows with id, title, app name, size, minimized/focused state |
| `focus_window` | Focus a window by id |
| `screenshot`, `screenshot_window` | Capture the screen or one window |
| `move`, `click`, `double_click`, `right_click` | Pointer actions at screenshot coordinates |
| `drag` | Hold the left button and move from one screenshot coordinate to another |
| `scroll` | Scroll at a screenshot point |
| `type`, `key` | Text and key input |
| `wait` | Pause without touching the machine |
| `app_launch` | Launch an application, `.desktop` id, executable, file, or URL |
| `clipboard_read` | Read the current clipboard text (capped at 64 KiB) |
| `clipboard_write` | Replace the clipboard with text (capped at 64 KiB) |

Pointer coordinates are pixels in the most recent screenshot. If the captured
desktop does not start at `(0, 0)`, the screenshot reports `origin_x` /
`origin_y` and the skill translates image pixels to desktop layout points.
`drag` applies the same translation to both endpoints.

`app_launch` returns the spawned PID and the strategy that was used. It does
not wait for the application to exit or claim that it reached a usable window.
`clipboard_write` returns the byte count and backend; `clipboard_read` returns
the text, the original byte count, and whether it was truncated. These fields
are in the tool output (`text`, `bytes`, `truncated` for a read; `pid`,
`strategy`, `program`, `args` for a launch).

## Platform matrix

| Capability | Linux X11 | Linux Wayland | macOS | Windows |
|---|---|---|---|---|
| Pointer, keyboard, drag | `xdotool` | `ydotool` pointer / `wtype` typing | **Not implemented** | **Not implemented** |
| App launch | `gtk-launch`, then `xdg-open`, then direct exec | Same as X11 | `open -a` for an app name, else `open` | Direct exec for an executable; `explorer.exe` for a path or URL |
| Clipboard | `xclip -selection clipboard`, then `xsel` | `wl-paste` / `wl-copy`, with XWayland fallback | `pbpaste` / `pbcopy` | PowerShell `Get-Clipboard -Raw` / `Set-Clipboard` |
| Screen capture | Supported | Supported | Supported | Supported |
| Window list / focus | Supported where `xdotool`/`hyprctl` can see the session | Compositor-dependent; focus can be refused | Not implemented | Not implemented |

macOS and Windows have **no pointer, keyboard, or drag backend**. Those actions
return an explicit unavailable error. App launch and clipboard are implemented
there because they use native OS helpers rather than synthetic input. Their
command selection is unit-tested; the live macOS and Windows helpers were not
exercised on those hosts as part of this change.

## Backend detection

Detection happens when a controller is created, not from a static platform
assumption:

- `InputInjector` looks for `xdotool`, `ydotool`, and `wtype` on `PATH` and
  chooses the pointer/typing/key backend for the session. On Wayland it prefers
  `ydotool` + `wtype`; on X11 it prefers `xdotool`.
- `AppLauncher` looks for `gtk-launch` and `xdg-open` on Linux, uses `open` on
  macOS, and uses `explorer.exe` on Windows. The target decides whether a
  helper is appropriate.
- `Clipboard` looks for the Wayland, X11, macOS, or PowerShell tools. If only
  one direction is available, only that direction is reported as available.

`computer_capabilities` reports the detected input summary, clipboard
read/write availability and backend name, and the preferred app-launch helper.
The Computer panel shows the same values. A backend can disappear between calls
(for example, a package is removed or the session changes), so each action
reports its own failure rather than trusting a cached capability. Detection is
binary presence, not permission: a Wayland compositor can still refuse the
clipboard protocol, and a launcher can still reject an unknown target.

## Approval and risk model

`computer_control` is a **High** risk skill. Every action except read-only
observation goes through the same gate as clicks and typing:

1. The per-agent **host control** switch must be on.
2. On Wayland, host control is fail-closed until the user opts in with
   `RAVENBOT_ALLOW_WAYLAND_CONTROL=1` or the Settings switch.
3. The process-wide **takeover** flag pauses agent input. Observation
   (`windows`, screenshots, `wait`) stays available while the user has control.
4. The bot's approval mode applies: `Ask` presents a card for non-read-only
   actions, `Auto` presents a card for High-risk actions, `Full` bypasses the
   card. High-risk actions are still subject to the safety policy.
5. The mutation runs under the same global mutating-tool lock as shell, Git,
   Docker, and database tools.

`app_launch`, `clipboard_read`, `clipboard_write`, and `drag` are all High.
Clipboard **reads** are High even though they do not change the machine: the
clipboard routinely contains passwords, tokens, and recovery codes. Launching a
program is High because it can start arbitrary local code. Dragging is High
because a stuck button or an unintended drop can change or destroy real data.

Takeover is checked before the action is dispatched. There is no mid-drag
cancellation: a drag is bounded to ten seconds and must finish or fail. This is
an honest limit, not a queue that silently drops the gesture.

## What is never logged

Activity events and tracing use:

- pointer coordinates;
- a truncated launch target;
- typed character counts, never the text;
- key names only when longer than one character;
- clipboard **byte/character counts**, never the contents.

Clipboard contents are returned to the calling model as tool output because
that is the purpose of the action, but they are not copied into the activity
feed or application logs.

## Honest limits

- **Linux-only synthetic input.** macOS and Windows need their own native input
  backends; until then `move`, `click`, `scroll`, `drag`, `type`, and `key`
  fail explicitly there.
- **App launch is not a sandbox.** A direct fallback can start any executable
  the user can run. The approval/takeover gate is the control boundary, not an
  application allowlist. `target` is validated (non-empty, bounded, no control
  characters, no leading option dash) and passed as argv, never through
  `sh -c` or `cmd /C`, but the launched program itself is not confined.
- **Spawn success is not readiness.** `app_launch` can return a PID while the
  application is still starting, crashing, or showing a modal. Verify with
  `windows` or a screenshot when the task depends on it.
- **Clipboard is text-only and current-selection-only.** There is no MIME
  negotiation, image or rich-text support, clipboard history, or primary
  selection action. A clipboard read can expose secrets; treat it like reading
  a credential store.
- **Empty clipboard errors depend on the tool.** `xclip`, `xsel`, and
  `wl-paste` differ in how they report "nothing is copied"; RAVENBOT surfaces
  the tool's error rather than inventing an empty success. Clipboard commands
  time out after ten seconds instead of wedging the agent loop.
- **Drag validation is strict.** Negative, NaN, and infinite coordinates are
  rejected; durations above 10 seconds are rejected. This is intentional even
  though clicks can use negative layout coordinates on multi-monitor setups.
- **Window control is compositor-dependent.** Strict Wayland compositors can
  refuse focus; the tool reports the window manager's error.
- **App names are heuristics.** `gtk-launch` receives a desktop id when the
  target looks like one; `open -a` is used on macOS for a target that is not a
  path or URL. A weird target can still reach the helper and fail there; the
  helper's exit or spawn error is reported.
- **The browser is separate.** Browser actions are documented in
  [`browser-and-computer.md`](browser-and-computer.md); the desktop skill does
  not script page DOM.

## Verification

The pure planning layer is tested without a display:

- `cargo test -p ravenbot-vision` covers app-launch selection and validation,
  clipboard backend selection and caps, and xdotool/ydotool drag argv including
  the ydotool `0x40` down / `0x80` up masks.
- `cargo test -p ravenbot-skills` covers the schema enum, High risk, argument
  validation, and takeover refusal.
- `cargo test -p ravenbot` covers the runtime risk mapping for the new actions.

Live clipboard and app-launch checks are marked `#[ignore]` and opt-in:

- `RAVENBOT_LIVE_CLIPBOARD=1 cargo test -p ravenbot-vision live_clipboard_read_is_opt_in -- --ignored`
- `RAVENBOT_LIVE_LAUNCH_TARGET=<target> cargo test -p ravenbot-vision live_app_launch_is_opt_in -- --ignored`

They are not part of the headless test path because a CI runner has no desktop
session, no clipboard owner, and must not launch arbitrary applications. The
launch check still only proves that the process was spawned.
