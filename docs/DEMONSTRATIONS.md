# Teach a browser workflow

RAVENBOT can learn a skill by watching a person use the browser dock: press
**Teach**, do the task once, press **Stop**, review the generated `SKILL.md`,
and save it as an ordinary versioned skill. **Replay** later performs the
recorded actions in the open dock and reports what happened.

This is the browser half of demonstration learning. Desktop recording
(`computer_control`), automatic DOM rewriting, and model-based repair are not
implemented. The capture is deterministic, the review gate is mandatory, and a
failed replay is reported as a failure rather than repaired silently.

## Capture rules

The recorder is `crates/skills/assets/demo-recorder.js`. Rust injects it into
the active tab with the page evaluator; the file is also read directly by the
Vitest suite so the tested JavaScript is the shipped JavaScript.

It records one active tab:

| Event | What is captured |
|---|---|
| `navigate` | The start URL and any later URL change |
| `click` | The nearest interactive ancestor (`a`, `button`, clickable `input`, `[role=button]`, `[role=link]`, `[contenteditable]`), including checkbox/radio state. A `<select>` click is skipped because its `change` is the meaningful action. |
| `fill` | Text inputs, textareas, selects and contenteditable elements |
| `press` | Only `Enter`, `Escape` and `Tab` |

A typing burst for one field is one `fill`, not one per keystroke. A recording
stops at **200 actions** or **10 minutes** and returns a truncation warning.
Clicks on file inputs are not recorded because a file chooser cannot be replayed
from the DOM.

### Surviving navigation

The recorder lives in the page's JavaScript context, so a navigation replaces
it. Rust keeps the authoritative event log:

1. A 200 ms harvester copies the page buffer into the process.
2. Every finished page load for the recording tab re-injects the recorder.
3. Rust writes the navigation boundary as a `navigate` action, because the old
   page cannot observe the load event.

The recorder deliberately does **not** mirror events into `localStorage` or
`sessionStorage`: the remote site's own scripts could read them, and a fill
value is not safe to leave in page storage. That choice means an event fired in
the last few milliseconds before a hard navigation can be lost when the 200 ms
poll has not run. RAVENBOT does not claim perfect capture across navigation; the
review screen is where a missing step is noticed, and re-recording is how the
recording is corrected.

## Selector and fallback strategy

For each control the recorder stores one primary selector and up to three
fallbacks, selected in this order:

1. `data-testid` / `data-test`, then a stable `id`;
2. `name`, `aria-label`, `placeholder`;
3. a bounded CSS path with `:nth-of-type` up to a stable ancestor.

It also stores a human `label`: `aria-label`, visible text for a button/link, a
form field's `name` or `<label>`, and visible text as a last resort. Generated
ids (long hex hashes, React `:r…:` ids) are not treated as stable.

Selectors are data. They are validated for length and control characters, never
evaluated while recording, and always passed to `document.querySelector` inside
a `try` block during replay.

## Secret exclusions

The recorder never reads the value of:

- `input[type=password]` and `input[type=file]`;
- fields whose `autocomplete` contains `current-password`, `new-password`,
  `one-time-code` or `cc-`;
- fields whose `name`, `id`, `aria-label`, `placeholder` or `data-testid`
  matches password/passcode/OTP/token/secret/card-number/CVV markers;
- any field, including a contenteditable, inside a password-manager card
  (1Password, LastPass, Bitwarden, Dashlane and similar markup).

A sensitive fill is still recorded as an action so replay can stop at the right
place. Its `sensitive` flag is `true`, its `text` is absent, and its label is
taken from attributes rather than the field's contents. The value never enters
the event buffer, `demo.json`, logs or a skill document. File-input actions are
omitted entirely.

All free text that does reach the draft passes through the same credential
redaction as run learning (`Bearer`, cloud/provider tokens, sensitive
`KEY=value` assignments). The scanner is pattern-based; it cannot prove that
every secret shape is absent, which is why the review gate remains mandatory.

Non-sensitive values are stored. The text you type into a non-sensitive field
is kept in `demo.json` because replay needs it; review the draft before saving,
and use the built-in sensitive-field exclusions for secrets.

## Review before saving

**Stop** returns a `SkillDraft` and opens the review dialog titled **Teach this
workflow**. The dialog shows the proposed name, steps, tools, warnings and an
editable `SKILL.md`, exactly like a run-learned draft. Nothing is written until
the person presses **Save & install** or **Save without installing**.

The generated document says **“Demonstrated on …”**, lists the observed steps,
and explicitly does not claim that the workflow was re-run or verified. It
warns about sensitive manual steps, synthetic key events, fallback behavior and
the action cap.

## `demo.json`

Saving a demonstration writes two files through the same
`LocalSkillLibrary::create_with_source` path as every other skill:

```text
<root>/library/<id>/<version>/SKILL.md
<root>/library/<id>/<version>/demo.json
```

`demo.json` is the normalized `Demonstration`:

```json
{
  "version": 1,
  "start_url": "https://example.com/app",
  "recorded_at": "2026-11-26T10:00:00Z",
  "actions": [
    {
      "kind": "click",
      "url": "https://example.com/app",
      "selector": "[data-testid=\"search\"]",
      "fallbacks": ["#go"],
      "label": "Search",
      "sensitive": false
    }
  ]
}
```

Provenance is `demonstrated:<recorded_at>` (for example
`demonstrated:2026-11-26T10:00:00Z`), and the version note names the start URL
and time. The library list and view expose `has_demonstration` when the
latest/installed files include `demo.json`. Installing is still a separate,
explicit step, and replay requires the installed copy.

