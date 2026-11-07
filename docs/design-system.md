# Design system

RAVENBOT's interface is one local-first workspace with one visual grammar:
quiet neutral surfaces, a single accent reserved for focus and selection,
semantic status colours, compact controls, and motion that explains a state
change rather than decorating it. The system is inspired by OpenBot's public
design language, but the implementations here are native Svelte and use
RAVENBOT's own tokens.

## Token layers

`src/lib/styles/tokens.css` is the only declaration site for the default
palette. `src/lib/theme.ts` rewrites the same semantic names at runtime for the
theme catalogue, so a component reads a role, never a colour literal.

| Layer | Tokens | Used for |
|---|---|---|
| Canvas and frames | `--surface-0`, `--surface-1`, `--surface-2`, `--surface-3`, `--surface-4`, `--hairline`, `--hairline-strong` | App frame, panels, raised rows, controls |
| Text | `--text-primary`, `--text-secondary`, `--text-tertiary`, `--text-muted`, `--text-faint`, `--text-on-light` | Copy, metadata, disabled text, inverted buttons |
| Accent | `--brand`, `--brand-hover`, `--brand-text`, `--brand-soft`, `--brand-strong` | Focus, links, selected controls, agent activity |
| Status | `--success-*`, `--warning-*`, `--danger-*`, `--question-*` | Outcomes, risk, attention, interactive questions |
| Type | `--text-xs` … `--text-3xl`, `--leading-*`, `--weight-*` | One six-step scale, 11px floor |
| Space | `--space-0-5` … `--space-8` | Padding, gaps, insets |
| Radii | `--radius-xs` … `--radius-modal`, `--radius-pill` | Controls, cards, chat bubbles, dialogs |
| Controls | `--control-xs` … `--control-xl` | Button heights and compact toolbars |
| Motion | `--duration-*`, `--ease-out`, `--ease-in-out` | State changes and entrance motion |

Rules:

1. A component uses a semantic token, not a raw colour.
2. A new token is added only for a new role, in `tokens.css` and, when the role
   is themeable, assigned in `theme.ts`.
3. `prefers-reduced-motion` collapses durations and removes non-essential
   motion; status remains visible through colour and shape.
4. A focusable control has a visible focus ring. A scroll container that is
   reachable by keyboard is focusable.
5. Text never sits below the 11px floor. `type-scale.test.ts` enforces it.

## Chat grammar

The conversation is not a single bubble type. A message is a row with a role,
and the row chooses a surface based on what the content is.

- **User** messages are right-aligned, content-sized, and use the raised surface.
- **Agent** messages are left-aligned. A prose answer uses the bordered muted
  bubble; a code, table, chart, visual, or image answer uses the ghost variant
  so the artifact is not boxed twice.
- **Reasoning** is a quiet panel above the answer. It is evidence, not the
  answer, and it never replaces the answer.
- **Tool work** is a compact action marker above the answer: status glyph, verb,
  argument chip, and elapsed time. It is not a bubble.
- **Every message with text has Copy** in its hover/focus metadata row. The
  button confirms with a check for a moment; it never turns a failed clipboard
  permission into a false success.
- **Long transcripts are windowed.** The feed mounts the latest 30 office
  messages (50 in a one-to-one thread), loads another batch automatically as
  the reader reaches the old edge, and keeps the viewport anchored while rows
  are prepended. A counted "Load earlier messages" control remains the explicit
  path; search and reply jumps reveal the older target before scrolling. On the real 216-message office this reduced mount work from
  ~1.7 s / 20,325 DOM nodes to roughly 0.3 s / ~2,800 nodes.
  The full message list stays in memory, so search, quotes, and reactions still
  see the whole conversation.
- **The office board opens collapsed when the run is settled** and expands when
  a run starts. On a narrow window this gives the conversation the space and
  keeps long DONE previews out of the initial layout; the header still carries
  the progress and the board remains one click away.
- **Transcript navigation** keeps the reader oriented: `Alt+↑` / `Alt+↓` centers
  the previous or next message, and scrolling away from the bottom turns the
  jump-to-latest pill into a counted tally of messages that arrived meanwhile.
  The count clears when the feed is at the bottom again.
- **Replies** show a compact quote card above the bubble: author, two lines of
  the original, and a click that scrolls to the source. The composer repeats
  the quote while the user types, and Escape cancels it. A reply id is
  validated against its own conversation on the backend; an office queue item
  carries the reply target until its turn starts.
- **Reactions** are persisted chips beneath the message. The add button opens a
  fixed palette in the metadata row; choosing an emoji writes through the Tauri
  command and a failed write restores the previous list. A chip is the control
  that removes the reaction, so there is no separate counter to drift.
- **In-thread search** lives in the header. It searches the text each row
  renders, reports `current of total`, wraps at both ends, and scrolls the
  current hit into view. Matches tint the row and mark it with a brand edge;
  the current hit gets a stronger ring. Row-level highlighting is deliberate:
  wrapping substrings inside rendered tables, code, or charts would corrupt what
  the model wrote.
