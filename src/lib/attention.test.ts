import { describe, expect, it } from "vitest";
import { relativeAttentionTime } from "./attention.svelte";

describe("relativeAttentionTime", () => {
  const now = Date.parse("2026-11-09T12:00:00.000Z");

  it("reads recent times in seconds and minutes", () => {
    expect(relativeAttentionTime("2026-11-09T11:59:45.000Z", "en", now)).toContain(
      "15 seconds",
    );
    expect(relativeAttentionTime("2026-11-09T11:55:00.000Z", "en", now)).toContain(
      "5 minutes",
    );
  });

  it("reads older times in hours and days", () => {
    expect(relativeAttentionTime("2026-11-09T09:00:00.000Z", "en", now)).toContain(
      "3 hours",
    );
    expect(relativeAttentionTime("2026-11-06T12:00:00.000Z", "en", now)).toContain(
      "3 days",
    );
  });

  it("returns nothing for an unparseable timestamp", () => {
    expect(relativeAttentionTime("not-a-date", "en", now)).toBe("");
  });
});
