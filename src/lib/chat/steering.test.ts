import { describe, expect, it } from "vitest";
import { canSteerRun } from "./steering";

const base = {
  running: true,
  streamingTool: "shell_exec",
  activeRunId: "run-1",
  text: "also check the tests",
  attachmentCount: 0,
};

describe("canSteerRun", () => {
  it("allows text while a tool is running", () => {
    expect(canSteerRun(base)).toBe(true);
  });

  it("rejects an idle run or a run between tools", () => {
    expect(canSteerRun({ ...base, running: false })).toBe(false);
    expect(canSteerRun({ ...base, streamingTool: null })).toBe(false);
    expect(canSteerRun({ ...base, activeRunId: null })).toBe(false);
  });

  it("rejects empty text and attachments", () => {
    expect(canSteerRun({ ...base, text: "   " })).toBe(false);
    expect(canSteerRun({ ...base, attachmentCount: 1 })).toBe(false);
  });
});
