import { describe, it, expect } from "vitest";
import { showAuthorHeader, authorHue, authorKey } from "./grouping";

describe("message grouping", () => {
  const msgs = [
    { role: "user" },
    { sender_bot_id: "a", sender_name: "RANO" },
    { sender_bot_id: "a", sender_name: "RANO" },
    { sender_bot_id: "b", sender_name: "CODEX" },
    { sender_bot_id: "a", sender_name: "RANO" },
  ];

  it("shows the header on the first row of every author run", () => {
    expect(msgs.map((_, i) => showAuthorHeader(msgs, i))).toEqual([
      true, true, false, true, true,
    ]);
  });

  it("falls back to role when sender fields are absent", () => {
    expect(authorKey({ role: "user" })).toBe("user");
    expect(showAuthorHeader([{ role: "a" }, { role: "a" }], 1)).toBe(false);
  });

  it("gives each author a stable hue", () => {
    expect(authorHue("a")).toBe(authorHue("a"));
    expect(authorHue("a")).not.toBe(authorHue("zzqxl"));
    const h = authorHue("anything");
    expect(h).toBeGreaterThanOrEqual(0);
    expect(h).toBeLessThan(360);
  });
});
