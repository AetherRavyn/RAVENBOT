import { describe, expect, it } from "vitest";
import { approvalSummary } from "./interactionSummary";

describe("approvalSummary", () => {
  it("favours the command, path, or query over a JSON dump", () => {
    expect(approvalSummary({ command: "cargo test" })).toBe("cargo test");
    expect(approvalSummary({ path: "src/app.rs" })).toBe("src/app.rs");
    expect(approvalSummary({ query: "release notes" })).toBe("release notes");
    expect(approvalSummary({ path: "a", content: "12345" })).toBe("a (+5 chars)");
  });

  it("falls back to JSON and clips long values", () => {
    expect(approvalSummary({ nested: { a: 1 } })).toContain('"a":1');
    expect(approvalSummary("x".repeat(400))).toHaveLength(280);
    expect(approvalSummary("x".repeat(400)).endsWith("…")).toBe(true);
  });

  it("returns nothing for no arguments", () => {
    expect(approvalSummary(null)).toBe("");
  });
});
