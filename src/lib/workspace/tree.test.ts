import { describe, it, expect } from "vitest";
import { buildTree, flattenTree, fmtBytes, type Entry } from "./tree";

/**
 * The workspace tree, now shared by the settings browser and the projects IDE.
 *
 * Extracted precisely so there would be one answer to these questions instead
 * of two that drift. Every case here is a way the flat-list-to-tree conversion
 * has to be robust, because the backend sends a flat list on purpose — the
 * hierarchy is reconstructed here, and reconstruction is where the bugs live.
 */

const entry = (path: string, isDir = false): Entry => ({
  path,
  name: path.split("/").pop() || path,
  isDir,
  isLink: false,
  sizeBytes: 10,
  preview: null,
  isBinary: false,
});

describe("buildTree", () => {
  it("nests children under their folder", () => {
    const tree = buildTree([
      entry("src", true),
      entry("src/lib", true),
      entry("src/lib/app.ts"),
      entry("README.md"),
    ]);
    expect(tree.map((n) => n.entry.name)).toEqual(["src", "README.md"]);
    expect(tree[0].children.map((n) => n.entry.name)).toEqual(["lib"]);
    expect(tree[0].children[0].children[0].entry.name).toBe("app.ts");
  });

  /**
   * The backend sorts; the shape must not depend on how.
   *
   * A single pass would attach a child to a parent that had not been created
   * yet and strand it at the top level — which reads as "this file is in the
   * workspace root", the single most misleading thing a file tree can say.
   */
  it("builds the same shape whatever order the list arrives in", () => {
    const entries = [entry("src/lib/app.ts"), entry("src", true), entry("src/lib", true), entry("README.md")];
    const a = buildTree(entries);
    const b = buildTree([...entries].reverse());
    const shape = (nodes: typeof a): string[] =>
      nodes.flatMap((n) => [n.entry.path, ...shape(n.children)]);
    expect(shape(a)).toEqual(shape(b));
    expect(a[0].children[0].children[0].entry.path).toBe("src/lib/app.ts");
  });

  it("synthesises a folder the listing capped away", () => {
    // `src/lib` hit the depth/entry cap but its file came through. Without the
    // synthesised parent the file lands at the top level.
    const tree = buildTree([entry("src/lib/app.ts")]);
    expect(tree).toHaveLength(1);
    expect(tree[0].entry.path).toBe("src");
    expect(tree[0].entry.isDir).toBe(true);
    expect(tree[0].children[0].entry.path).toBe("src/lib");
  });

  it("puts folders before files, then sorts by name", () => {
    const tree = buildTree([
      entry("zebra.md"),
      entry("src", true),
      entry("apple.md"),
      entry("tests", true),
    ]);
    expect(tree.map((n) => n.entry.name)).toEqual(["src", "tests", "apple.md", "zebra.md"]);
  });

  it("sorts every level, not just the top", () => {
    const tree = buildTree([entry("src", true), entry("src/z.ts"), entry("src/a.ts")]);
    expect(tree[0].children.map((n) => n.entry.name)).toEqual(["a.ts", "z.ts"]);
  });

  it("records depth as path segments minus one", () => {
    const tree = buildTree([entry("src/lib/app.ts")]);
    expect(tree[0].depth).toBe(0);
    expect(tree[0].children[0].depth).toBe(1);
    expect(tree[0].children[0].children[0].depth).toBe(2);
  });

  it("returns nothing for an empty listing", () => {
    expect(buildTree([])).toEqual([]);
  });

  it("keeps two roots separate", () => {
    const tree = buildTree([entry("a/one.txt"), entry("b/two.txt")]);
    expect(tree).toHaveLength(2);
  });
});

describe("flattenTree", () => {
  const tree = buildTree([
    entry("src", true),
    entry("src/lib", true),
    entry("src/lib/app.ts"),
    entry("README.md"),
  ]);

  it("walks depth-first over what is open", () => {
    const all = flattenTree(tree, () => true);
    expect(all.map((n) => n.entry.path)).toEqual([
      "src",
      "src/lib",
      "src/lib/app.ts",
      "README.md",
    ]);
  });

  /** Collapsed folders must vanish from the sequence, or arrow-key navigation
   *  would step into files the user cannot see. */
  it("skips the contents of closed folders", () => {
    const visible = flattenTree(tree, (p) => p !== "src");
    expect(visible.map((n) => n.entry.path)).toEqual(["src", "README.md"]);
  });
});

describe("fmtBytes", () => {
  it("formats at human scales", () => {
    expect(fmtBytes(0)).toBe("0 B");
    expect(fmtBytes(1023)).toBe("1023 B");
    expect(fmtBytes(1024)).toBe("1.0 KB");
    expect(fmtBytes(1536)).toBe("1.5 KB");
    expect(fmtBytes(1024 ** 2)).toBe("1.0 MB");
    expect(fmtBytes(1024 ** 4)).toBe("1.0 TB");
  });

  /**
   * Five magnitudes is all there are; a sixth would need a new unit the code
   * does not have. What must not happen is inventing one — so the value keeps
   * growing but the unit does not.
   */
  it("stops at the largest unit", () => {
    expect(fmtBytes(1024 ** 5)).toBe("1024.0 TB");
    expect(fmtBytes(1024 ** 6)).toContain("TB");
    expect(fmtBytes(1024 ** 6)).not.toContain("PB");
  });
});
