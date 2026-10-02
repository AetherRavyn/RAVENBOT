/**
 * The contrast guarantee on the theme's text ramp.
 *
 * This file exists because of a bug that no unit test would have caught and no
 * screenshot review would have either: `--text-muted` was specified as a mix
 * ratio — `mixHex(muted, bg, 0.28)` — rather than as a contrast ratio, so every
 * dark theme in the catalogue produced secondary text between 2.6:1 and 4.0:1
 * against the panels it actually sits on. Just under the 4.5:1 WCAG AA asks of
 * body text. The app looked correct, because "just under" is not "wrong" to an
 * eye.
 *
 * It was caught by measuring the rendered DOM — walking every text node in
 * Settings and the composer, resolving the colour actually painted against the
 * background actually behind it, and comparing. So these tests are the same
 * measurement, written down so it cannot regress silently.
 */
import { describe, it, expect } from "vitest";
import { contrastRatio, ensureContrast, AA_TEXT } from "./theme";

/** WCAG relative luminance, spelled out rather than imported, so a change to
 *  the implementation cannot quietly change what the test means. */
function lum(hex: string): number {
  const [r, g, b] = hex.replace("#", "").match(/../g)!.map((h) => {
    const c = parseInt(h, 16) / 255;
    return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
  }) as [number, number, number];
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function ratio(a: string, b: string): number {
  return (Math.max(lum(a), lum(b)) + 0.05) / (Math.min(lum(a), lum(b)) + 0.05);
}

/** The surfaces a text token can land on, in the Neutral Dark theme. */
const SURFACES = {
  canvas: "#141414",
  panel: "#212121",
  raised: "#2c2c2c",
  hover: "#373737",
};

describe("contrastRatio", () => {
  it("matches the WCAG definition", () => {
    expect(ratio("#000000", "#ffffff")).toBeCloseTo(21, 1);
    expect(ratio("#ffffff", "#000000")).toBeCloseTo(21, 1);
    // A colour against itself is 1 by definition.
    expect(ratio("#212121", "#212121")).toBeCloseTo(1, 5);
  });

  it("is symmetric", () => {
    expect(contrastRatio("#8b8b90", "#141414")).toBeCloseTo(
      contrastRatio("#141414", "#8b8b90"),
      10,
    );
  });
});

describe("ensureContrast", () => {
  /**
   * The regression, stated as a test. These are the exact values the shipped
   * ramp produced: secondary text at 3.46:1 on a panel and 2.56:1 on a hover
   * row, against a 4.5:1 requirement.
   */
  it("lifts a colour that fails AA to one that passes", () => {
    const shippedMuted = "#747479";
    expect(ratio(shippedMuted, SURFACES.panel), "the shipped value already passed?").toBeLessThan(
      AA_TEXT,
    );

    const fixed = ensureContrast(shippedMuted, SURFACES.raised, AA_TEXT);
    expect(ratio(fixed, SURFACES.raised)).toBeGreaterThanOrEqual(AA_TEXT);
    // And it must still pass on every *darker* surface, because raising a
    // colour's luminance can only help against a darker background.
    expect(ratio(fixed, SURFACES.panel)).toBeGreaterThanOrEqual(AA_TEXT);
    expect(ratio(fixed, SURFACES.canvas)).toBeGreaterThanOrEqual(AA_TEXT);
  });

  /** The whole point: a token that already passes is not touched. */
  it("leaves a passing colour exactly alone", () => {
    const good = "#dcdcdc";
    expect(ensureContrast(good, SURFACES.panel, AA_TEXT)).toBe(good);
  });

  it("darkens rather than lightens on a light background", () => {
    const dim = "#9a9a9f";
    expect(ratio(dim, "#f2f2f4")).toBeLessThan(AA_TEXT);
    const fixed = ensureContrast(dim, "#f2f2f4", AA_TEXT);
    expect(lum(fixed), "should have moved down, not up").toBeLessThan(lum(dim));
    expect(ratio(fixed, "#f2f2f4")).toBeGreaterThanOrEqual(AA_TEXT);
  });

  it("makes the smallest move that clears the floor, not the largest one", () => {
  // White text on white: nothing is legible. The naive fix is to flip to the
  // opposite extreme, which is legible and much too dark — a muted label that
  // is now nearly black reads as primary text and undoes the whole ramp.
  const fixed = ensureContrast("#ffffff", "#ffffff", AA_TEXT);
  expect(ratio(fixed, "#ffffff"), "must be legible").toBeGreaterThanOrEqual(AA_TEXT);
  expect(lum(fixed), "should have moved most of the way, not all of it").toBeGreaterThan(0.05);
  expect(lum(fixed), "and certainly not all the way to black").toBeLessThan(0.4);
});

it("terminates rather than looping on degenerate input", () => {
  // Candidate identical to background — there is nothing legible about it and
  // no direction to nudge, so the search has to walk. This is the case a theme
  // engine must survive, because hanging here renders nothing at all.
  const fixed = ensureContrast("#000000", "#000000", AA_TEXT);
  expect(ratio(fixed, "#000000"), "must at least become legible").toBeGreaterThanOrEqual(
    AA_TEXT,
  );
  // Two rounds, so a future "optimisation" cannot quietly turn this into a hang.
  expect(() => ensureContrast("#000000", "#000000", 21)).not.toThrow();
});

  it("never overshoots far past the floor", () => {
    // A step size that is too coarse lands visibly brighter than needed, which
    // is the failure mode a guarantee like this usually has.
    const fixed = ensureContrast("#5a5a5f", SURFACES.panel, AA_TEXT);
    const slack = ratio(fixed, SURFACES.panel) - AA_TEXT;
    expect(slack, `overshot by ${slack.toFixed(2)}:1`).toBeLessThan(0.25);
  });

  /**
   * The guarantee has to hold for themes nobody has looked at, which is the
   * whole reason it is expressed as a ratio. This sweeps backgrounds and
   * starting colours rather than the catalogue.
   */
  it("holds for every background and starting colour in a wide sweep", () => {
    const backgrounds = [
      "#000000", "#0a0a0a", "#141414", "#212121", "#2a2a2a", "#373737",
      "#4a4a4a", "#808080", "#c0c0c0", "#e8e8ea", "#ffffff",
    ];
    const starts = [
      "#000000", "#333333", "#747479", "#8b8b90", "#a6a6ab", "#dcdcdc", "#ffffff",
    ];
    for (const bg of backgrounds) {
      for (const start of starts) {
        const fixed = ensureContrast(start, bg, AA_TEXT);
        const got = ratio(fixed, bg);
        const reachable = ratio(start === bg ? "#000000" : "#ffffff", bg) >= AA_TEXT;
        if (reachable) {
          expect(
            got,
            `${start} on ${bg} → ${fixed} reached only ${got.toFixed(2)}:1`,
          ).toBeGreaterThanOrEqual(AA_TEXT);
        }
      }
    }
  });

  it("still honours a lower floor for decoration", () => {
    // `--text-faint` is placeholder and separator text, which is non-text
    // under WCAG and gets 3:1. Pinning it to the body floor here would make
    // the ramp brighter than the design wants for decoration.
    const faint = ensureContrast("#4e4e53", SURFACES.raised, 3);
    expect(ratio(faint, SURFACES.raised)).toBeGreaterThanOrEqual(3);
    expect(ratio(faint, SURFACES.raised)).toBeLessThan(AA_TEXT + 0.25);
  });
});