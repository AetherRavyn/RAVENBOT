import { describe, expect, it } from "vitest";
import { highlightParts, matchesQuery, normalizedSearch } from "$lib/search/highlight";

describe("search highlight", () => {
  it("matches every part case-insensitively", () => {
    expect(matchesQuery(["Billing Ledger", "finance"], "LEDGER")).toBe(true);
    expect(matchesQuery(["Billing Ledger", "finance"], "invoice")).toBe(false);
    expect(matchesQuery(["anything"], "")).toBe(true);
  });

  it("returns the matched runs without losing the original casing", () => {
    expect(highlightParts("Invoice review", "invoice")).toEqual([
      { text: "Invoice", match: true },
      { text: " review", match: false },
    ]);
    expect(highlightParts("Invoice invoice", "invoice")).toEqual([
      { text: "Invoice", match: true },
      { text: " ", match: false },
      { text: "invoice", match: true },
    ]);
  });

  it("normalizes whitespace", () => {
    expect(normalizedSearch("  Billing   Ledger  ")).toBe("billing ledger");
  });
});
