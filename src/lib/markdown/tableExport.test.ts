import { describe, expect, it } from "vitest";
import { tableToCsv, tableToMarkdown } from "./tableExport";

const table = {
  headers: ["Requirement", "Status", "Notes"],
  rows: [
    ["Rust", "Matched", "stable"],
    ["Kinesis", "Missing", 'contains "quotes", commas'],
  ],
};

describe("table exports", () => {
  it("writes a Markdown table that keeps every cell", () => {
    expect(tableToMarkdown(table)).toBe(
      "| Requirement | Status | Notes |\n| --- | --- | --- |\n| Rust | Matched | stable |\n| Kinesis | Missing | contains \"quotes\", commas |",
    );
  });

  it("writes RFC 4180 CSV", () => {
    expect(tableToCsv(table)).toBe(
      'Requirement,Status,Notes\nRust,Matched,stable\nKinesis,Missing,"contains ""quotes"", commas"',
    );
  });

  it("escapes pipes in Markdown cells", () => {
    expect(tableToMarkdown({ headers: ["a"], rows: [["x | y"]] })).toContain(
      "x \\| y",
    );
  });
});
