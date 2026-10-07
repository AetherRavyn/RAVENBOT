import { describe, expect, it } from "vitest";
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
 * The dictionary itself, plus the two rules that keep it honest.
 *
 * The first version of this file probed eleven hardcoded sections through
 * `t()`. That test could not fail, for a reason worth stating: `t()` falls back
 * to **English** before it falls back to the key, so a German string that does
 * not exist returns a perfectly good English string and every assertion below
 * it still passes. Three whole blocks — `reasoning`, `tools`, `projects` — were
 * missing from five locales and the coverage test said everything was fine.
 *
 * So coverage is checked against the locale files, not through `t()`, and the
 * section list is walked from English rather than typed by hand. A key added
 * to English tomorrow is checked against six locales tomorrow too.
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
});
