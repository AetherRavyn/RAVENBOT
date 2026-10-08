// @vitest-environment jsdom
/**
 * The diff renderer, in a DOM.
 *
 * A diff's whole job is to be checked. Two integers saying "31 added, 12
 * removed" cannot be; the lines can. So the claims worth pinning are the ones
 * where a renderer would silently disagree with the numbers above it —
 * misnumbering a column, swallowing a line, or reading the text of a removal
 * as a filename and filing it under a path nobody has.
 *
 * Parsing lives in the component rather than in a helper deliberately: this is
 * the surface the parsing is *for*, and a unit test on a function nobody else
 * calls would keep passing after the component stopped using it.
 */
import { describe, it, expect, beforeEach } from "vitest";
import { render, cleanup } from "@testing-library/svelte";
import DiffView from "./DiffView.svelte";

beforeEach(cleanup);

const diff = [
  "--- a/src/lib.rs",
  "+++ b/src/lib.rs",
  "@@ -1,3 +1,3 @@",
  " fn a() {}",
  "-fn b() {}",
  "+fn B() {}",
  " fn c() {}",
  "",
].join("\n");

const rows = (container: HTMLElement) => [...container.querySelectorAll<HTMLElement>(".dif__row")];
const text = (el: HTMLElement) => el.querySelector(".dif__text")?.textContent ?? "";
const numbers = (el: HTMLElement) =>
  [...el.querySelectorAll(".dif__no")].map((n) => n.textContent ?? "");

describe("DiffView", () => {
  it("numbers the old and the new side separately", () => {
    const { container } = render(DiffView, { props: { diff } });
    const r = rows(container);
    // Headers and hunk markers carry no numbers; the six content rows follow.
    const content = r.filter((el) => ["add", "del", "ctx"].includes(el.dataset.k ?? ""));

    expect(content).toHaveLength(4);
    expect(numbers(content[0])).toEqual(["1", "1"]); // context: on both sides
    expect(numbers(content[1])).toEqual(["2", ""]); // removed: old side only
    expect(numbers(content[2])).toEqual(["", "2"]); // added: new side only
    expect(numbers(content[3])).toEqual(["3", "3"]);
  });

  it("counts into a strip that agrees with the rows below it", () => {
    const { container } = render(DiffView, { props: { diff } });
    const strip = container.querySelector(".dif__count")?.textContent ?? "";
    expect(strip).toContain("+1");
    expect(strip).toContain("−1");

    const adds = rows(container).filter((el) => el.dataset.k === "add").length;
    const dels = rows(container).filter((el) => el.dataset.k === "del").length;
    expect(strip).toContain(`+${adds}`);
    expect(strip).toContain(`−${dels}`);
  });

  it("reads a removed line that starts with -- as content, not as a path", () => {
    // Deleting a markdown rule or a commented-out shell line prints as
    // `--- …`. If that were taken for a filename, the removal would vanish
    // from the diff while the count above still claimed it.
    const tricky = [
      "--- a/doc.md",
      "+++ b/doc.md",
      "@@ -1,2 +1,2 @@",
      " keep()",
      "--- rule",
      " still()",
      "",
    ].join("\n");

    const { container } = render(DiffView, { props: { diff: tricky } });
    const content = rows(container).filter((el) => ["add", "del", "ctx"].includes(el.dataset.k ?? ""));
    // Three content rows, all attributed to the same file, none to "rule".
    expect(content).toHaveLength(3);
    expect(content.map((el) => el.dataset.k)).toEqual(["ctx", "del", "ctx"]);
    expect(text(content[1])).toBe("-- rule");
    expect(container.textContent).not.toContain("+++ b/rule");
  });

  it("renders a trailing newline as nothing rather than as a blank row", () => {
    const { container } = render(DiffView, { props: { diff } });
    // The split leaves one empty element after a trailing `\n`. Rendering it
    // would add a phantom row to every diff in the app.
    const empties = rows(container).filter((el) => text(el) === "" && el.dataset.k === "ctx");
    expect(empties).toHaveLength(0);
  });

  it("shows code as text and runs none of it", () => {
    const hostile = [
      "--- a/x",
      "+++ b/x",
      "@@ -1 +1 @@",
      "-<script>window.__pwned = 1</script>",
      "+<img src=x onerror=alert(1)>",
      "",
    ].join("\n");

    const { container } = render(DiffView, { props: { diff: hostile } });
    expect(container.querySelector("script")).toBeNull();
    expect(container.querySelector("img")).toBeNull();
    expect((window as unknown as { __pwned?: number }).__pwned).toBeUndefined();
    // The text is still there, so a reader can see exactly what changed.
    expect(container.textContent).toContain("<script>window.__pwned = 1</script>");
  });

  it("keeps the marker line's indentation intact", () => {
    // `pre`, not `pre-wrap`: a diff whose indentation has been collapsed no
    // longer matches the file it describes, and it is the indentation that
    // says which block a change belongs to.
    const indented = [
      "--- a/x",
      "+++ b/x",
      "@@ -1 +1 @@",
      "-    indented()",
      "+        deeper()",
      "",
    ].join("\n");

    const { container } = render(DiffView, { props: { diff: indented } });
    const added = rows(container).find((el) => el.dataset.k === "add");
    expect(added ? text(added) : "").toBe("        deeper()");
  });

  it("labels both halves of a diff that touches two files", () => {
    const two = [
      "--- a/one.rs",
      "+++ b/one.rs",
      "@@ -1 +1 @@",
      "-a",
      "+b",
      "--- a/two.rs",
      "+++ b/two.rs",
      "@@ -1 +1 @@",
      "-c",
      "+d",
      "",
    ].join("\n");

    const { container } = render(DiffView, { props: { diff: two } });
    const heads = rows(container).filter((el) => el.dataset.k === "head").map(text);
    expect(heads).toEqual([
      "--- a/one.rs",
      "+++ b/one.rs",
      "--- a/two.rs",
      "+++ b/two.rs",
    ]);
    // And the strip totals both files, not just the first.
    expect(container.querySelector(".dif__count")?.textContent).toContain("+2");
  });

  it("handles an empty diff without throwing", () => {
    // Reaching here means the caller passed `null` through as `""`, which the
    // view is supposed to prevent — but a blank strip beats a white screen.
    const { container } = render(DiffView, { props: { diff: "" } });
    expect(rows(container)).toHaveLength(0);
  });
});
