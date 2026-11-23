import { beforeEach, describe, expect, it } from "vitest";
import { setLocale } from "$lib/i18n";
import { messageDayKey, messageDayLabel } from "./dayMarkers";

beforeEach(() => setLocale("en"));

describe("message day markers", () => {
  it("keys messages by local calendar day", () => {
    expect(messageDayKey("2026-11-09T12:00:00")).toBe("2026-11-9");
    expect(messageDayKey("bad date")).toBe("");
    expect(messageDayKey(null)).toBe("");
  });

  it("labels today, yesterday, and older dates", () => {
    const now = new Date(2026, 10, 10, 12, 0, 0);
    expect(messageDayLabel("2026-11-10T08:00:00", now)).toBe("Today");
    expect(messageDayLabel("2026-11-09T20:00:00", now)).toBe("Yesterday");
    expect(messageDayLabel("2025-12-31T12:00:00", now)).toContain("2025");
  });

  it("ignores invalid timestamps", () => {
    expect(messageDayLabel("not a date")).toBe("");
  });
});
