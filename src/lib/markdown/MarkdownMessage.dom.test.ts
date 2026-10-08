// @vitest-environment jsdom
/**
 * The markdown renderer, in a DOM.
 *
 * `lex.test.ts` proves the lexer produces the right tokens and
 * `safeUrl.test.ts` proves `safeHref` refuses a `javascript:` URL. Neither
 * proves the component *uses* them. Those are three separate claims, and the
 * third is the one that matters: the whole reason this renderer exists is that
 * the previous one called `marked.parse` and injected the result with
 * `{@html}`, so a model answer could execute script and a model-authored link
 * could be a `javascript:` URL.
 *
 * A token-rendering bug is invisible to every other test in the suite, and it is
 * the bug that matters most. So it is pinned here, in the place where a
 * regression would actually show up.
 */
import { describe, it, expect, beforeEach } from "vitest";
import { render, cleanup } from "@testing-library/svelte";
import MarkdownMessage from "./MarkdownMessage.svelte";
import { setLocale } from "$lib/i18n";

beforeEach(() => {
  cleanup();
  // The renderer reads translated strings (a code block's "copy" affordance, the
  // table caption), so the locale is pinned rather than left to whatever a
  // previous test left behind.
  setLocale("en");
});

const renderMd = (content: string) => render(MarkdownMessage, { props: { content } });

