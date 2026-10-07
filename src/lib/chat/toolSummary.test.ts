import { describe, it, expect } from "vitest";
import { summarizeArgs, renderToolValue } from "./toolSummary";

/**
 * Summaries for tool arguments.
 *
 * The live marker and the persisted trace share this, so a call reads the same
 * while it is running and after a reload. What every case here protects against
 * is the same failure: a line that says nothing useful, which is worse than no
 * line at all because it occupies the space where the useful line would go.
 */
describe("summarizeArgs", () => {
  it("prefers the identifying key over the payload", () => {
    // The failure mode: dumping JSON puts `content` first, so the header reads
    // as the beginning of a file instead of the file's name.
    expect(
      summarizeArgs({ path: "src/lib/app.rs", content: "fn main() {}".repeat(20) }),
    ).toBe("src/lib/app.rs");
  });

  it("handles each hint key the tools actually use", () => {
    expect(summarizeArgs({ file_path: "/etc/hosts" })).toBe("/etc/hosts");
    expect(summarizeArgs({ filePath: "a/b.ts" })).toBe("a/b.ts");
    expect(summarizeArgs({ url: "https://example.com" })).toBe("https://example.com");
    expect(summarizeArgs({ query: "  slow queries  " })).toBe("slow queries");
    expect(summarizeArgs({ command: "cargo test" })).toBe("cargo test");
  });

  it("falls back to any string value before giving up", () => {
    // `{ target_branch: "main" }` has no hint key but is perfectly
    // summarisable. Blank would be worse than showing it.
    expect(summarizeArgs({ target_branch: "main" })).toBe("main");
  });

  it("falls back to compact JSON for an object with nothing to hint at", () => {
    // No string to pick out, so the honest thing is the object itself — short
    // objects are perfectly readable and inventing `key=value` for arbitrary
    // keys would be a second format to learn.
    expect(summarizeArgs({ a: 1, b: true })).toBe('{"a":1,"b":true}');
  });

  it("handles scalars and bare strings", () => {
    expect(summarizeArgs("hello world")).toBe("hello world");
    expect(summarizeArgs(42)).toBe("42");
    expect(summarizeArgs(true)).toBe("true");
    expect(summarizeArgs(null)).toBe("");
    expect(summarizeArgs(undefined)).toBe("");
  });

  it("collapses whitespace and clips a long value", () => {
    expect(summarizeArgs("a\n\n  b\t c")).toBe("a b c");
    const long = summarizeArgs("x".repeat(500));
    expect(long.length).toBeLessThanOrEqual(91);
    expect(long.endsWith("…")).toBe(true);
  });

  /**
   * The whole point of the cap: a tool that reads a file hands back megabytes,
   * and a header that long would push the entire run off screen.
   */
  it("never lets a pasted document become a header", () => {
    const out = summarizeArgs({ content: "lorem ipsum ".repeat(5000) });
    expect(out.length).toBeLessThanOrEqual(91);
  });
});

describe("renderToolValue", () => {
  it("passes strings through untouched", () => {
    expect(renderToolValue("plain output")).toBe("plain output");
  });

  it("pretty-prints objects so a result is readable", () => {
    expect(renderToolValue({ ok: true })).toBe('{\n  "ok": true\n}');
  });

  it("returns nothing for absent values", () => {
    expect(renderToolValue(null)).toBe("");
    expect(renderToolValue(undefined)).toBe("");
    expect(renderToolValue("null")).toBe("");
  });

  it("survives a value that cannot be stringified", () => {
    const cyclic: Record<string, unknown> = {};
    cyclic.self = cyclic;
    expect(() => renderToolValue(cyclic)).not.toThrow();
    expect(typeof renderToolValue(cyclic)).toBe("string");
  });
});
