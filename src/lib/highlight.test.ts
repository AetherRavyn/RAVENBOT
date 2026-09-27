import { describe, it, expect } from "vitest";
import { hl, hlLanguage, hasGrammar } from "./highlight";

/** The invariant that makes a regex highlighter acceptable here. */
function render(tokens: { text: string }[]): string {
  return tokens.map((t) => t.text).join("");
}

describe("hlLanguage", () => {
  it("normalises tags and aliases to a family", () => {
    expect(hlLanguage("ts")).toBe("js");
    expect(hlLanguage("TypeScript")).toBe("js");
    expect(hlLanguage("rs")).toBe("rs");
    expect(hlLanguage("shell")).toBe("sh");
    expect(hlLanguage("bash")).toBe("sh");
    expect(hlLanguage("golang")).toBe("go");
  });

  it("returns empty for a language it has no grammar for", () => {
    expect(hlLanguage("brainfuck")).toBe("");
    expect(hlLanguage("")).toBe("");
  });
});

describe("no text is ever lost", () => {
  // The whole justification for a regex tokenizer: a token it gets wrong is a
  // token coloured wrongly, because the source is emitted verbatim and in
  // order. If this ever fails, the highlighter is destructive.
  const samples: [string, string][] = [
    ["js", `const x = "hello"; // note\nfunction f(a) { return a + 1; }`],
    ["py", `def f(x):\n    # comment\n    return {"a": 1, 'b': [2, 3]}`],
    ["rs", `fn main() { let s = "x"; /* c */ println!("{}", s); }`],
    ["go", `package main\n// hi\nfunc main() { fmt.Println("x") }`],
    ["sh", `#!/bin/sh\n# set it\nnpm install "my pkg" --save`],
    ["json", `{ "a": 1, "b": [true, null], "c": "s" }`],
    ["sql", `SELECT * FROM t WHERE a = 1; -- note`],
    ["css", `.a { color: #fff; content: "x"; }`],
    ["yaml", `key: value # note\nlist:\n  - a`],
    ["docker", `FROM node:20\nRUN npm ci && echo "ok"`],
    ["diff", `--- a\n+++ b\n@@ -1 +1 @@\n-old\n+new`],
  ];

  for (const [lang, code] of samples) {
    it(`preserves every character of a ${lang} sample`, () => {
      expect(render(hl(code, lang))).toBe(code);
    });
  }

  it("preserves a pathological input", () => {
    const nasty = [
      "",
      '"',
      "'",
      "/*",
      "//",
      "`unterminated",
      "a\\",
      "  \t\n  ",
      "$'",
      "𝔘𝔫𝔦𝔠𝔬𝔡𝔢",
      "x".repeat(500),
    ].join("\n");
    expect(render(hl(nasty, "js"))).toBe(nasty);
    expect(render(hl(nasty, "sh"))).toBe(nasty);
  });
});

describe("token classification", () => {
  it("marks keywords, strings, comments, numbers and calls", () => {
    const tokens = hl('const n = 42; // note\nf("s");', "js");
    const byType = (t: string) => tokens.filter((x) => x.type === t).map((x) => x.text);
    expect(byType("keyword")).toContain("const");
    expect(byType("number")).toContain("42");
    expect(byType("comment")).toContain("// note");
    expect(byType("string")).toContain('"s"');
    expect(byType("function")).toContain("f");
  });

  it("does not read a comment as code", () => {
    const tokens = hl("// const fake = 1\nconst real = 2;", "js");
    const comment = tokens.find((t) => t.type === "comment");
    expect(comment?.text).toBe("// const fake = 1");
    // The `const` inside the comment is part of the comment token, so exactly
    // one keyword comes back — the one in the real code.
    expect(tokens.filter((t) => t.type === "keyword").map((t) => t.text)).toEqual(["const"]);
  });

  it("stops an unterminated string at the newline", () => {
    // What a half-received block looks like: colouring the rest of the file
    // because a quote has not closed yet would be unreadable.
    const tokens = hl('let a = "open\nlet b = 1;', "js");
    const str = tokens.find((t) => t.type === "string");
    expect(str?.text).toBe('"open');
  });

  it("handles an escaped quote inside a string", () => {
    const tokens = hl('let a = "he said \\"hi\\""; let b = 2;', "js");
    const str = tokens.find((t) => t.type === "string");
    expect(str?.text).toBe('"he said \\"hi\\""');
  });

  it("handles a block comment", () => {
    const tokens = hl("/* multi\nline */ let x = 1;", "js");
    expect(tokens.find((t) => t.type === "comment")?.text).toBe("/* multi\nline */");
  });

  it("keeps markdown fences for a shell comment", () => {
    const tokens = hl("#!/bin/sh\nnpm i", "sh");
    expect(tokens.find((t) => t.type === "comment")?.text).toBe("#!/bin/sh");
  });

  it("returns one text token for a language with no grammar", () => {
    const tokens = hl("some random text", "");
    expect(tokens).toEqual([{ type: "text", text: "some random text" }]);
  });

  it("bails out on a very large block", () => {
    // A 12 MB binary should not be spending time tokenising a 130 kB paste.
    const big = "x = 1\n".repeat(20_000) + "y";
    expect(big.length).toBeGreaterThan(120_000);
    expect(hl(big, "js")).toEqual([{ type: "text", text: big }]);
  });
});

describe("hasGrammar", () => {
  it("reports which languages are coloured", () => {
    expect(hasGrammar("js")).toBe(true);
    expect(hasGrammar("")).toBe(false);
  });
});
