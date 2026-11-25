import { describe, expect, it } from "vitest";
import {
  BROWSER_COLUMN_MIN_DOCK,
  BROWSER_COLUMN_MAX_DOCK,
  browserDockWidth,
  shouldOverlayBrowser,
} from "./layout";

describe("browser dock layout", () => {
  it("mirrors the CSS clamp width", () => {
    expect(browserDockWidth(800)).toBe(BROWSER_COLUMN_MIN_DOCK);
    expect(browserDockWidth(1920)).toBe(BROWSER_COLUMN_MAX_DOCK);
    expect(browserDockWidth(1280)).toBeCloseTo(588.8, 1);
  });

  it("keeps the side-by-side dock only when the conversation has room", () => {
    expect(shouldOverlayBrowser(1920, 280)).toBe(false);
    expect(shouldOverlayBrowser(1440, 280)).toBe(false);
    expect(shouldOverlayBrowser(1280, 280)).toBe(true);
    expect(shouldOverlayBrowser(1119, 0)).toBe(true);
    expect(shouldOverlayBrowser(900, 88)).toBe(true);
  });

  it("counts the sidebar when deciding, not just the window", () => {
    // Same window: with the sidebar the conversation is too narrow; without it
    // the dock can stay in its own column.
    expect(shouldOverlayBrowser(1280, 280)).toBe(true);
    expect(shouldOverlayBrowser(1280, 0)).toBe(false);
  });
});
