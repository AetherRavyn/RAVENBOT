# UI audit

Measures the rendered app instead of guessing at it. I cannot look at a
screenshot, and neither can a reviewer catch everything: a contrast ratio of
3.96:1 *looks fine* and is a WCAG AA failure, and a 5px drag handle that covers
five buttons is invisible in a diff.

Everything here drives a real headless Chromium over the DevTools Protocol with
no dependencies beyond Node 24's global `WebSocket`. Start the dev server first
(`npm run dev`, port 1420), then:

```bash
node scripts/ui-audit/audit.mjs       # contrast, hit targets, names, overflow, clipping
node scripts/ui-audit/geometry.mjs    # per-element geometry snapshot, for before/after diffing
```

`geometry.mjs <out.json>` writes a snapshot. Take one before a change that moves
type or spacing and one after, then diff: the thing that matters is *new*
overflow or clipping, which is what silently breaks a layout sweep.

## What it checks, and what it deliberately does not

| Check | Why |
|---|---|
| Text contrast vs the background actually behind it | AA is 4.5:1 for body text. The theme engine derives its text ramp, so this can fail app-wide. |
| Pointer target ≥ 24×24 (WCAG 2.2 AA) | The failure is invisible until someone tries to click it. |
| Accessible name on every button/link | `title` is a last-resort fallback, not a name. |
| Horizontal overflow past the viewport | Layout escape. |
| Text clipped with no scroll and no ellipsis | Content nobody can read. |

It skips text over gradients and images, because there is no single background to
measure against and a report there is a false positive.

## One check that *was* a false positive

The first version flagged 73 elements as having no focus style. That was wrong:
`:focus-visible { outline: 2px solid … }` is a global rule in `components.css`,
and computed styles do not show a `:focus-visible` rule until the element is
actually focused — and programmatic `.focus()` does not trigger `:focus-visible`
either. Verify focus by dispatching real key events before believing a focus
finding.

## Known-good state

At the time of writing, Settings, MCPs & Connectors and the composer report
**clean** across every check, and the type scale is 11/12/13/14/16/18/20/24 —
enforced by `src/lib/styles/type-scale.test.ts`.
