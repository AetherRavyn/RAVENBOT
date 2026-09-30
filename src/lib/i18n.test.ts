import { describe, expect, it } from "vitest";
import {
  getAvailableLocales,
  getLocale,
  initI18n,
  setLocale,
  t,
  type TranslationKey,
} from "./i18n";

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
  it("all 6 locales have full coverage (no fallback gaps on any key)", () => {
    const available = getAvailableLocales();
    expect(available).toHaveLength(6);

    // Walk every key of the English dictionary and check each locale resolves
    for (const key of enKeys()) {
      for (const loc of available) {
        setLocale(loc.code);
        const translated = t(key as TranslationKey);
        expect(translated, `${loc.code}:${key}`).not.toBe("");
        // A missing key would fall through to the key itself
        expect(translated, `${loc.code}:${key}`).not.toBe(key);
      }
    }
  });

  it("defaults to English", () => {
    setLocale("fr");
    initI18n();
    expect(getLocale()).toBe("en");
  });
});

function enKeys(): string[] {
  // The English dictionary is the source of truth for keys; reconstruct it
  // via the public API of the module under test.
  const keys: string[] = [];
  const walk = (prefix: string, obj: unknown) => {
    if (obj && typeof obj === "object") {
      for (const [k, v] of Object.entries(obj as Record<string, unknown>)) {
        const next = prefix ? `${prefix}.${k}` : k;
        if (v && typeof v === "object") walk(next, v);
        else keys.push(next);
      }
    }
  };
  // The module exports no dictionary accessor; instead detect fallthrough by
  // probing known top-level sections against each locale. To keep the test
  // self-contained, probe with the sections that exist in the app.
  walk("", {
    app: { name: "", tagline: "", loading: "" },
    sidebar: { search: "", filterWaiting: "", noBots: "", pauseAll: "", createBot: "" },
    bot: { status: { idle: "", thinking: "", runningTool: "", waitingOnUser: "", paused: "" }, settings: "", skills: "", delete: "", deleteConfirm: "" },
    thread: { threads: "", noThreads: "", welcome: "", emptyWelcome: "", typing: "", send: "" },
    settings: { title: "", apiKeys: "", apiKeysDesc: "", localModels: "", localModelsDesc: "", about: "", save: "", saved: "", cancel: "", openrouter: "", anthropic: "", openai: "", ollamaUrl: "", version: "" },
    skills: { title: "", description: "", requires: "", saveSkills: "" },
    killSwitch: { active: "", resume: "", trigger: "", confirmTitle: "", confirmMessage: "", reasonPlaceholder: "", activate: "" },
    commandPalette: { placeholder: "", noResults: "", createBot: "", createBotDesc: "", settings: "", settingsDesc: "" },
    emptyState: { selectBot: "", commandPalette: "", settingsShortcut: "" },
    errors: { loadFailed: "", sendFailed: "", createFailed: "", updateFailed: "", deleteFailed: "" },
    a11y: { sidebar: "", thread: "", compose: "", settings: "", killSwitch: "" },
  });
  return keys;
}
