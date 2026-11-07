import { describe, expect, it } from "vitest";
import { anchoredScrollTop, transcriptWindow } from "./messageWindow";

describe("transcriptWindow", () => {
  it("follows the tail when no end is pinned", () => {
    expect(transcriptWindow(216, 50)).toEqual({ start: 166, end: 216 });
  });

  it("pins the end for a jump into older history", () => {
    expect(transcriptWindow(216, 50, 100)).toEqual({ start: 50, end: 100 });
  });

  it("never starts below zero on a short transcript", () => {
    expect(transcriptWindow(12, 50)).toEqual({ start: 0, end: 12 });
    expect(transcriptWindow(216, 50, 20)).toEqual({ start: 0, end: 20 });
  });

  it("clamps a pinned end to the transcript", () => {
    expect(transcriptWindow(216, 50, 999)).toEqual({ start: 166, end: 216 });
    expect(transcriptWindow(216, 50, -10)).toEqual({ start: 0, end: 0 });
  });

  it("handles an empty transcript and a zero window", () => {
    expect(transcriptWindow(0, 50)).toEqual({ start: 0, end: 0 });
    expect(transcriptWindow(216, 0)).toEqual({ start: 216, end: 216 });
  });
});

describe("anchoredScrollTop", () => {
  it("adds the height gained above the viewport", () => {
    expect(anchoredScrollTop(0, 1000, 1600)).toBe(600);
    expect(anchoredScrollTop(240, 1000, 1600)).toBe(840);
  });

  it("does not move when no height was added", () => {
    expect(anchoredScrollTop(240, 1000, 1000)).toBe(240);
    expect(anchoredScrollTop(240, 1000, 900)).toBe(240);
  });

  it("never returns a negative offset", () => {
    expect(anchoredScrollTop(-40, 1000, 900)).toBe(0);
  });
});
