# Rendering and design stack

This is the contract between what an agent can produce and what RAVENBOT can
show or export. It exists so a model does not have to guess which fence to use,
and so a "chart" never becomes a pile of raw JSON in the answer.

## Capability matrix

| Need | In-chat surface | Export | Engine |
|---|---|---|---|
| Data chart | `chart` fence | SVG (download button) | Native SVG, no runtime |
| Math | `$...$`, `$$...$$`, `\(...\)`, `\[...\]` | Carried into DOCX/PDF/PPTX | KaTeX 0.19 |
| Quick deck review | `slides` fence; arrows, Home/End, fullscreen present, progress | — | Native Markdown renderer |
| Designed slide deck | `slides` fence for review; rendered PNG thumbnails in the tool trace | 1920×1080 PNGs + image-only `.pptx` | Rust vector layouts → `rsvg-convert` → OOXML writer |
| Editable text deck | Markdown deck | `.pptx` via `make_document` | Pandoc |
| Diagram | `mermaid` fence | Screenshot / source | Mermaid |
| Interactive page | `visual` fence | Not exported as a file | Sandboxed `ravenvisual://` webview |
| Static HTML | `html` fence | HTML via `make_document` if needed | Sanitized static preview |
| Report / brief / contract | Normal Markdown + `make_document` | `.docx`, `.pdf`, `.html` | Pandoc + XeLaTeX |
| Short video | Not embedded as a player by default | `.mp4` / `.webm` | fframes (Rust/SVG/ffmpeg) or ffmpeg fallback |
| Images / audio / video | Attachment cards and lightbox | Workspace files | Native app |

## The image-first slide path

`make_slides` is the designed-deck path. A deck spec becomes one SVG per slide
at 1920×1080, then one PNG per slide, then a PPTX package:

1. **Layout in Rust.** `slide_render.rs` places text, cards, icon badges,
   shapes, timelines, stats, quotes, and charts on a fixed grid. Text is wrapped
   with a conservative advance estimate and shrunk until it fits, so a long
   title cannot run off the canvas.
2. **Native vector artwork.** Icons are inline SVG paths; charts are generated
   as SVG bars, lines, areas, pie/donut arcs, or scatter points. No browser,
   charting runtime, or network font is involved.
3. **Rasterize.** `rsvg-convert` writes exactly 1920×1080 PNGs. The writer
   verifies the PNG signature and the IHDR dimensions, not just the exit code.
4. **Package.** `pptx.rs` writes a minimal OOXML package with a blank 16:9
   slide layout and one picture per slide at `0,0`, 13.333×7.5 in
   (12,192,000×6,858,000 EMU). The picture is full-bleed, so PowerPoint has no
   text frame, placeholder, or theme to reflow.
5. **Show the result.** The tool trace renders a horizontal thumbnail strip and
   an **Open PPTX** button. Clicking a thumbnail opens the shared image
   lightbox, so a deck can be reviewed slide by slide without leaving chat.

The SVG source files remain beside the PNGs for design edits. The PPTX is
image-only on purpose. If the user needs editable slide text, use
`make_document` instead and say that the design will be simpler.

## Rendering invariants

1. **No third-party script in the app origin.** Charts are SVG, math is KaTeX
   HTML, Mermaid is rendered in its own component, and `visual` pages run only
   on a separate origin in a sandboxed frame.
2. **No horizontal page scroll.** Charts use a responsive `viewBox`; slides use
   a 16:9 stage that scrolls internally; Mermaid scrolls inside its own box.
3. **The answer follows the reader.** Streaming scroll follows the revealed
   words, not the raw token buffer, so the last line is never left below the
   fold.
4. **A file is not a claim.** `make_document` verifies Pandoc's exit status, the
   path, and the file length. `make_slides` verifies every PNG's dimensions and
   that the PPTX package is non-empty. A failed PDF reports the missing TeX
   engine instead of pretending it worked.
