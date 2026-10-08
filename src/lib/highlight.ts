/**
 * A small, dependency-free syntax highlighter.
 *
 * There is no Shiki or highlight.js in this project, and adding a 500 kB
 * highlighter to a 12 MB release binary is a poor trade for colouring code.
 * So this is a single-pass regex tokenizer covering what actually shows up in
 * agent output: strings, comments, numbers, and keywords, per language family.
 *
 * It is not a parser. A token it gets wrong is a token coloured wrongly — the
 * text is always emitted verbatim and in order, so a mis-tokenised string is
 * cosmetic rather than destructive. That property is what makes a regex
 * acceptable here, and it is checked by `no_text_is_ever_lost`.
 *
 * Skipped entirely on a block that is still streaming, since re-tokenising on
 * every token is the most expensive thing a code block could do.
 */

export interface Token {
  /** `text` passes through; anything else gets a `tok-*` class. */
  type: "text" | "string" | "comment" | "number" | "keyword" | "function" | "newline";
  text: string;
}

/** Normalise a language tag to one of the families below. */
export function hlLanguage(language: string): string {
  const l = (language || "").toLowerCase().trim();
  if (["sh", "bash", "zsh", "shell", "console", "shell-session"].includes(l)) return "sh";
  if (["js", "jsx", "mjs", "cjs", "ts", "tsx", "typescript", "javascript"].includes(l)) return "js";
  if (["py", "python"].includes(l)) return "py";
  if (["rs", "rust"].includes(l)) return "rs";
  if (["go", "golang"].includes(l)) return "go";
  if (["json", "jsonc"].includes(l)) return "json";
  if (["sql"].includes(l)) return "sql";
  if (["html", "xml", "svg", "vue"].includes(l)) return "markup";
  if (["css", "scss", "less"].includes(l)) return "css";
  if (["yaml", "yml", "toml"].includes(l)) return "yaml";
  if (["md", "markdown"].includes(l)) return "md";
  if (["diff", "patch"].includes(l)) return "diff";
  if (["dockerfile"].includes(l) || l === "docker") return "docker";
  return "";
}

interface Grammar {
  keywords: Set<string>;
  lineComment: string[];
  blockComment?: [string, string];
  /** Quote characters that start a string. */
  quotes: string[];
  /** Backslash escapes inside a string, or false when the language has none. */
  escapes: boolean;
}

const js = new Set(
  ("await as async break case catch class const continue debugger declare default delete do else enum export " +
    "extends finally for from function get if implements import in instanceof interface let new of private " +
    "protected public readonly return satisfies set static super switch this throw try type typeof var void while " +
    "yield true false null undefined").split(" "),
);
const py = new Set(
  ("and as assert async await break class continue def del elif else except finally for from global if import " +
    "in is lambda match nonlocal not or pass raise return try while with yield True False None self").split(" "),
);
const rs = new Set(
  ("as async await break const continue crate dyn else enum extern false fn for if impl in let loop match mod " +
    "move mut pub ref return self Self static struct super trait true type unsafe use where while").split(" "),
);
const go = new Set(
  ("break case chan const continue default defer else fallthrough for func go goto if import interface map " +
    "package range return select struct switch type var nil true false").split(" "),
);
const sh = new Set(
  ("if then else elif fi for while do done case esac function return export local readonly declare source " +
    "echo cd set unset trap exit shift eval exec in break continue true false").split(" "),
);
const sql = new Set(
  ("select from where join left right inner outer on group by order having limit offset insert into values " +
    "update set delete create table alter drop index view union all distinct as and or not null is in exists " +
    "primary key foreign references default with returning pragma explain begin commit rollback").split(" "),
);

