import { describe, it, expect, beforeEach } from "vitest";
import { Lexer } from "marked";
import { lexBlocks, lexInline, clearLexCaches, lexCacheSize, streamingBlockIndex } from "./lex";
import { tailOffsets } from "$lib/chat/reveal";

/**
 * The token pipeline, checked without a DOM.
 *
 * The renderer is a component tree now, so what is worth testing here is the
 * part that decides *what* it renders: that a body lexes into the block types
 * the component handles, that a streaming body is not cached, and that the
 * tail arithmetic picks the right blocks.
 */

beforeEach(() => clearLexCaches());

/** The token types the block renderer has a case for. */
const HANDLED = new Set([
  "space", "def", "heading", "paragraph", "text", "blockquote", "list",
  "code", "table", "hr", "html", "br",
]);

describe("lexBlocks", () => {
  it("produces block tokens for a normal answer", () => {
    const types = lexBlocks("# Title\n\nSome text.\n\n- a\n- b\n", false).map((t) => t.type);
    expect(types).toContain("heading");
    expect(types).toContain("paragraph");
    expect(types).toContain("list");
  });

  it("produces a code token carrying the code without the fence", () => {
    const tokens = lexBlocks("```ts extra\nconst a = 1;\n```", false);
    const code = tokens.find((t) => t.type === "code") as { lang?: string; text: string };
    expect(code).toBeDefined();
    expect(code.text).toBe("const a = 1;");
    // `lang` still carries the extra words; the component splits it.
    expect(code.lang).toBe("ts extra");
  });

  it("does not produce a token type the renderer lacks a case for", () => {
    // An unhandled type falls through to a raw-text branch, which silently
    // loses formatting. Better to fail here.
    const samples = [
      "# h\n\np\n\n- l\n\n> q\n\n```js\nx\n```\n\n| a |\n|---|\n| 1 |\n\n---\n\n<div>x</div>\n",
      "just text",
      "",
      "1. one\n2. two",
      "- [ ] task\n- [x] done",
    ];
    for (const body of samples) {
      for (const t of lexBlocks(body, false)) {
        expect(HANDLED, `unhandled token type "${t.type}" in ${JSON.stringify(body)}`).toContain(t.type);
      }
    }
  });

  it("caches a settled body and returns the identical token tree", () => {
    const a = lexBlocks("hello", false);
    const b = lexBlocks("hello", false);
    expect(b).toBe(a);
  });

  it("never caches a streaming body", () => {
    // Every revealed word is a new prefix. Caching them would fill the cache
    // with obsolete trees and evict the settled messages worth keeping.
    const first = lexBlocks("hel", true);
    const second = lexBlocks("hel", true);
    expect(second).not.toBe(first);
    expect(second.map((t) => t.raw)).toEqual(first.map((t) => t.raw));
  });

  it("keeps the cache bounded", () => {
    for (let i = 0; i < 260; i += 1) lexBlocks(`body number ${i}`, false);
    // A long session has thousands of messages; an unbounded map would hold
    // every token tree for the life of the window.
    expect(lexCacheSize()).toBeLessThanOrEqual(200);
    expect(lexCacheSize()).toBeGreaterThan(0);
  });

  it("does not let streaming fills count against the bound", () => {
    for (let i = 0; i < 300; i += 1) lexBlocks(`streaming ${i}`, true);
    // Streaming bodies never enter the cache at all, so the settled ones it
    // already holds are still there.
    const settled = lexBlocks("a settled body", false);
    expect(lexCacheSize()).toBe(1);
    expect(lexBlocks("a settled body", false)).toBe(settled);
  });
});

describe("lexInline", () => {
  it("splits inline markup into tokens", () => {
    const types = lexInline("some **bold** and `code`").map((t) => t.type);
    expect(types).toContain("strong");
    expect(types).toContain("codespan");
  });

  it("caches by body", () => {
    expect(lexInline("x")).toBe(lexInline("x"));
  });
});

describe("streamingBlockIndex", () => {
  it("points at the last meaningful block", () => {
    const tokens = lexBlocks("para one\n\npara two\n\n", true);
    const i = streamingBlockIndex(tokens);
    expect(tokens[i].type).toBe("paragraph");
    expect(tokens[i].raw).toContain("para two");
  });

  it("returns -1 for an empty body", () => {
    expect(streamingBlockIndex(lexBlocks("", true))).toBe(-1);
  });

  it("skips trailing whitespace tokens", () => {
    const tokens = lexBlocks("only\n\n\n", true);
    expect(tokens[streamingBlockIndex(tokens)].raw).toBe("only");
  });
});

describe("tail selection", () => {
  /**
   * The arithmetic MarkdownMessage uses: `after[i]` is the number of body
   * characters following block i, and `tailOffsets` decides which blocks fall
   * inside the fading window.
   */
  function offsetsFor(body: string, trailChars: number) {
    const tokens = lexBlocks(body, false);
    const lengths = tokens.map((t) => t.raw.length);
    const after: (number | undefined)[] = new Array(tokens.length);
    let rest = 0;
    for (let i = tokens.length - 1; i >= 0; i -= 1) {
      after[i] = rest;
      rest += lengths[i];
    }
    return tailOffsets(lengths, after[tokens.length - 1], trailChars);
  }

  it("fades only the last block for a small trail", () => {
    const offsets = offsetsFor("first para\n\nsecond para", 5);
    const last = offsets.length - 1;
    expect(offsets[last]).toBe(0);
    expect(offsets[last - 1]).toBeUndefined();
  });

  it("fades the last two blocks for a trail spanning both", () => {
    const offsets = offsetsFor("first para\n\nsecond para", 40);
    const last = offsets.length - 1;
    expect(offsets[last]).toBe(0);
    expect(offsets[last - 1]).toBeDefined();
  });

  it("fades nothing when the trail is empty", () => {
    const offsets = offsetsFor("first para\n\nsecond para", 0);
    expect(offsets.every((o) => o === undefined)).toBe(true);
  });

  it("never lets a settled block fade on a long body", () => {
    const body = Array.from({ length: 20 }, (_, i) => `paragraph ${i}`).join("\n\n");
    const offsets = offsetsFor(body, 10);
    // Only the final two can be inside a 10-character window.
    const fading = offsets.filter((o) => o !== undefined).length;
    expect(fading).toBeLessThanOrEqual(2);
  });
});

describe("raw html is text, not markup", () => {
  it("lexes a tag into an html token the renderer can show as text", () => {
    const tokens = lexBlocks('<img src=x onerror="alert(1)">', false);
    expect(tokens.some((t) => t.type === "html")).toBe(true);
    // It arrives as a token, so the component decides to render it as a
    // string. It is never handed to {@html}.
    expect(tokens[0].raw).toContain("onerror");
  });
});

describe("marked configuration", () => {
  it("treats a single newline as a break inside a paragraph", () => {
    const a = Lexer.lex("one\ntwo", { breaks: true, gfm: true });
    const b = Lexer.lex("one\ntwo", { breaks: false, gfm: true });
    expect(JSON.stringify(a)).not.toBe(JSON.stringify(b));
  });
});