Replay executes `demo.json`, not the English step list in `SKILL.md`. Editing
the document during review changes what a future agent reads; it does not
change the recorded selectors, values or actions. To change the replay, record
the demonstration again (or edit `demo.json` deliberately).

## Replay semantics

Replay is the **Replay** action on a `has_demonstration` skill. It asks for
confirmation because it will click and fill a real page, then:

1. requires an open browser dock tab and the installed skill;
2. refuses while the person has taken over the dock;
3. navigates to `start_url` and waits briefly;
4. executes one action at a time through the page evaluator;
5. waits 250–500 ms after navigation or click and records the resulting URL.

For each action the page helper tries the primary selector, then the recorded
fallbacks, then the human label (`aria-label`, `name`/`id`, `<label>` text,
visible text). A fallback or label match is counted as **repaired** and reported
with the selector that matched or the label strategy that found it. The skill is
never rewritten automatically.

Replay stops at the first failed step. A failure report contains:

```json
{
  "ok": false,
  "completed": 2,
  "total": 5,
  "failed_index": 2,
  "action": { "kind": "click", "selector": "#submit" },
  "page_url": "https://example.com/app",
  "attempted": ["#submit", ".primary", "label:Submit"],
  "error": "No element matched the recorded selector, its fallbacks, or the recorded label.",
  "repair_hint": "Replay tried `#submit`, `.primary` and then the label \"Submit\". Open …"
}
```

A successful report has `ok: true`, `completed == total`, the final `page_url`,
the `repaired` count, and per-step repair entries. Warnings may explain a
synthetic key press or a click that navigated before WebKit returned its result.

### Sensitive steps are manual

If a `fill` is `sensitive`, replay stops **before** touching the field:

```json
{
  "ok": false,
  "needs_input": true,
  "error": "This step needs your input: the demonstration intentionally did not record the value for this sensitive field.",
  "repair_hint": "This value was deliberately not recorded. In <page>, fill \"Password\" manually, …"
}
```

It does not send an empty value, does not read the field, and does not count the
step as completed. Fill the field yourself and complete the workflow, or record
a demonstration that does not include that sensitive field. Replay always
restarts from the beginning; it does not resume halfway.

### Key presses

Enter, Escape and Tab are replayed as synthetic `KeyboardEvent`s. Synthetic
events do not trigger native browser default actions by themselves, so the
helper requests submission of the enclosing form for Enter when no handler
cancelled the event and the form passes validation, and reports
`synthetic: true` / `submitted_form`. Escape and Tab only act if the page's own
handlers respond. This is an honest limitation, not a success claim.

## Failure behavior

A replay never reports success for an action it could not perform:

- a selector miss tries the fallbacks and label, then stops;
- a select whose recorded option no longer exists fails with a repair hint;
- a page that does not answer the evaluator fails unless the URL changed, which
  is treated as evidence that a click/press navigated;
- a sensitive field is a manual step;
- the dock must be open, the skill installed, and the platform must expose the
  Linux DOM evaluator. Elsewhere the command returns
  “Browser DOM actions are not available on this platform yet.”

## Limits

- **Browser only.** There is no desktop demonstration capture. A workflow that
  leaves the browser must be authored as a normal skill.
- **Linux evaluator only.** Replay and recording need the WebKitGTK
  result-bearing DOM bridge. macOS and Windows return the explicit unavailable
  error; they do not fake a replay.
- **No automatic DOM repair.** Fallbacks are tried and reported. The skill and
  `demo.json` are not rewritten, and no model is asked to invent a new selector.
- **No verification claim.** A completed replay is not proof that the page
  reached the intended state. Check the page's own success criteria. The page is
  remote, untrusted content and can script around the evaluator; the replay
  report is a record of attempted actions, not a security boundary.
- **Navigation races.** A last-millisecond event before a hard navigation can
  be lost; the 200 ms poll is best effort. A slow page may not be ready after
  the fixed replay wait.
- **One tab.** Recording follows the tab that was active when Teach was pressed.
  Closing the dock, closing that tab, switching browser profiles, or destroying
  the dock cancels the recording.
- **Contenteditable is text-only.** Replay writes `textContent`; rich formatting
  inside a contenteditable is not reconstructed.
- **No shadow DOM or iframe traversal.** The recorder and replay helper query the
  top document. A control inside a closed shadow root or a cross-origin iframe
  is not recorded, and replay cannot reach it.
- **No popup replay.** A click that opens a new window/tab can create another
  dock tab; replay stays pinned to the original tab and stops if the active tab
  changes, so later steps may not run.
- **Bounded.** 200 actions, 10 minutes, 2,000 characters per value, at most four
  selectors per action, and a 500-event Rust buffer with a warning.

## Tests

- `cargo test -p ravenbot-skills --lib` covers normalization, URL-change
  navigation, fill dedupe, the 200 cap, sensitive removal, selector/fallback
  validation, proposal content, `demo.json`, `read_file`, `has_demonstration`,
  and replay script/report parsing.
- `cargo test -p ravenbot --lib` covers `demo.json` save/no-save and role
  access.
- `src/lib/demonstration-assets.test.ts` runs the recorder and replay assets in
  jsdom: click, fill, select, Enter, password/OTP/card omission,
  password-manager contenteditable, selector fallbacks, stop JSON, label
  fallback, failure reporting and sensitive replay refusal.
- The BrowserDock, LearnFromRun and LocalSkillsPanel DOM suites cover the Teach
  flow, prefilled review, Replay confirmation, success and failure rendering.
