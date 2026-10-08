import { describe, expect, it } from "vitest";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import {
  getAvailableLocales,
  getLocale,
  initI18n,
  setLocale,
  t,
  type Locale,
  type TranslationKey,
} from "./i18n";
import en from "./i18n/locales/en.json";
import de from "./i18n/locales/de.json";
import es from "./i18n/locales/es.json";
import fr from "./i18n/locales/fr.json";
import ja from "./i18n/locales/ja.json";
import zh from "./i18n/locales/zh.json";

/**
 * The dictionary itself, plus the rules that keep it honest.
 *
 * The first version of this file probed eleven hardcoded sections through
 * `t()`. That test could not fail, for a reason worth stating: `t()` falls back
 * to **English** before it falls back to the key, so a German string that does
 * not exist returns a perfectly good English string and every assertion below
 * it still passes. Three whole blocks — `reasoning`, `tools`, `projects` — were
 * missing from five locales and the coverage test said everything was fine.
 *
 * So the key list is walked from English rather than typed by hand, and the
 * checks read the locale files instead of going through `t()`. Four guards,
 * each named after the defect it would have caught:
 *
 *  1. every English key exists in every locale, in its own dictionary;
 *  2. no locale carries a key English does not;
 *  3. no string uses a single-brace placeholder `t()` will never fill, and
 *     every `{{x}}` in every locale actually resolves;
 *  4. a key's placeholders are the same names in all six locales, and every
 *     call site passes the ones its string asks for.
 *
 * (3) and (4) are invisible to (1): the key is present, non-empty, and still
 * renders `{{count}}` to the user.
 */

const dictionaries: Record<string, unknown> = { en, de, es, fr, ja, zh };

type Flat = Record<string, string>;

function flatten(obj: unknown, prefix = "", out: Flat = {}): Flat {
  if (obj && typeof obj === "object") {
    for (const [k, v] of Object.entries(obj as Record<string, unknown>)) {
      flatten(v, prefix ? `${prefix}.${k}` : k, out);
    }
  } else if (typeof obj === "string") {
    out[prefix] = obj;
  }
  return out;
}

const flat = new Map(Object.entries(dictionaries).map(([code, d]) => [code, flatten(d)]));

describe("t()", () => {
  it("returns the English translation by key", () => {
    expect(t("app.name")).toBe("RAVENBOT");
    expect(t("thread.send")).toBe("Send message");
  });

  it("substitutes params", () => {
    expect(t("skills.title", { botName: "Atlas" })).toBe("Skills - Atlas");
    expect(t("bot.deleteConfirm", { name: "Atlas" })).toBe(
      'Delete "Atlas"? This cannot be undone.'
    );
  });

  it("falls back to the key when missing", () => {
    expect(t("nonexistent.key" as never)).toBe("nonexistent.key");
  });
});

describe("locale switching", () => {
  it("all 6 locales carry every English key in their own dictionary", () => {
    expect(getAvailableLocales()).toHaveLength(6);

    const gaps: string[] = [];
    for (const [code, entries] of flat) {
      for (const key of Object.keys(flat.get("en")!)) {
        if (typeof entries[key] !== "string" || entries[key] === "") {
          gaps.push(`${code}:${key}`);
        }
      }
    }
    expect(gaps).toEqual([]);
  });

  it("has no keys that only exist in a translation", () => {
    // A key nobody reads is a typo waiting to happen — `projets.title` passes
    // every other check in this file and is simply never shown to anybody.
    const english = new Set(Object.keys(flat.get("en")!));
    const strays: string[] = [];
    for (const [code, entries] of flat) {
      for (const key of Object.keys(entries)) {
        if (!english.has(key)) strays.push(`${code}:${key}`);
      }
    }
    expect(strays).toEqual([]);
  });

  it("defaults to English", () => {
    setLocale("fr");
    initI18n();
    expect(getLocale()).toBe("en");
  });
});

/**
 * `t()` fills `{{name}}`, so a string written with one brace is not a
 * placeholder at all — it is text, shown to the user, braces and all, in
 * every locale. Four of those shipped: the avatar cadence tooltip in all six,
 * the reasoning step count, the workspace count, and a German office button
 * that was a single `}}` short of working.
 *
 * `${VAR}` is exempt: those are shell syntax inside a connectors string and
 * must reach the shell untouched.
 */
