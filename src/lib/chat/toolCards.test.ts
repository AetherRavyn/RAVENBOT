import { describe, expect, it } from "vitest";
import {
  commandCard,
  diffCard,
  readCard,
  searchCard,
  toolCardKind,
  toolOutput,
} from "./toolCards";

const trace = (name: string, args: any, result: any, is_error = false) => ({
  name,
  arguments: args,
  result,
  is_error,
});

describe("tool card detection", () => {
  it("maps the first-class tools and leaves everything else generic", () => {
    expect(toolCardKind("shell_exec")).toBe("command");
    expect(toolCardKind("file_edit")).toBe("diff");
    expect(toolCardKind("apply_patch")).toBe("diff");
    expect(toolCardKind("file_write")).toBe("diff");
    expect(toolCardKind("file_read")).toBe("read");
    expect(toolCardKind("code_search")).toBe("search");
    expect(toolCardKind("memory_search")).toBeNull();
  });

  it("unwraps the success/output envelope and bare output alike", () => {
    expect(toolOutput(trace("x", {}, { success: true, output: { a: 1 } }))).toEqual({
      a: 1,
    });
    expect(toolOutput(trace("x", {}, { a: 1 }))).toEqual({ a: 1 });
  });
});

describe("commandCard", () => {
  it("reads the command, output, stderr and exit code", () => {
    const card = commandCard(
      trace(
        "shell_exec",
        { command: "npm test", cwd: "/tmp/ws" },
        {
          success: true,
          output: {
            command: "npm test",
            stdout: "ok 3 tests\n",
            stderr: "",
            exit_code: 0,
          },
        },
      ),
    );
    expect(card).toMatchObject({
      command: "npm test",
      cwd: "/tmp/ws",
      stdout: "ok 3 tests\n",
      exitCode: 0,
      success: true,
    });
  });

  it("falls back to a clipped string result and the call error", () => {
    const card = commandCard(
      trace("shell_exec", { command: "false" }, "exit 1: bad", true),
    );
    expect(card?.command).toBe("false");
    expect(card?.stdout).toContain("bad");
    expect(card?.success).toBe(false);
  });
});

describe("readCard", () => {
  it("keeps the line window and next cursor", () => {
    const card = readCard(
      trace(
        "file_read",
        { path: "src/main.rs" },
        {
          success: true,
          output: {
            path: "/ws/src/main.rs",
            content: "one\ntwo",
            offset: 2,
            limit: 10,
            returned_lines: 2,
            total_lines: 12,
            next_offset: 4,
          },
        },
      ),
    );
    expect(card).toMatchObject({
      path: "/ws/src/main.rs",
      offset: 2,
      returnedLines: 2,
      totalLines: 12,
      nextOffset: 4,
    });
    expect(card?.content).toBe("one\ntwo");
  });
});

describe("diffCard", () => {
  const diff = "--- a/a.ts\n+++ b/a.ts\n@@ -1 +1 @@\n-old\n+new";

  it("summarises the file changes and keeps each diff", () => {
    const card = diffCard(
      trace(
        "file_edit",
        { patch: diff },
        {
          success: true,
          output: {
            applied: true,
            file_changes: [
              {
                path: "a.ts",
                absolute_path: "/ws/a.ts",
                lines_added: 1,
                lines_deleted: 1,
                diff,
              },
            ],
          },
        },
      ),
    );
    expect(card?.added).toBe(1);
    expect(card?.deleted).toBe(1);
    expect(card?.files[0]).toMatchObject({ path: "a.ts", diff });
  });

  it("recognises a bare unified diff in a clipped result", () => {
    const card = diffCard(trace("code_edit", { path: "a.ts" }, diff));
    expect(card?.files).toHaveLength(1);
    expect(card?.files[0].diff).toContain("@@ -1 +1 @@");
  });

  it("does not invent a card for an edit failure with no diff", () => {
    expect(
      diffCard(trace("file_edit", {}, { success: false, error: "no match" }, true)),
    ).toBeNull();
  });
});

describe("searchCard", () => {
  it("shows matches and file-tree entries", () => {
    const search = searchCard(
      trace(
        "code_search",
        { pattern: "TODO", path: "." },
        {
          success: true,
          output: {
            pattern: "TODO",
            path: ".",
            matches: ["src/a.ts:1:TODO", "src/b.ts:2:TODO"],
            count: 2,
            truncated: false,
          },
        },
      ),
    );
    expect(search?.matches).toHaveLength(2);
    expect(search?.count).toBe(2);

    const tree = searchCard(
      trace(
        "file_tree",
        { path: "." },
        {
          success: true,
          output: {
            entries: [
              { type: "file", path: "a.ts", bytes: 10 },
              { type: "dir", path: "src", bytes: 0 },
            ],
            count: 2,
          },
        },
      ),
    );
    expect(tree?.entries).toBe(true);
    expect(tree?.matches).toEqual(["a.ts", "src"]);
  });
});
