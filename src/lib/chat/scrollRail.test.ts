import { describe, expect, it } from "vitest";
import { fraction, viewportBand } from "./scrollRail";

describe("scroll rail geometry", () => {
  it("clamps offsets and handles empty transcripts", () => {
    expect(fraction(0, 1000)).toBe(0);
    expect(fraction(500, 1000)).toBe(0.5);
    expect(fraction(1500, 1000)).toBe(1);
    expect(fraction(-10, 1000)).toBe(0);
    expect(fraction(10, 0)).toBe(0);
    expect(fraction(Number.NaN, 1000)).toBe(0);
  });

  it("computes the viewport band as fractions of the transcript", () => {
    expect(viewportBand(250, 500, 1000)).toEqual({ top: 0.25, height: 0.5 });
    expect(viewportBand(0, 500, 0)).toEqual({ top: 0, height: 0 });
  });
});
