/**
 * The small, validated bridge between a sandboxed visual page and a composer.
 *
 * A page may ask the host to put text in the composer or attach a visual
 * context quote. It can never send, and the host never trusts the page's word
 * on length or type. The helpers are pure so the limits and replacement rules
 * are tested without a browser.
 */

export const VISUAL_COMPOSE_EVENT = "ravenbot:visual-compose";
export const VISUAL_CONTEXT_EVENT = "ravenbot:visual-context";

export const VISUAL_COMPOSE_LIMIT = 10_000;
export const VISUAL_CONTEXT_LIMIT = 2_000;

/** Markers are editor-only; they are removed before the message is sent. */
export const VISUAL_CONTEXT_START = "<!-- ravenbot-visual-context:start -->";
export const VISUAL_CONTEXT_END = "<!-- ravenbot-visual-context:end -->";

/**
 * Normalize page text: strings only, trimmed, line endings normalized, and
 * rejected rather than silently truncated when it exceeds the cap.
 */
export function normalizeVisualText(value: unknown, limit: number): string | null {
  if (typeof value !== "string") return null;
  const text = value.replace(/\r\n?/gu, "\n").trim();
  if (!text) return null;
  if (Array.from(text).length > limit) return null;
  return text;
}

/** Append composer text without replacing what the person already typed. */
export function visualComposeDraft(draft: string, text: string): string {
  const current = draft.trimEnd();
  return current ? `${current}\n${text}` : text;
}

/** A blockquote that carries page state, wrapped in replaceable editor markers. */
export function visualContextBlock(text: string, label: string): string {
  const safe = text
    .split(VISUAL_CONTEXT_START)
    .join("")
    .split(VISUAL_CONTEXT_END)
    .join("")
    .trim();
  const quoted = safe
    .split("\n")
    .map((line) => (line.length > 0 ? `> ${line}` : ">"))
    .join("\n");
  return `${VISUAL_CONTEXT_START}\n> **${label}**\n${quoted}\n${VISUAL_CONTEXT_END}`;
}

/** Remove a previous context block, if the draft still has one. */
export function removeVisualContextBlock(draft: string): string {
  const start = draft.indexOf(VISUAL_CONTEXT_START);
  if (start === -1) return draft;
  const end = draft.indexOf(VISUAL_CONTEXT_END, start);
  if (end === -1) return draft;
  const next = `${draft.slice(0, start)}${draft.slice(end + VISUAL_CONTEXT_END.length)}`;
  return next.replace(/\n{3,}/gu, "\n\n").trim();
}

/** Strip the editor markers but keep the quoted context the person can read. */
export function stripVisualContextMarkers(text: string): string {
  return text
    .split(VISUAL_CONTEXT_START)
    .join("")
    .split(VISUAL_CONTEXT_END)
    .join("")
    .replace(/\n{3,}/gu, "\n\n")
    .trim();
}