const GRAMMARS: Record<string, Grammar> = {
  js: { keywords: js, lineComment: ["//"], blockComment: ["/*", "*/"], quotes: ['"', "'", "`"], escapes: true },
  ts: { keywords: js, lineComment: ["//"], blockComment: ["/*", "*/"], quotes: ['"', "'", "`"], escapes: true },
  py: { keywords: py, lineComment: ["#"], quotes: ['"', "'"], escapes: true },
  rs: { keywords: rs, lineComment: ["//"], blockComment: ["/*", "*/"], quotes: ['"'], escapes: true },
  go: { keywords: go, lineComment: ["//"], blockComment: ["/*", "*/"], quotes: ['"', "`"], escapes: true },
  sh: { keywords: sh, lineComment: ["#"], quotes: ['"', "'"], escapes: true },
  sql: { keywords: sql, lineComment: ["--"], blockComment: ["/*", "*/"], quotes: ["'"], escapes: false },
  json: { keywords: new Set(["true", "false", "null"]), lineComment: ["//"], quotes: ['"'], escapes: true },
  yaml: { keywords: new Set(["true", "false", "null", "yes", "no"]), lineComment: ["#"], quotes: ['"', "'"], escapes: true },
  css: { keywords: new Set(["important", "inherit", "initial", "unset"]), lineComment: [], blockComment: ["/*", "*/"], quotes: ['"', "'"], escapes: true },
  markup: { keywords: new Set(), lineComment: [], blockComment: ["<!--", "-->"], quotes: ['"', "'"], escapes: false },
  md: { keywords: new Set(), lineComment: [], quotes: [], escapes: false },
  diff: { keywords: new Set(), lineComment: [], quotes: [], escapes: false },
  docker: { keywords: new Set(["FROM", "RUN", "CMD", "ENTRYPOINT", "COPY", "ADD", "ENV", "WORKDIR", "EXPOSE", "VOLUME", "USER", "ARG", "LABEL"]), lineComment: ["#"], quotes: ['"', "'"], escapes: true },
};

const IDENT = /[A-Za-z_$][A-Za-z0-9_$]*/y;
const NUMBER = /\d[\d_]*(?:\.\d+)?(?:[eE][+-]?\d+)?/y;

/**
 * Tokenize `code`. Returns a single `text` token when the language is unknown,
 * so the renderer does not pay for a scan it cannot colour.
 */
export function hl(code: string, language: string): Token[] {
  const grammar = GRAMMARS[language];
  if (!grammar) return [{ type: "text", text: code }];
  if (code.length > 120_000) return [{ type: "text", text: code }];

  const out: Token[] = [];
  let plain = "";
  let i = 0;

  const flush = () => {
    if (plain) {
      out.push({ type: "text", text: plain });
      plain = "";
    }
  };
  const push = (type: Token["type"], text: string) => {
    flush();
    out.push({ type, text });
  };

  while (i < code.length) {
    const c = code[i];

    if (c === "\n") {
      flush();
      out.push({ type: "newline", text: "\n" });
      i += 1;
      continue;
    }

    // A line comment, checked before identifiers so `#` in a shell script and
    // `//` in Rust are not read as operators.
    const line = grammar.lineComment.find((m) => code.startsWith(m, i));
    if (line) {
      const end = code.indexOf("\n", i);
      const stop = end === -1 ? code.length : end;
      push("comment", code.slice(i, stop));
      i = stop;
      continue;
    }

    const bc = grammar.blockComment;
    if (bc && code.startsWith(bc[0], i)) {
      const end = code.indexOf(bc[1], i + bc[0].length);
      const stop = end === -1 ? code.length : end + bc[1].length;
      push("comment", code.slice(i, stop));
      i = stop;
      continue;
    }

    if (grammar.quotes.includes(c)) {
      let j = i + 1;
      while (j < code.length) {
        if (grammar.escapes && code[j] === "\\") {
          j += 2;
          continue;
        }
        if (code[j] === c) {
          j += 1;
          break;
        }
        // An unterminated string stops at the newline rather than colouring
        // the rest of the file, which is what a half-received block looks like.
        if (code[j] === "\n" && language !== "markdown") break;
        j += 1;
      }
      push("string", code.slice(i, j));
      i = j;
      continue;
    }

    IDENT.lastIndex = i;
    const ident = IDENT.exec(code)?.[0];
    if (ident) {
      const after = code[IDENT.lastIndex];
      // `foo(` or `foo(` after whitespace is a call; a bare word is a keyword
      // only if the grammar lists it.
      if (after === "(") {
        push("function", ident);
      } else if (grammar.keywords.has(ident)) {
        push("keyword", ident);
      } else {
        plain += ident;
      }
      i = IDENT.lastIndex;
      continue;
    }

    NUMBER.lastIndex = i;
    const num = NUMBER.exec(code)?.[0];
    if (num) {
      push("number", num);
      i = NUMBER.lastIndex;
      continue;
    }

    plain += c;
    i += 1;
  }
  flush();
  return out;
}

/** Whether a language has a grammar, so the caller can skip the scan. */
export function hasGrammar(language: string): boolean {
  return language in GRAMMARS;
}
