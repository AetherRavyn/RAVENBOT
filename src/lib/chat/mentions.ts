/** Composer mention tokens: `@agent`, `#file`, `$skill`. */

export type MentionSigil = "@" | "#" | "$";

export interface MentionToken {
  sigil: MentionSigil;
  query: string;
  /** Index of the sigil. */
  start: number;
  /** Caret at the time the token was read. */
  end: number;
}

export interface MentionItem {
  id: string;
  label: string;
  detail?: string;
  kind: "agent" | "file" | "skill";
}

/**
 * Read the mention token immediately before the caret.
 *
 * A token starts at the beginning of the message or after whitespace. That
 * keeps an email address or a `C#` in prose from opening a picker; the user
 * still types the marker deliberately.
 */
export function mentionAtCaret(text: string, caret: number): MentionToken | null {
  const before = text.slice(0, Math.max(0, Math.min(caret, text.length)));
  const match = before.match(/(^|\s)([@#$])([^\s@#$]*)$/u);
  if (!match) return null;
  const sigil = match[2] as MentionSigil;
  const query = match[3] ?? "";
  const start = before.length - query.length - 1;
  return { sigil, query, start, end: caret };
}

/** Replace the token with a picked mention and return the new caret. */
export function applyMention(
  text: string,
  token: MentionToken,
  label: string,
): { text: string; caret: number } {
  const needsSpace = token.end >= text.length || !/\s/u.test(text[token.end]);
  const insert = `${token.sigil}${label}${needsSpace ? " " : ""}`;
  const next = text.slice(0, token.start) + insert + text.slice(token.end);
  return { text: next, caret: token.start + insert.length };
}

/** Case-insensitive prefix/contains filter, keeping the caller's order. */
export function filterMentionItems(items: MentionItem[], query: string, limit = 8): MentionItem[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return items.slice(0, limit);
  return items
    .filter((item) => {
      const haystack = `${item.label} ${item.detail ?? ""}`.toLowerCase();
      return haystack.includes(needle);
    })
    .slice(0, limit);
}
