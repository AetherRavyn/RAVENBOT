import { afterEach, describe, expect, it, vi } from "vitest";
import {
  applyPaletteFiles,
  paletteThemeFromJson,
  parseThemePaletteFiles,
  type ThemeFilePayload,
} from "./themePalette";
import { getPaletteThemes, getThemes, setPaletteThemes, THEMES } from "./theme";

function palette(overrides: Record<string, unknown> = {}): string {
  return JSON.stringify({
    name: "Midnight Bloom",
    mode: "dark",
    tokens: {
      background: "#0b0b12",
      card: "#14141f",
      border: "#2a2a3d",
      primary: "#8b5cf6",
      accent: "#22d3ee",
      text: "#e8e8f0",
      muted: "#9a9ab0",
      ...overrides,
    },
  });
}

function file(name: string, contents: string): ThemeFilePayload {
  return { file_name: name, contents };
}

afterEach(() => {
  setPaletteThemes([]);
  vi.restoreAllMocks();
});

describe("palette file validation", () => {
  it("maps the required tokens and takes the id from the file name", () => {
    const result = paletteThemeFromJson("midnight-bloom", JSON.parse(palette()));
    expect("theme" in result).toBe(true);
    if (!("theme" in result)) return;
    expect(result.theme).toMatchObject({
      id: "midnight-bloom",
      name: "Midnight Bloom",
      category: "Custom Dark",
      bgHex: "#0b0b12",
      cardHex: "#14141f",
      borderHex: "#2a2a3d",
      primaryColor: "#8b5cf6",
      accentColor: "#22d3ee",
      textColor: "#e8e8f0",
      mutedTextColor: "#9a9ab0",
      source: "palette",
      file: "midnight-bloom.json",
    });
  });

  it("accepts #rrggbbaa and keeps it on the raw token", () => {
    const result = paletteThemeFromJson(
      "alpha",
      JSON.parse(
        JSON.stringify({
          name: "Alpha",
          mode: "light",
          tokens: {
            background: "#ffffffee",
            card: "#ffffffff",
            border: "#00000022",
            primary: "#2563ebcc",
            accent: "#1d4ed8",
            text: "#18181b",
            muted: "#71717a",
          },
        }),
      ),
    );
    expect("theme" in result).toBe(true);
    if (!("theme" in result)) return;
    expect(result.theme.bgHex).toBe("#ffffffee");
    expect(result.theme.borderHex).toBe("#00000022");
    expect(result.theme.category).toBe("Custom Light");
  });

  it("requires a name, a mode and every required token", () => {
    expect(paletteThemeFromJson("x", { mode: "dark", tokens: {} })).toHaveProperty(
      "reason",
      "name must be a non-empty string",
    );
    expect(
      paletteThemeFromJson("x", { name: "X", mode: "dim", tokens: {} }),
    ).toHaveProperty("reason", 'mode must be "light" or "dark"');
    const missing = paletteThemeFromJson(
      "x",
      JSON.parse(palette({ muted: undefined })),
    );
    expect(missing).toHaveProperty("reason", "tokens.muted is required");
  });

  it("rejects a bad hex, a mismatched id and a built-in collision", () => {
    expect(
      paletteThemeFromJson("x", JSON.parse(palette({ primary: "blue" }))),
    ).toHaveProperty("reason", "tokens.primary must be #rrggbb or #rrggbbaa");
    expect(
      paletteThemeFromJson("x", { ...JSON.parse(palette()), id: "y" }),
    ).toHaveProperty("reason", 'id "y" must match the file name "x"');
    expect(
      paletteThemeFromJson("openbot", JSON.parse(palette())),
    ).toHaveProperty("reason", 'id "openbot" collides with a built-in theme');
  });

  it("reports per-file warnings and ignores invalid documents", () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    const result = parseThemePaletteFiles([
      file("good.json", palette()),
      file("bad.json", "{ not json"),
      file("wrong.txt", palette()),
      file("BadCase.json", palette()),
      file("short.json", palette({ text: "#12345" })),
    ]);
    expect(result.themes.map((theme) => theme.id)).toEqual(["good"]);
    expect(result.invalidIds).toContain("bad");
    expect(result.invalidIds).toContain("short");
    expect(result.warnings.map((warning) => warning.file)).toEqual([
      "bad.json",
      "BadCase.json",
      "short.json",
    ]);
  });
});

describe("palette hot reload", () => {
  it("adds a renamed file and removes the old id", () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    applyPaletteFiles([file("first.json", palette())]);
    expect(getPaletteThemes().map((theme) => theme.id)).toEqual(["first"]);

    applyPaletteFiles([file("second.json", palette())]);
    expect(getPaletteThemes().map((theme) => theme.id)).toEqual(["second"]);
    expect(getThemes().some((theme) => theme.id === "first")).toBe(false);
    expect(getThemes().slice(0, THEMES.length)).toEqual(THEMES);
  });

  it("applies a write to the same id", () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    applyPaletteFiles([file("bloom.json", palette())]);
    expect(getPaletteThemes()[0].primaryColor).toBe("#8b5cf6");

    applyPaletteFiles([file("bloom.json", palette({ primary: "#f472b6" }))]);
    expect(getPaletteThemes()[0].primaryColor).toBe("#f472b6");
  });

  it("keeps the last good definition while a saved file is invalid", () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    applyPaletteFiles([file("bloom.json", palette())]);
    expect(getPaletteThemes()[0].primaryColor).toBe("#8b5cf6");

    applyPaletteFiles([file("bloom.json", "{ half a writ")]);
    expect(getPaletteThemes()[0].primaryColor).toBe("#8b5cf6");
    expect(getPaletteThemes()[0].name).toBe("Midnight Bloom");
  });

  it("removes an invalid file that was never valid", () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    applyPaletteFiles([file("broken.json", "{")]);
    expect(getPaletteThemes()).toEqual([]);
  });
});
