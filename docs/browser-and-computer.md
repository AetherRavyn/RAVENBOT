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
- **click**, **fill**, **press**, **scroll**, **highlight** — real page actions.
  Clicked/filled elements scroll into view and flash a visible outline.
- **text**, **html** — read the page.
- **back**, **forward**, **reload**, **wait** — navigate and pace a flow.
- **eval** — run JavaScript in the page origin.

Selectors and text are escaped before they enter the script. The script runs in
the remote page's origin and cannot reach the Tauri IPC bridge or app storage.

On Linux, evaluation uses WebKitGTK's result-bearing `run_javascript` callback.
Other platforms return an honest "not available" until they have their own
bridge.

## Computer

### The host desktop

`computer_control` drives the real desktop through `xdotool` (X11) or
`ydotool`/`wtype` (Wayland). Every mutating action is high-risk and goes through
the approval broker unless the agent is in Full mode. Host control is also
**off by default** per agent and must be enabled in the agent's settings.

Actions:

| Action | Result |
|---|---|
| `windows` | List open windows with id, title, app name, size, minimized/focused state |
| `focus_window` | Focus a window by id |
| `screenshot_window` | Capture one window by id |
| `screenshot` | Capture the full screen |
| `click`, `double_click`, `right_click`, `move` | Pointer actions at screen coordinates |
| `type`, `key` | Text and key input |
| `scroll` | Scroll at a screen point |
| `wait` | Pause without touching the machine |

Window focus uses `xdotool`. Strict Wayland compositors may refuse it; the tool
reports the window manager's error rather than pretending the focus happened.

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
- Read-only inspection (`snapshot`, `text`, `html`, `scroll`, `highlight`,
  `back`, `forward`, `wait`; computer `windows`, `screenshot`,
  `screenshot_window`, `wait`) bypasses the approval card. `click`, `fill`,
  `press`, `reload`, `eval`, `focus_window`, and all pointer/keyboard actions
  remain high-risk. Reload is high-risk because it can resubmit a form.
- Activity events contain action, selector/coordinates, and result, **never
  typed text, single-character key presses, or page contents**.
- The remote page cannot reach the app IPC bridge.
- The isolated Docker desktop remains available when host control is blocked.

## Recommended agent flows

Browser login or form:

1. `browser_open` the URL.
2. `browser_dom` action `snapshot`.
3. `browser_dom` `fill`/`click` using selectors from the snapshot.
4. `browser_dom` `text` or `press` to confirm the result.
5. Leave the dock open so the user can watch or take over.

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
- Wayland-native window activation.
- Multi-monitor scaling for the click marker when the app is not on the primary
  monitor.
- Shadow DOM and iframe traversal in `snapshot`; today the agent can target
  them with an explicit selector or `eval`.
- A same-dock policy for `target="_blank"` links; today the snapshot exposes
  `href`, and the agent can call `browser_open` with it.
