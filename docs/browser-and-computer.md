# Browser and computer use

RAVENBOT gives an agent two different ways to operate a machine. They are
deliberately separate: fetching a page is not the same as driving the visible
browser, and the browser is not the same as the desktop.

## Browser

### The dock

The browser dock is a **native child webview**, not an iframe. Remote sites set
`X-Frame-Options`/`frame-ancestors`, so an iframe cannot show most real pages.
The dock:

- keeps cookies, logins, history, and scroll position while hidden;
- has URL, back, forward, reload, title, and a downloads list;
- accepts only `http(s)` URLs;
- can be paused with **Take over**, which blocks agent DOM actions until the
  user releases control;
- shows the latest agent action and which agent performed it.

### Tools

| Tool | Use |
|---|---|
| `browser_open` | Show a real page in the dock for a login, booking, visual check, or any flow the user should see |
| `browser_navigate` | Headless fetch plus readable-text extraction, for research where no visible page is needed |
| `browser_dom` | Operate the open page |

`browser_dom` actions:

- **snapshot** — list up to 200 visible links, buttons, inputs, selects, and
  contenteditable elements with a CSS selector, text, name, placeholder, href,
  and disabled state. Agents should call this before guessing selectors.
- **click**, **fill**, **press**, **hover**, **scroll**, **highlight** — real
  page actions. Clicked/filled/hovered elements scroll into view and flash a
  visible outline.
- **select_option** — choose one or more `<select>` options by value or visible
  label. A single-select accepts one value.
- **set_checked** — set a checkbox or radio to the requested state using the
  native click path so the page's handlers run.
- **text**, **html** — read the page.
- **back**, **forward**, **reload**, **wait** — navigate and pace a flow.
- **wait_for** — poll a selector, text or URL substring until it matches, up to
  30 seconds. Selector state can be `visible`, `hidden` or `attached`. It never
  waits blindly: at least one condition is required.
- **run_steps** — run 1–200 of the actions above in order and stop on the first
  failure. The payload is capped at 20,000 characters. `open`, `eval` and nested
  `run_steps` are refused. The result reports the completed count and the last
  step result, not every intermediate value.
- **eval** — run JavaScript in the page origin.

Selectors and text are escaped before they enter the script. The script runs in
the remote page's origin and cannot reach the Tauri IPC bridge or app storage.

On Linux, evaluation uses WebKitGTK's result-bearing `run_javascript` callback.
Other platforms return an honest "not available" until they have their own
bridge.

## Computer

### The host desktop

`computer_control` drives the real desktop through `xdotool` (X11) or
`ydotool`/`wtype` (Wayland). Mutating actions are high-risk. Under the default
Full ("YOLO") policy they run without a card; RAVENBOT's safety policy still
hard-blocks destructive shell/database/file operations. Choose Ask or Auto on
an agent that should still present approval cards. Host control is **off by
default** per agent and must be enabled in the agent's settings.

Actions:

| Action | Result |
|---|---|
| `windows` | List open windows with id, title, app name, size, minimized/focused state |
| `focus_window` | Focus a window by id |
| `screenshot_window` | Capture one window by id |
| `screenshot` | Capture the full screen |
| `click`, `double_click`, `right_click`, `move` | Pointer actions at screen coordinates |
| `drag` | Hold the left button and drag between two screen coordinates (max 10 s) |
| `type`, `key` | Text and key input |
| `scroll` | Scroll at a screen point |
| `wait` | Pause without touching the machine |
| `app_launch` | Launch an app, `.desktop` id, executable, file, or URL without a shell |
| `clipboard_read` | Read current clipboard text (capped at 64 KiB) |
| `clipboard_write` | Replace clipboard text (capped at 64 KiB) |

Window focus uses `xdotool`. Strict Wayland compositors may refuse it; the tool
reports the window manager's error rather than pretending the focus happened.
Pointer, keyboard, and drag input are Linux-only today. App launch and
clipboard use native helpers on macOS and Windows; [`COMPUTER-USE.md`](COMPUTER-USE.md)
records the exact matrix, backend detection, and the limits that are not faked.

### The Computer panel

The panel opens from an agent's chat header and shows:

- a live screen preview;
- **Take over / Release control**, which pauses agent input while leaving
  screenshots and window listing available;
- the latest action with the performing agent's name;
- a click marker at the last pointer action;
- a short action log;
- a **Windows** tab with focus and capture controls;
- the isolated Docker desktop tab for a separate, disposable machine.

### Safety

- Host control is opt-in per agent (`bot.config.host_control`).
- Wayland host control is fail-closed unless
  `RAVENBOT_ALLOW_WAYLAND_CONTROL=1` is set.
- Agent input can be paused at any time; the skill checks the pause flag before
  every mutating action.
- The whole `browser_dom` skill is High risk, so Ask/Auto gates the tool as a
  unit; the read-only/mutating split below describes what an action does, not a
  separate permission bypass.
- Read-only inspection: `snapshot`, `text`, `html`, `scroll`, `highlight`,
  `hover`, `back`, `forward`, `wait`, `wait_for`; computer `windows`,
  `screenshot`, `screenshot_window`, `wait`.
- Mutating: `click`, `fill`, `press`, `select_option`, `set_checked`, `reload`,
  `run_steps`, `eval`, `focus_window`, pointer/keyboard actions, `drag`,
  `app_launch`, and clipboard reads/writes. Reload remains high-risk because it
  can resubmit a form; clipboard reads are High because the clipboard can hold
  credentials; app launch can start arbitrary local code.
- Activity events contain action, selector/coordinates, a truncated launch
  target, and result, **never typed text, single-character key presses,
  clipboard contents, or page contents**.
- The remote page cannot reach the app IPC bridge.
- The isolated Docker desktop remains available when host control is blocked.

## Recommended agent flows

Browser login or form:

1. `browser_open` the URL.
2. `browser_dom` action `snapshot`.
3. `browser_dom` `fill`/`select_option`/`set_checked`/`click` using selectors from
   the snapshot.
4. `browser_dom` `wait_for` the result text or URL, then `text` to confirm.
5. For a repeated flow, pass the same steps to `run_steps` so it stops at the
   first failed step instead of continuing blindly.
6. Leave the dock open so the user can watch or take over.

Desktop app task:

1. `computer_control` action `windows`.
2. `focus_window` the target app.
3. `screenshot_window` to see its current state.
4. `click`/`type` at coordinates from the screenshot.
5. `screenshot` to verify, then report.

## Remaining work

- Browser tabs and named profiles.
- A page-only screenshot cropped to the dock, instead of a full-screen capture.
- Per-action browser leases, so an in-flight action is cancelled when the user
  takes over.
- Native DOM bridges on macOS and Windows.
- Native pointer, keyboard, and drag backends on macOS and Windows.
- Wayland-native window activation.
- Multi-monitor scaling for the click marker when the app is not on the primary
  monitor.
- Shadow DOM and iframe traversal in `snapshot`; today the agent can target
  them with an explicit selector or `eval`.
- A same-dock policy for `target="_blank"` links; today the snapshot exposes
  `href`, and the agent can call `browser_open` with it.
