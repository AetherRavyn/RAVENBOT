import { describe, it, expect } from "vitest";
import { lexBlocks, lexInline } from "./lex";

describe("math delimiters", () => {
  it("lexes $$ and \\[...\\] as block math", () => {
    const dollars = lexBlocks("$$E=mc^2$$", false) as any[];
    expect(dollars.some((token) => token.type === "mathBlock")).toBe(true);
    const brackets = lexBlocks("\\[\\int_0^1 x\\,dx\\]", false) as any[];
    expect(brackets.some((token) => token.type === "mathBlock")).toBe(true);
  });

  it("lexes $...$ and \\(...\\) as inline math", () => {
    const dollars = lexInline("solve $x^2$ now") as any[];
    expect(dollars.some((token) => token.type === "mathInline")).toBe(true);
    const parens = lexInline("solve \\(x^2\\) now") as any[];
    expect(parens.some((token) => token.type === "mathInline")).toBe(true);
  });

  it("still treats a plain dollar amount as prose", () => {
    const tokens = lexInline("it costs $5 today") as any[];
    expect(tokens.some((token) => token.type === "mathInline")).toBe(false);
  });
});