- **Permission requests** and **agent questions** are interaction cards, not
  speech. They pause the run, so they are visually distinct, keyboard reachable,
  and never nested inside the bubble.

### Bubble variants

| Variant | Surface | Use |
|---|---|---|
| `msg-bubble-agent` | `--surface-1`, hairline, 22px radius | Prose answers |
| `msg-bubble-user` | `--surface-2`, right aligned | Human messages |
| `msg-bubble-ghost` | Transparent, no radius | Code, tables, charts, visuals, media |
| Interaction card | `--surface-1`, 16px radius, tone border | Approvals and questions |

The bubble is a presentation shell. Markdown, tables, charts, math, code, and
visuals own their own internal layout.

## Interaction cards

`ApprovalCard.svelte` and `QuestionCard.svelte` are the gate surfaces. They
share `.chat-interaction` geometry and differ only in tone and controls.

- The header has an icon badge, a small role label, a title, and a status chip.
- A high-stakes approval changes the icon, chip, border, and adds the existing
  high-stakes sentence. It does not turn the whole card red; the decision stays
  readable.
- The argument preview is a bounded monospace surface, not a wall of JSON. The
  command, path, URL, or query is shown first.
- Allow is the light primary button. Deny is an outlined danger button. The run
  resumes only after a decision.
- A question shows suggested answer chips, with an optional text field. The
  agent can close the text path when it needs one of the named choices.
- Busy states disable every decision at once, so a double-click cannot send two
  answers.

The cards are used by both the one-to-one thread and the office channel. A fix
to one cannot drift from the other.

### Fresh threads

`ChatEmptyState.svelte` gives a new thread a face, one sentence of purpose, and
three concrete prompts that can be sent as written. It is a launch surface, not
a decorative card: each suggestion calls the same `sendMessage` path as the
composer, and the composer stays visible and primary.

## Content surfaces

- **Markdown** uses a 14/20 body rhythm with a 12px block gap, 600-weight
  headings, and `-0.1px` tracking.
- **Code** is a hairline card with a 32px header, language label, copy, and an
  artifact hand-off. It never widens the message column. Highlighting is
  deferred until the block is within 600px of the viewport, so a transcript
  full of offscreen code pays for colour only when the reader approaches it.
  Lines carry a gutter number generated by a CSS counter; the number is not a DOM text node,
  so selecting or copying a block never includes it. The gutter is sticky while
  a long line scrolls horizontally, and multi-line tokens stay one colour
  across the line break.
- **Tables** use a focusable horizontal scroll region, bounded fixed columns
  that actually wrap, a quiet header fill, row hover, and explicit **Markdown**
  and **CSV** copy actions. A wide table scrolls inside its card; it never
  pushes the message column past the window. A table is data, not a screenshot.
- **Long agent reports** (>3,000 characters) collapse to a 520px preview with
  a surface fade and a **Show full answer** control. Regenerated answers start
  collapsed again; short replies and streaming text are never clamped.
- **Images** use a compact gallery for two or more tiles, but a single image is
  shown at its own aspect ratio: a square crop turns a screenshot, chart, or
  portrait into a thumbnail of itself.
- **Documents** (PDF, archives, spreadsheets, presentations, binaries) render as
  attachment cards with the file name, kind, decoded byte size, and a download
  action. A file preserved on the message must remain retrievable from it.
- **Charts** are native SVG with a responsive `viewBox`, theme tokens, an
  accessible summary, and an SVG download action.
- **Math** is KaTeX, with raw-source fallback for invalid LaTeX.
- **Visuals** run in an isolated `ravenvisual://` origin, without
  `allow-same-origin`, and report their own height.
- **Slides** have a 16:9 stage, keyboard navigation, fullscreen present mode,
  and a progress bar.
- **Designed slide decks** are rendered as code-generated images; the PPTX uses
  one full-bleed image per slide.
- **Exported documents** appear as an Open card in the tool trace.

## Plugins & Native Tools

The plugin picker is a real OpenAPI installer, not a catalog of pre-labelled
actions.

- A card is an **app** (Gmail, Stripe, Jira, …), not a fabricated action.
  Installing fetches the live OpenAPI or `ai-plugin.json` document, parses its
  operations, and stores the parsed rows locally.
- A plugin with zero operations contributes nothing to the model. The old
  `inapp://` stub path returned a fake success without making a request and has
  been removed.
- Operation ids are qualified and sanitized to provider-safe names
  (`stripe__post_charges`, max 64 chars) so an imported spec cannot make the
  whole tools array fail at an OpenAI-shaped provider.
- The input schema merges body and query/path fields. Execution puts declared
  query parameters in the URL even on POST/PUT/PATCH, and sends the remaining
  fields as JSON or `application/x-www-form-urlencoded` according to the spec.
  Swagger 2 `in: body` and `in: formData` parameters are parsed, not ignored.
- Credentials are optional per app, stored locally, and applied as the
  configured header on every operation. The renderer only receives the header
  name and a configured flag, never the secret. When the spec declares an API
  key or bearer scheme, the credential form prefills the right header
  (`X-API-Key`, `Authorization`, …) instead of guessing.