describe("MarkdownMessage", () => {
  // ── the XSS closure ───────────────────────────────────────────────────

  it("shows raw HTML as text rather than executing it", () => {
    const { container } = renderMd('<img src=x onerror="window.__pwned = 1">');
    // The element must not exist in the document at all.
    expect(container.querySelector("img")).toBeNull();
    // And the text must be present, so the reader can see what was written.
    expect(container.textContent).toContain("<img src=x");
  });

  it("does not run a script tag", () => {
    const { container } = renderMd("<script>window.__pwned = 1</script>");
    expect(container.querySelector("script")).toBeNull();
    expect((window as any).__pwned).toBeUndefined();
  });

  it("does not build a javascript: link", () => {
    const { container } = renderMd("[click me](javascript:window.__pwned=1)");
    const anchors = [...container.querySelectorAll("a")];
    for (const a of anchors) {
      const href = a.getAttribute("href") ?? "";
      expect(href.toLowerCase().replace(/\s/g, "")).not.toContain("javascript:");
    }
    // Whatever it became, it must not be a working javascript: URL.
    for (const a of anchors) {
      expect((a.getAttribute("href") ?? "").startsWith("javascript:")).toBe(false);
    }
  });

  /** The obfuscations a model will actually emit, not just the textbook form. */
  it("refuses a javascript: URL however it is spelled", () => {
    const variants = [
      "[a](javascript:alert(1))",
      "[a](JaVaScRiPt:alert(1))",
      "[a](java\tscript:alert(1))",
      "[a]( javascript:alert(1))",
      "[a](&#106;avascript:alert(1))",
      "[a](data:text/html,<script>alert(1)</script>)",
      "[a](vbscript:msgbox(1))",
    ];
    for (const md of variants) {
      const { container, unmount } = renderMd(md);
      for (const a of container.querySelectorAll("a")) {
        const href = (a.getAttribute("href") ?? "").toLowerCase().replace(/\s/g, "");
        expect(href).not.toContain("javascript:");
        expect(href).not.toContain("vbscript:");
        expect(href).not.toContain("data:text/html");
      }
      unmount();
    }
  });

  it("keeps a safe link working", () => {
    const { container } = renderMd("[docs](https://example.com/path?q=1)");
    const a = container.querySelector("a");
    expect(a).not.toBeNull();
    expect(a!.getAttribute("href")).toBe("https://example.com/path?q=1");
  });

  // ── the token renderer actually renders each token type ───────────────

  it("renders a heading, emphasis and inline code", () => {
    const { container } = renderMd("## Heading\n\nSome **bold** and `code` text.");
    expect(container.querySelector("h2")?.textContent).toContain("Heading");
    expect(container.querySelector("strong")?.textContent).toBe("bold");
    expect(container.querySelector("code")?.textContent).toBe("code");
  });

  it("renders a fenced code block as code, not as prose", () => {
    const { container } = renderMd("```rust\nfn main() { let x = 1; }\n```");
    const block = container.querySelector("pre");
    expect(block).not.toBeNull();
    // The braces and semicolon must survive verbatim, which is what a prose
    // renderer would quietly mangle.
    expect(block!.textContent).toContain("let x = 1;");
  });

  /**
   * A code block is the one place a model may legitimately write markup, and it
   * is the place a naive renderer is most likely to execute it.
   */
  it("does not execute markup written inside a code block", () => {
    const { container } = renderMd("```html\n<script>window.__pwned = 1</script>\n```");
    expect(container.querySelector("script")).toBeNull();
    expect(container.querySelector("pre")?.textContent).toContain("<script>");
  });

  it("renders a list as a list", () => {
    const { container } = renderMd("- one\n- two\n- three");
    const items = container.querySelectorAll("li");
    expect(items.length).toBe(3);
    expect([...items].map((i) => i.textContent?.trim())).toEqual(["one", "two", "three"]);
  });

  it("renders a table with a row per line", () => {
    const { container } = renderMd(
      "| Requirement | Status |\n| --- | --- |\n| Rust | Matched |\n| Kinesis | Missing |",
    );
    const rows = container.querySelectorAll("tr");
    // Header plus two body rows.
    expect(rows.length).toBe(3);
    expect(container.querySelectorAll("th").length).toBe(2);
    expect(container.querySelectorAll("td").length).toBe(4);
    expect(container.textContent).toContain("Kinesis");
  });

  it("renders a blockquote", () => {
    const { container } = renderMd("> the office is isolated");
    expect(container.querySelector("blockquote")?.textContent).toContain("isolated");
  });

  it("renders an image as an image", () => {
    const { container } = renderMd("![a diagram](./diagram.png)");
    const img = container.querySelector("img");
    expect(img).not.toBeNull();
    expect(img!.getAttribute("src")).toBe("./diagram.png");
  });

  it("refuses a javascript: image source", () => {
    const { container } = renderMd("![x](javascript:alert(1))");
    const img = container.querySelector("img");
    // Either it is not rendered as an image, or its source is inert. It must
    // never be the javascript: URL itself.
    if (img) {
      expect((img.getAttribute("src") ?? "").toLowerCase()).not.toContain("javascript:");
    }
  });

  // ── robustness of the renderer itself ─────────────────────────────────

  it("renders nothing for empty input without throwing", () => {
    expect(() => renderMd("")).not.toThrow();
    expect(() => renderMd("   \n\n  ")).not.toThrow();
  });

  /**
   * The generator runs on every token, so a model streaming half a table, half
   * a fence or half a link hits it. Unterminated input is the normal state of a
   * message mid-stream, not an edge case.
   */
  it("survives unterminated markdown", () => {
    const partials = [
      "**bold that never closes",
      "`code that never closes",
      "```rust\nfn main() {",
      "| a | b |\n| --- |",
      "[link](http://example.com",
      "```\n| a | b |",
      "> quote\n> still going",
      "[![img](a.png)](http://x)",
    ];
    for (const md of partials) {
      expect(() => renderMd(md), `threw on: ${md}`).not.toThrow();
    }
  });

  it("does not nest an anchor inside an anchor", () => {
    const { container } = renderMd("[![alt](./i.png)](https://example.com)");
    const anchors = container.querySelectorAll("a");
    for (const a of anchors) {
      expect(a.querySelector("a")).toBeNull();
    }
  });

  it("keeps text that looks like markdown as text when it is escaped", () => {
    const { container } = renderMd("\\*not emphasis\\*");
    expect(container.querySelector("em")).toBeNull();
    expect(container.textContent).toContain("*not emphasis*");
  });
});
