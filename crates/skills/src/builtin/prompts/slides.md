# Slide design

A deck is an argument, not a pile of slides. Design the argument first; the
layout exists to make each step land.

## 1. Pin the brief

Before writing a spec, state in one sentence: **who is in the room, what
decision they are making, and what they need to believe at the end.** If the
user did not supply these, infer them from the request and say so. A deck with
no decision is a document with extra steps.

Choose a length that fits the brief: 5–7 slides for a decision, 8–12 for a
review, more only when the material genuinely needs it.

## 2. Outline the argument

Use one idea per slide, in this shape:

1. **Title** — the claim, not the topic. “Onboarding loses 40% of new users”,
   not “Onboarding”.
2. **Evidence** — a chart, a stat, a quote, or a short list that proves it.
3. **Consequence** — what follows if the room accepts or rejects the claim.
4. **Ask** — the decision, owner, and date.

An executive summary belongs on slide two only if the room will read it. Do not
repeat the same title-and-bullets slide with different words.

## 3. Choose layouts

`make_slides` accepts these code-defined layouts. Pick the one that carries the
idea; do not put prose in a stat slide.

| Layout | Use for |
|---|---|
| `title` | Opening: kicker, title, subtitle |
| `section` | A turn in the argument, with a number |
| `bullets` | 3–6 short points, no sub-bullets |
| `cards` | 2–6 parallel options, roles, or workstreams |
| `stats` | 1–4 numbers with labels |
| `timeline` | 1–5 dated milestones |
| `quote` | One quotation and an attribution |
| `chart` | A bar, line, area, pie, donut, or scatter chart |
| `closing` | The ask, the next step, or a thank-you |

## 4. Design rules

- **One dominant element per slide.** A title plus one chart, or one stat row,
  or one quote. Not all three.
- **Palette dominance.** Pick one background and one accent. Use the accent for
  the one thing the eye should land on; use muted text for everything else.
- **No accent lines.** Decorative bars, stripes, and side rules are the visual
  equivalent of filler. The layouts do not draw them.
- **Type is a hierarchy, not a decoration.** Titles are large and tight; body
  text stays at reading size. Never shrink body text to fit a paragraph — cut
  the paragraph.
- **Charts carry numbers.** Label the axes or the bars, state the unit, and put
  the takeaway in the title. A chart with no scale is a shape.
- **No placeholder text.** Every slide must say something true about this
  brief. Remove “Lorem ipsum”, “Insert metric here”, and “TBD”.
- **Text-only slides are a last resort.** If a slide has no visual, it should at
  least have one number or one memorable sentence.
- **Respect the canvas.** The renderer wraps and shrinks text, but it cannot
  invent space. Keep titles under ~12 words and bullets under ~20.

## 5. The spec

Call `make_slides` with a title, an optional theme, and an ordered slide array.

```json
{
  "title": "Q3 review",
  "theme": {
    "background": "#111318",
    "surface": "#1b1f27",
    "text": "#f8fafc",
    "muted": "#a8b0bf",
    "accent": "#79b8ff",
    "accent2": "#9ae6b4",
    "font": "Inter, system-ui, sans-serif"
  },
  "slides": [
    { "layout": "title", "kicker": "Q3 2026", "title": "Growth is compounding", "subtitle": "Revenue, retention, and the next bet" },
    { "layout": "stats", "title": "The quarter in three numbers", "stats": [
      { "value": "42%", "label": "Revenue growth" },
      { "value": "91%", "label": "Gross retention" },
      { "value": "18 days", "label": "Payback" }
    ]},
    { "layout": "chart", "title": "Activation is the constraint", "chart": {
      "type": "line", "labels": ["Jul", "Aug", "Sep"], "values": [61, 64, 68]
    }},
    { "layout": "quote", "quote": "The fastest onboarding in the category.", "attribution": "Customer advisory board" },
    { "layout": "closing", "title": "Approve the onboarding rewrite", "subtitle": "Owner: Product · Decision by Friday" }
  ]
}
```

The tool writes `slide-01.svg`, `slide-01.png`, … and a `deck.pptx` into
`deliverables/<title>/`. The PPTX is image-only: each PNG is full-bleed on a
blank 16:9 slide, so PowerPoint cannot reflow the design.

## 6. Preview and export

- Use a fenced `slides` block for a quick review in chat. It is Markdown, not a
  rendered deck.
- Use `make_slides` for the designed deck. Review the thumbnail strip in the
  tool trace and open the PPTX.
- Use `make_document` when the user needs editable text, a DOCX/PDF/HTML
  deliverable, or a themed-but-plain PPTX. Its PPTX uses Pandoc's theme, not the
  code-defined layouts. Say which path you used and why.
- A designed deck is not editable text. If the user asks to change a sentence
  after export, revise the spec and re-run `make_slides`; do not promise that
  PowerPoint text editing will preserve the layout.

## 7. Verify before you claim

Check the tool result: `count` matches the spec, every PNG is 1920×1080, and
`verified` is true. If `rsvg-convert` is missing, report the install hint and
leave the SVG sources; do not claim a deck was produced.

## 8. Avoid

- Accent stripes, corner glows, gradients, and glass.
- Three-column icon grids with no argument.
- A chart whose only label is “Data”.
- Repeating the deck title on every slide.
- “Thank you” as the only closing when a decision was requested.
- Speaker notes pretending to be slide content.
