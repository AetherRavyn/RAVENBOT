// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { setPaletteThemes } from "./theme";
import { startThemePaletteSync } from "./themePalette";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);

function paletteFile() {
  return {
    file_name: "midnight.json",
    contents: JSON.stringify({
      name: "Midnight",
      mode: "dark",
      tokens: {
        background: "#0b0b12",
        card: "#14141f",
        border: "#2a2a3d",
        primary: "#8b5cf6",
        accent: "#22d3ee",
        text: "#e8e8f0",
        muted: "#9a9ab0",
      },
    }),
  };
}

describe("theme palette startup", () => {
  beforeEach(() => {
    localStorage.clear();
    invokeMock.mockReset();
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {},
      configurable: true,
    });
  });

  afterEach(() => {
    setPaletteThemes([]);
    localStorage.clear();
    Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
  });

  it("applies a saved built-in theme even when the palette directory is empty", async () => {
    invokeMock.mockResolvedValue([]);
    localStorage.setItem("raven-theme", "paper-light");
    const stop = startThemePaletteSync(60_000);
    await vi.waitFor(() =>
      expect(document.documentElement.getAttribute("data-theme")).toBe("paper-light"),
    );
    expect(localStorage.getItem("raven-theme")).toBe("paper-light");
    stop();
  });

  it("waits for palette files before applying a saved custom theme", async () => {
    invokeMock.mockResolvedValue([paletteFile()]);
    localStorage.setItem("raven-theme", "midnight");
    const stop = startThemePaletteSync(60_000);
    await vi.waitFor(() =>
      expect(document.documentElement.getAttribute("data-theme")).toBe("midnight"),
    );
    expect(localStorage.getItem("raven-theme")).toBe("midnight");
    stop();
  });

  it("keeps a saved id that is temporarily unavailable instead of replacing it", async () => {
    invokeMock.mockResolvedValue([]);
    localStorage.setItem("raven-theme", "missing-palette");
    const stop = startThemePaletteSync(60_000);
    await vi.waitFor(() =>
      expect(document.documentElement.getAttribute("data-theme")).toBe("openbot"),
    );
    expect(localStorage.getItem("raven-theme")).toBe("missing-palette");
    stop();
  });
});
