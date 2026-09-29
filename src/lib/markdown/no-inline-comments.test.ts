import { describe, it, expect } from "vitest";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

/**
 * Guard against a comment silently changing rendered text.
 *
 * An HTML comment in a Svelte template is a *node*. That is the whole problem: a
 * comment inside a rendered branch makes the whitespace around it significant,
 * so Svelte collapses the newline beside it into a real space text node. The
 * markdown inline renderer had two such comments, and every message in the app
 * gained a space between every inline element — `**bold**` rendered as "bold ",
 * `a **b** c` as "a b  c", and text copied out of a reply carried the extra
 * space with it. Nothing looked broken; the text was just quietly wrong, which
 * is the worst way for it to be wrong.
 *
 * Svelte offers no comment form that leaves nothing behind. The brace-and-slash
 * form — an expression tag containing only a comment — is a parse error,
 * because an expression with no expression in it is an empty expression. So the fix is to put the explanation in the `<script>` block,
 * where it belongs anyway, and reserve `<!-- … -->` for `svelte-ignore` — a
 * compiler directive, stripped at compile time, which has to sit immediately
 * before the element it silences.
 *
 * The other half of the same bug needs no comment at all: two blocks separated
 * by a newline get a space between them. `MarkdownMessage.dom.test.ts` pins that
 * by asserting on `textContent`, which is the only place it is observable.
 *
 * Scope is the markdown renderers, because that is where inline content is
 * emitted character-for-character. A comment between two *block* elements is
 * invisible, since HTML does not render whitespace between blocks.
 */

const DIR = "src/lib/markdown";

function walk(dir: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) walk(full, out);
    else if (entry.endsWith(".svelte")) out.push(full);
  }
  return out;
}

/**
 * Comments in a file's *markup*, as 1-based line numbers, with the compiler
 * directives removed.
 *
 * Only the markup matters. A `//` comment in the script is a JS comment and
 * reaches the browser as nothing at all; the whole point is to move
 * explanations there, so the guard has to permit them.
 */
function markupComments(source: string): number[] {
  const scriptEnd = source.indexOf("</script>");
  // Everything before the closing tag is the script; a line number shift keeps
  // the report pointing at real lines in the file.
  const scriptLines = scriptEnd === -1 ? 0 : source.slice(0, scriptEnd).split("\n").length - 1;
  const markup = source.slice(scriptEnd + "</script>".length);

  const found: number[] = [];
  markup.split("\n").forEach((line, i) => {
    const withoutDirectives = line.replace(/<!--\s*svelte-ignore\s+[\w-]+\s*-->/g, "");
    if (withoutDirectives.includes("<!--")) found.push(i + 1 + scriptLines);
  });
  return found;
}

describe("markdown renderers", () => {
  it("carry no HTML comments in their markup", () => {
    const offenders = walk(DIR)
      .map((file) => ({ file, lines: markupComments(readFileSync(file, "utf8")) }))
      .filter((x) => x.lines.length > 0);

    expect(
      offenders,
      `An HTML comment in a rendered branch becomes a DOM node, and the whitespace beside it becomes a real space text node — so every message gains a space between its inline elements. Move the explanation into the <script> block:\n${offenders
        .map((o) => `  ${o.file}:${o.lines.join(", ")}`)
        .join("\n")}`,
    ).toEqual([]);
  });

  it("still allows svelte-ignore, which is stripped rather than rendered", () => {
    // A comment is only dangerous where it is rendered. A directive next to an
    // element is removed by the compiler, so the guard above has to let it
    // through, and this records that the exemption is deliberate.
    const withDirective = walk(DIR).filter((f) =>
      readFileSync(f, "utf8").includes("<!-- svelte-ignore"),
    );
    expect(markupComments(readFileSync(withDirective[0] ?? DIR, "utf8"))).toEqual([]);
  });
});
