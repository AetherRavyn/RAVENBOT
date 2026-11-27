# Visuals

Use a visual when a relationship, comparison, or sequence is easier to see than
to read. A wall of prose is not improved by decorating it; pick the smallest
visual that answers the question.

## Chart block

A fenced `chart` block is a native SVG chart. It renders during streaming,
inherits the app theme, and needs no network or third-party runtime.

````markdown
```chart
{
  "type": "bar",
  "title": "Weekly signups",
  "labels": ["Mon", "Tue", "Wed", "Thu", "Fri"],
  "series": [
    { "name": "Product", "values": [12, 18, 15, 24, 31] },
    { "name": "Partner", "values": [4, 6, 5, 9, 11] }
  ]
}
```
````

`type` is one of `bar`, `line`, `area`, `pie`, `donut`, or `scatter`. For a
single series you may pass `"values"` instead of `"series"`. Use real numbers
from the conversation or a source; never invent data to make a chart look full.
Every chart needs a `title`, and complex charts need a short caption below the
fence explaining the takeaway.

## Slide block

A fenced `slides` block is a review deck rendered in chat. Slides are separated
by a line containing only `---`. Use it to check the argument before exporting
a designed deck.

````markdown
```slides
# The next quarter

Our plan in one line.

---

## What changes

- Ship the onboarding rewrite
- Cut the setup path to under two minutes
- Measure activation weekly
```
````

Arrow keys and Home/End navigate. The present button opens a fullscreen 16:9
stage. Keep one idea per slide and no more than five bullets.

## Mermaid

Use `mermaid` for flowcharts, sequence diagrams, state machines, and timelines.
It renders wide diagrams inside their own scroll box. Keep labels short; a
diagram that needs a paragraph per node is a document, not a diagram.

## Interactive visual

A fenced `visual` block is a self-contained HTML page with its own scripts. It
runs on a separate `ravenvisual://` origin without same-origin access, can report
its height, and opens web links through the app. Use it for calculators,
dashboards, and click-through prototypes — not for anything that must be printed
or filed.

The page gets a small `window.ravenbot` bridge:

- `ravenbot.compose(text)` puts text in the composer for review. It appends to
  whatever the person already typed and never sends. Limit: 10,000 characters.
- `ravenbot.setContext(text)` attaches one replaceable "Visual context"
  blockquote for the next message. Calling it again replaces the previous one;
  `setContext("")` clears it. Limit: 2,000 characters.
- `ravenbot.requestFullscreen()` and `ravenbot.exitFullscreen()` change the
  frame size.

All three require a real click or keypress inside the page; a call while the
page loads is ignored. Guard for the mobile/preview case with
`if (window.ravenbot) ...`. Do not build a second send button: the person sends
from the real composer.

A fenced `html` block is a static, script-free preview. Use it when the markup
is the deliverable.

## Exporting

- Use `make_slides` for a designed deck: structured layouts become 1920×1080
  images and a full-bleed image `.pptx`.
- Use `make_document` for editable `.docx`, `.pdf`, `.pptx`, or `.html`. Its
  PPTX converts `---` slide separators, but the design is the document theme,
  not a code-defined layout.
- Say which path produced the file and name the artifact path. A chart in chat
  is a preview; it is not a file unless a tool wrote one.

## Rules

1. Never put raw JSON in the answer instead of a chart fence.
2. Do not use a chart for two numbers a sentence can compare.
3. A malformed chart should be fixed, not hidden; the renderer shows its error
   and source so the model can repair it.
4. Do not embed third-party scripts. Charts are SVG, math is KaTeX, Mermaid is a
   component, and interactive pages are sandboxed.
