/**
 * Local palette files.
 *
 * A palette is a small, validated JSON document in `~/RAVENBOT/themes/`. It is
 * intentionally not a stylesheet, a URL, or a component: a theme file can only
 * name colours and a light/dark mode, so loading one cannot run code, fetch
 * anything, or replace the saved theme with an invalid document.
 */
import { invoke } from "@tauri-apps/api/core";
import {
  applySavedTheme,
  flatBrand,
  getPaletteThemes,
  isHexColor,
  setPaletteThemes,
  THEMES,
  type ThemeDefinition,
} from "$lib/theme";

export interface ThemeFilePayload {
  file_name: string;
  contents: string;
}

export interface ThemePaletteWarning {
  file: string;
  reason: string;
}

export interface ThemePaletteResult {
  themes: ThemeDefinition[];
  /** Valid file names whose contents were rejected, for last-good retention. */
  invalidIds: string[];
  warnings: ThemePaletteWarning[];
}

/** The seven colours every palette must name. */
export const PALETTE_REQUIRED_TOKENS = [
  "background",
  "card",
  "border",
  "primary",
  "accent",
  "text",
  "muted",
] as const;

/** Extra colours a palette may name. */
export const PALETTE_OPTIONAL_TOKENS = [
  "secondaryAccent",
  "button",
  "buttonForeground",
] as const;

const PALETTE_ID = /^[a-z0-9][a-z0-9._-]*$/;
const MAX_THEME_FILE_BYTES = 64 * 1024;
const MAX_THEME_NAME = 80;

