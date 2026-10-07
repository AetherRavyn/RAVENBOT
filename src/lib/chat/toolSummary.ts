/**
 * One readable line from a tool's argument bag.
 *
 * Shared by the live action marker and the persisted trace list, because the
 * same call should read the same way while it runs and after a reload — two
 * summaries is two things to keep in sync, and they would not stay in sync.
 *
 * Deliberately not a JSON dump. The line answers "which call was this", and
 * `{"path":"src/app.rs","content":"…"}` is a wall, not an answer. Falls back to
 * raw JSON when no known key is present, so an unfamiliar skill is never
 * summarised as blank.
 */

/** Longest summary, so a pasted document cannot become a header. */
const MAX_SUMMARY = 90;

/** Keys whose values tend to be the identifying part of a call. */
const HINT_KEYS = [
  "path",
  "file_path",
  "filePath",
  "url",
  "query",
  "command",
  "pattern",
  "name",
  "directory",
  "topic",
  "question",
  "instruction",
  "content",
  "description",
];

function clip(s: string, max = MAX_SUMMARY): string {
  const one = s.replace(/\s+/g, " ").trim();
  return one.length > max ? `${one.slice(0, max)}…` : one;
}

export function summarizeArgs(value: unknown): string {
  if (value == null) return "";
  if (typeof value === "string") return clip(value);
  if (typeof value !== "object") return clip(String(value));

  const obj = value as Record<string, unknown>;
  for (const key of HINT_KEYS) {
    const v = obj[key];
    if (typeof v === "string" && v.trim()) return clip(v);
    if (typeof v === "number" || typeof v === "boolean") return `${key}=${v}`;
  }
  // Prefer the first short string among *any* keys before giving up: a call
  // like `{ "target_branch": "main" }` has no hint key but is perfectly
  // summarisable, and showing nothing would be worse than showing that.
  const firstString = Object.values(obj).find((v) => typeof v === "string" && (v as string).trim());
  if (firstString) return clip(firstString as string);
  try {
    return clip(JSON.stringify(value));
  } catch {
    // A cyclic or throwing toJSON is not a reason to lose the line entirely.
    return "";
  }
}

/**
 * How long something took, or has been running.
 *
 * `min` and `s` are SI symbols, not words, so they need no translation — and a
 * number ticking once a second is not a string anyone should have to re-read
 * through a locale layer.
 *
 * Sub-second calls render as nothing. `320ms` next to a tool that just flashed
 * past is noise, and a live counter that changes four times a second draws the
 * eye to the least interesting thing on screen.
 */
export function fmtElapsed(ms: number | null | undefined): string {
  if (ms == null || ms < 1000) return "";
  const total = Math.floor(ms / 1000);
  if (total < 60) return `${total}s`;
  const minutes = Math.floor(total / 60);
  const seconds = total % 60;
  return seconds ? `${minutes}m ${seconds}s` : `${minutes}m`;
}

/** Full, pretty-printed value for an expanded body. */
export function renderToolValue(value: unknown): string {
  if (value == null || value === "null") return "";
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}