describe("placeholders", () => {
  const stray = /(?<![$\\])\{[A-Za-z_][A-Za-z_0-9]*\}(?!\})/;

  it("has no single-brace placeholders in any locale", () => {
    const offenders: string[] = [];
    for (const [code, entries] of flat) {
      for (const [key, value] of Object.entries(entries)) {
        const match = value.match(stray);
        if (match) offenders.push(`${code}:${key} = ${JSON.stringify(value)}`);
      }
    }
    expect(offenders).toEqual([]);
  });

  it("fills every placeholder in every locale, not just the first", () => {
    // A plain `String.replace` swaps one occurrence, so a string that repeats
    // a placeholder renders the rest of them literally.
    const leftovers: string[] = [];
    for (const [code, entries] of flat) {
      setLocale(code as Locale);
      for (const [key, value] of Object.entries(entries)) {
        const params: Record<string, string> = {};
        for (const m of value.matchAll(/\{\{([A-Za-z_][A-Za-z_0-9]*)\}\}/g)) {
          params[m[1]] = "X";
        }
        if (!Object.keys(params).length) continue;
        const out = t(key as TranslationKey, params);
        if (out.includes("{{")) leftovers.push(`${code}:${key} = ${JSON.stringify(out)}`);
      }
    }
    expect(leftovers).toEqual([]);
    setLocale("en");
  });

  it("asks for the same placeholder names in every locale", () => {
    // A translation that renames the placeholder fills nothing: the caller
    // passes `count`, the German string asks for `n`, and the user reads
    // `{{n}}`. Coverage cannot see it — the key exists and is not empty.
    const names = (value: string) =>
      [...value.matchAll(/\{\{([A-Za-z_][A-Za-z_0-9]*)\}\}/g)]
        .map((m) => m[1])
        .sort()
        .join(",");

    const english = flat.get("en")!;
    const drifted: string[] = [];
    for (const [key, value] of Object.entries(english)) {
      for (const [code, entries] of flat) {
        if (code === "en") continue;
        const got = names(entries[key]);
        if (got !== names(value)) {
          drifted.push(`${code}:${key} — wants [${got}], English wants [${names(value)}]`);
        }
      }
    }
    expect(drifted).toEqual([]);
  });
});

/**
 * The other half of a placeholder: the call site that supplies it.
 *
 * `t("settings.testReply", { replyy })` fills nothing — the string asks for
 * `{{reply}}` and gets it left on screen. Only the direction that produces
 * visible text is checked; passing an unused param is harmless, and failing
 * the build on it would be a rule with no bug behind it.
 *
 * Deliberately conservative. The object is matched with `[^{}]*`, so a call
 * whose value contains a nested object or function call does not match at all
 * and is simply not checked; a value containing a top-level comma (an array
 * literal) parses as unrecognised and is skipped. Under-reporting is the right
 * trade — a source-scanning test that fails on a legitimate call gets deleted,
 * which loses every other check in this file with it.
 */
describe("call sites", () => {
  /** Property names in a simple object literal, or `null` if not statically obvious. */
  function paramNames(args: string): string[] | null {
    const names: string[] = [];
    for (const raw of args.split(",")) {
      const part = raw.trim();
      if (!part) continue;
      const explicit = part.match(/^([A-Za-z_][A-Za-z_0-9]*)\s*:/);
      const shorthand = part.match(/^([A-Za-z_][A-Za-z_0-9]*)$/);
      if (explicit) names.push(explicit[1]);
      else if (shorthand) names.push(shorthand[1]);
      else return null;
    }
    return names;
  }

  function walk(dir: string, out: string[] = []): string[] {
    for (const entry of readdirSync(dir)) {
      const full = join(dir, entry);
      if (statSync(full).isDirectory()) walk(full, out);
      else if (
        (entry.endsWith(".svelte") || entry.endsWith(".ts")) &&
        !entry.endsWith(".test.ts")
      ) {
        out.push(full);
      }
    }
    return out;
  }

  it("passes every placeholder its string asks for", () => {
    const english = flat.get("en")!;
    const gaps: string[] = [];

    for (const file of walk("src")) {
      const source = readFileSync(file, "utf8");
      // Built per file: `matchAll` carries `lastIndex` into a clone, and a
      // shared `g` regex is one more thing to get wrong for no benefit.
      const call = /\bt\(\s*"([A-Za-z0-9_.]+)"\s*,\s*\{([^{}]*)\}/g;
      for (const m of source.matchAll(call)) {
        const key = m[1];
        const value = english[key];
        if (typeof value !== "string") continue;

        const passed = paramNames(m[2]);
        if (passed === null) continue;

        const wanted = [...value.matchAll(/\{\{([A-Za-z_][A-Za-z_0-9]*)\}\}/g)].map((x) => x[1]);
        const missing = wanted.filter((w) => !passed.includes(w));
        if (!missing.length) continue;

        const line = source.slice(0, m.index).split("\n").length;
        gaps.push(
          `${file}:${line} — t("${key}") wants ${missing
            .map((w) => `{{${w}}}`)
            .join(", ")}, passed {${passed.join(", ")}}`
        );
      }
    }
    expect(gaps).toEqual([]);
  });
});