- A spec that omits security metadata is labelled **Auth not declared**, not
  "optional": several real documents (GitHub's, for one) require a token but do
  not say so. The credential form stays available. Curated entries carry their
  known auth requirement through installation, and Verify never downgrades a
  known-auth app to public.
- A spec whose `servers[0].url` is a placeholder (Jira's
  `https://your-domain.atlassian.net`) can be pointed at the real tenant from
  the app's details; the override is applied when operations are loaded.
- The meta tools are scoped to the agent: search only sees apps equipped for
  that agent (or globally enabled), and execute refuses an operation belonging
  to an unequipped app. An app that declares auth but has no credential is
  hidden from both direct skills and `plugin_search` until the key is added, so
  the agent is never handed a tool that can only return 401.
- **Verify installed** re-parses every stored spec and retires placeholder rows;
  failures are reported instead of silently leaving dead cards.
- The **OpenAPI directory** is the real APIs.guru index (2,500+ providers),
  compacted into a ~1.7 MB local cache. Search, category filters, and
  pagination run against the cache; a refresh re-downloads the directory and
  falls back to the cached copy with a visible warning if the network is down.
- Curated picks (Stripe, Slack, Jira, GitHub, Petstore) stay first because they
  point at canonical, maintained specs. The directory's duplicate entries for
  those providers are hidden so each app appears once.
- Installing any entry fetches and parses the live spec; nothing is
  pre-equipped. The card reports the spec version, provider, category, and
  that auth is checked on install, rather than inventing an operation count.

## Avatars and status

`RavenAvatar.svelte` derives a stable shape and colour from the agent's name.
The resting expression is identity; the mood is what the engine plays over it.
Attention states add a ring, not a second face. Reduced motion keeps the ring
and removes the breathing.

## Theme compatibility

All component CSS reads the semantic tokens, so the theme catalogue (Neutral
Dark, Paper Light, Grok Sovereign, Onyx, Dracula, Rosé Pine, Tokyo Night,
Catppuccin, One Dark, Solarized, and the rest) re-tints layout without forking
components. Inline Tailwind colour classes are allowed only for one-off semantic
emphasis and should be converted to a token when the same role appears twice.

**Paper Light** is a first-class member, not a dark theme with an inverted
background. `applyTheme()` measures the canvas luminance and sets
`color-scheme`, so native scrollbars, form controls and selection follow the
theme. Role colours that cannot be derived by mixing — status ladders, washes,
scrims, the rail pill, data-table ink — have a light override table. The
Tailwind `dark:` variant is bound to `data-color-scheme`, not the operating
system, so shadcn primitives cannot fall back to dark variants in a light
theme. Theme preview cards paint their own text colour, because a Paper Light
card in a dark settings page would otherwise be white on white.

## Verification

- `type-scale.test.ts` — the 11px floor and the allowed size scale.
- `no-component-styles.test.ts` — all styling stays in global CSS.
- `no-inline-comments.test.ts` — template comments cannot add stray text.
- `ApprovalCard.dom.test.ts` — risk tone, summary, allow/deny, busy state.
- `QuestionCard.dom.test.ts` — chips, typed answer, closed choice, empty submit.
- `interactionSummary.test.ts` — command/path/query selection and clipping.
- `theme.test.ts` — catalogue contrast, light canvas, and runtime
  `color-scheme`/role tokens.
- `ChatMessageRow.dom.test.ts` — persisted reaction chips, palette choice, and
  search-hit state.
- `search.test.ts` — text extraction, case-insensitive matching, stable keys,
  and wraparound navigation.
- `replies.test.ts` — quote resolution, whitespace collapsing, deleted and
  attachment-only originals, and clipping.
- `navigation.test.ts` — adjacent-message indexing, start/end behavior, and the
  empty transcript.
- `messageWindow.test.ts` — tail-following, pinned history windows, clamping,
  anchored scroll offsets, and empty transcripts.
- `ChatMessageRow.dom.test.ts` — long-report collapse/expand, reactions, quote
  cards, and search-hit state.
- `plugins` crate — OpenAPI parsing, `$ref` resolution, provider-safe ids,
  credential headers, tenant server overrides, real meta-tool search, and
  per-agent scope.
- `WindowedTranscript.dom.test.ts` — a 240-message synthetic office mounts
  exactly 30 rows, stays under 9,000 DOM nodes, and completes within a
  catastrophic-regression ceiling. The measured baseline is printed as
  `OFFICE_WINDOWED_RENDER_MS`, `ROWS`, and `NODES`.
- `MarkdownCode.dom.test.ts` — one line box per source line, multi-line token
  continuity, and copy without gutter numbers.
- `attachments.test.ts` — decoded byte counts, size formatting, and Blob
  construction.
- `FileAttachmentCard.dom.test.ts` — document name, kind, size, and download
  label.
- `ChatMessageRow.dom.test.ts` — long-report collapse/expand, reactions, quote
  cards, and search-hit state.
- `Visuals.dom.test.ts` — chart, slide, math, visual, image surfaces.
- `ToolTraceList.dom.test.ts` — document and slide-deck result cards.