5. **Design is content-specific.** The slides workflow forbids accent stripes,
   repeated title-and-bullets, placeholder text, and text-only slides. These
   are the defects that make generated decks look generated.
6. **A designed deck is not editable text.** The image PPTX preserves layout;
   it does not preserve text editing. The workflow states this rather than
   implying both at once.

## Sources and what was used

- **fframes** (MIT), `dmtrKovalenko/fframes`: current README and agent skill
  on `main`. Adopted the `cargo fframes new --template ... --backend ...`
  flow, `inspect`/`strip`/`frame`/`onion`/`audio analyze` review loop, and
  `npx skills add https://fframes.studio`. Added a local ffmpeg fallback for
  machines without `cargo-fframes`.
- **Anthropic skills** (`anthropics/skills`): the `pptx` skill's design rules
  (palette dominance, typography, one visual per slide, no accent bars, no
  text-only slides, QA/overflow checks) informed `prompts/slides.md`. Its
  create/edit split also confirmed the decision here: use a code-generated deck
  rather than hand-editing a template's XML. No proprietary source files or
  scripts were copied; the document skills are source-available, so only the
  public design guidance and workflow shape were adapted.
- **Pandoc 3.11**: DOCX/PDF/PPTX/HTML conversion, with XeLaTeX for PDF and
  `--embed-resources` for self-contained HTML.
- **KaTeX** and **Mermaid**: existing renderers, extended for `\(...\)` and
  `\[...\]` math delimiters and a fullscreen slide mode.
- **librsvg / `rsvg-convert`**: the rasterizer for the vector slide layouts.

## Built-in skills added

- `slides` — deck argument, layout, palette, icons, chart/math rules, preview,
  and the image-first `make_slides` export.
- `make_slides` — structured deck spec → PNG slides + full-bleed image PPTX.
- `make_document` — real DOCX/PDF/PPTX/HTML files inside the workspace.
- `visuals` — updated to point at the designed-slide path.
- `fframes` — updated to the current CLI and given a tested ffmpeg fallback.

## Verification

- `MathBlock.dom.test.ts`, `mathDelimiters.test.ts` — KaTeX + delimiters.
- `Visuals.dom.test.ts` — chart SVG, donut, malformed chart, chart download,
  slide navigation, keyboard navigation, present button, progress.
- `slide_design` tests — every layout renders, titles are XML-escaped, a bad
  theme color cannot inject SVG, Markdown fallback works, PNGs are exactly
  1920×1080, and the PPTX contains the media, slide XML, relationship, and
  full-bleed transform.
- `pptx` test — a minimal package opens as a ZIP and the slide XML carries
  `r:embed="rId2"` and the 12,192,000×6,858,000 EMU transform. Pandoc also
  reads the exported deck as a PPTX, which proves the package and relationships
  are coherent to an independent OOXML consumer.
- `ToolTraceList` test — the slide deck card shows thumbnails and an Open PPTX
  button for a real `make_slides` result.
- `documents` tests — DOCX/PPTX/PDF are real files (`PK`, `%PDF`), filename
  traversal is sanitized, slide separators become PPTX slide breaks.
- `visual_prompt_tests` — visuals/slides/fframes prompts keep the required
  sections and current fframes flags.

## Known limits

- Pandoc's default PPTX theme is plain. A user-supplied `reference_doc` gives a
  designed template for the editable-text path; the designed path does not use
  a PowerPoint theme at all.
- `visual` pages are interactive previews, not printable artifacts; use a
  `chart`/`slides`/`make_slides`/`make_document` path when the result must be a
  file.
- The ffmpeg fallback is a designed slideshow, not real motion graphics; the
  workflow says so rather than calling it fframes output.
- Video review still needs a human for taste; the agent verifies structure,
  frames, duration, codec, and audio levels.
- The designed-slide path needs `rsvg-convert` from librsvg. If it is missing,
  `make_slides` fails with an install hint instead of writing a broken deck.
  The editable-text path still works through Pandoc.