function fileStem(fileName: string): string {
  const base = fileName.split(/[\\/]/).pop() ?? "";
  if (!base.toLowerCase().endsWith(".json")) return "";
  return base.slice(0, -".json".length);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function nonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

/**
 * Validate one palette document.
 *
 * The error strings are user-facing hints in the console, not exception text,
 * so they name the offending field and the expected syntax.
 */
export function paletteThemeFromJson(
  id: string,
  raw: unknown,
): { theme: ThemeDefinition } | { reason: string } {
  if (!PALETTE_ID.test(id) || id.length > 64) {
    return { reason: "file name must be a lowercase id such as midnight-bloom.json" };
  }
  if (THEMES.some((theme) => theme.id === id)) {
    return { reason: `id "${id}" collides with a built-in theme` };
  }
  if (!isRecord(raw)) return { reason: "top level must be a JSON object" };

  const declaredId = raw.id;
  if (declaredId !== undefined && declaredId !== id) {
    return { reason: `id "${String(declaredId)}" must match the file name "${id}"` };
  }
  const name = raw.name;
  if (!nonEmptyString(name)) return { reason: "name must be a non-empty string" };
  if (name.trim().length > MAX_THEME_NAME) {
    return { reason: `name must be at most ${MAX_THEME_NAME} characters` };
  }
  const mode = raw.mode;
  if (mode !== "light" && mode !== "dark") {
    return { reason: 'mode must be "light" or "dark"' };
  }
  const tokens = raw.tokens;
  if (!isRecord(tokens)) return { reason: "tokens must be an object" };

  const colours: Record<string, string> = {};
  for (const key of PALETTE_REQUIRED_TOKENS) {
    const value = tokens[key];
    if (value === undefined) return { reason: `tokens.${key} is required` };
    if (!isHexColor(value)) {
      return { reason: `tokens.${key} must be #rrggbb or #rrggbbaa` };
    }
    colours[key] = value.toLowerCase();
  }
  for (const key of PALETTE_OPTIONAL_TOKENS) {
    const value = tokens[key];
    if (value === undefined) continue;
    if (!isHexColor(value)) {
      return { reason: `tokens.${key} must be #rrggbb or #rrggbbaa` };
    }
    colours[key] = value.toLowerCase();
  }

  const description = raw.description;
  if (description !== undefined && typeof description !== "string") {
    return { reason: "description must be a string" };
  }
  const cleanName = name.trim();
  const label = mode === "light" ? "LIGHT" : "DARK";
  return {
    theme: {
      id,
      name: cleanName,
      category: mode === "light" ? "Custom Light" : "Custom Dark",
      primaryColor: colours.primary,
      accentColor: colours.accent,
      secondaryAccent: colours.secondaryAccent,
      buttonHex: colours.button,
      buttonForegroundHex: colours.buttonForeground,
      bgHex: colours.background,
      cardHex: colours.card,
      borderHex: colours.border,
      textColor: colours.text,
      mutedTextColor: colours.muted,
      description:
        (description?.trim() || `${cleanName} — local ${mode} palette`),
      brand: flatBrand(
        cleanName.toUpperCase().slice(0, 18),
        label,
        `LOCAL ${label}`,
        `${cleanName} — a palette file on this machine`,
        "Loaded from ~/RAVENBOT/themes",
      ),
      source: "palette",
      file: `${id}.json`,
    },
  };
}

/**
 * Parse the backend's raw file list.
 *
 * Invalid documents are dropped; the caller gets one warning per file. A
 * valid-looking filename with invalid JSON is reported in `invalidIds` so the
 * runtime can keep the previous good definition while the file is half-saved.
 */
export function parseThemePaletteFiles(files: ThemeFilePayload[]): ThemePaletteResult {
  const themes: ThemeDefinition[] = [];
  const invalidIds: string[] = [];
  const warnings: ThemePaletteWarning[] = [];
  const seen = new Set<string>();

  for (const file of files) {
    const file_name = typeof file?.file_name === "string" ? file.file_name : "";
    const contents = typeof file?.contents === "string" ? file.contents : "";
    if (!file_name.toLowerCase().endsWith(".json")) continue;
    const id = fileStem(file_name);
    if (!PALETTE_ID.test(id)) {
      warnings.push({
        file: file_name,
        reason: "file name must be a lowercase id such as midnight-bloom.json",
      });
      continue;
    }
    if (seen.has(id)) {
      warnings.push({ file: file_name, reason: `duplicate id "${id}"` });
      continue;
    }
    seen.add(id);
    if (contents.length > MAX_THEME_FILE_BYTES) {
      warnings.push({ file: file_name, reason: "file is larger than 64 KB" });
      invalidIds.push(id);
      continue;
    }
    let raw: unknown;
    try {
      raw = JSON.parse(contents);
    } catch {
      warnings.push({ file: file_name, reason: "not valid JSON" });
      invalidIds.push(id);
      continue;
    }
    const parsed = paletteThemeFromJson(id, raw);
    if ("reason" in parsed) {
      warnings.push({ file: file_name, reason: parsed.reason });
      invalidIds.push(id);
      continue;
    }
    themes.push(parsed.theme);
  }

  themes.sort((a, b) => a.id.localeCompare(b.id));
  return { themes, invalidIds, warnings };
}

let lastWarningSignature = "";

/**
 * Apply a file list, retaining the previous definition for files that are
 * temporarily invalid (mid-save) so an editor cannot blank the active theme.
 */
export function applyPaletteFiles(files: ThemeFilePayload[]): ThemePaletteResult {
  const result = parseThemePaletteFiles(files);
  const previous = getPaletteThemes();
  const retained = previous.filter((theme) => result.invalidIds.includes(theme.id));
  setPaletteThemes([...result.themes, ...retained]);

  const signature = result.warnings
    .map((warning) => `${warning.file}: ${warning.reason}`)
    .join("\n");
  if (signature !== lastWarningSignature) {
    lastWarningSignature = signature;
    for (const warning of result.warnings) {
      console.warn(`[themes] ${warning.file}: ${warning.reason}`);
    }
  }
  return result;
}

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** Read and apply the palette directory now. */
export async function refreshPaletteThemes(): Promise<ThemePaletteResult> {
  if (!isTauriRuntime()) return applyPaletteFiles([]);
  try {
    const files = await invoke<ThemeFilePayload[]>("list_theme_files");
    return applyPaletteFiles(Array.isArray(files) ? files : []);
  } catch (error) {
    // A missing or unreadable directory is not a broken app; the built-ins
    // remain. Log once per failure text rather than every poll.
    const reason = String(error);
    if (reason !== lastWarningSignature) {
      lastWarningSignature = reason;
      console.warn(`[themes] could not read ~/RAVENBOT/themes: ${reason}`);
    }
    return { themes: [], invalidIds: [], warnings: [] };
  }
}

let subscribers = 0;
let timer: ReturnType<typeof setInterval> | null = null;
let focusListener: (() => void) | null = null;

/**
 * Watch the palette directory while the workspace is mounted.
 *
 * A two-second poll is deliberate. The Tauri process already owns the
 * filesystem, but an OS watcher would add a native dependency and a lifecycle
 * for one small folder; a read of a handful of tiny JSON files is cheaper than
 * that machinery and it also covers rename, delete, and editors that write via
 * a temporary file. `focus` makes returning from an editor feel immediate
 * instead of waiting for the next tick.
 */
export function startThemePaletteSync(pollMs = 2000): () => void {
  subscribers += 1;
  if (subscribers === 1) {
    // The first read must also apply the saved theme. A saved custom id is not
    // in the built-in catalogue until this read finishes, and applying early
    // would overwrite it with the default. Later polls only re-apply when the
    // catalogue actually changes (inside `setPaletteThemes`).
    void refreshPaletteThemes().finally(() => {
      if (typeof document !== "undefined") applySavedTheme();
    });
    timer = setInterval(() => void refreshPaletteThemes(), pollMs);
    focusListener = () => void refreshPaletteThemes();
    if (typeof window !== "undefined") {
      window.addEventListener("focus", focusListener);
    }
  }
  let stopped = false;
  return () => {
    if (stopped) return;
    stopped = true;
    subscribers = Math.max(0, subscribers - 1);
    if (subscribers === 0) {
      if (timer) clearInterval(timer);
      timer = null;
      if (focusListener && typeof window !== "undefined") {
        window.removeEventListener("focus", focusListener);
      }
      focusListener = null;
    }
  };
}
