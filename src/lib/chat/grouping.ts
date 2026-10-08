// OpenBot-style consecutive-author grouping: show the avatar/name header only
// on the FIRST message of a run by the same author; continuation rows get a
// gutter spacer so bubbles stay aligned.
export interface Groupable {
  role?: string;
  sender_bot_id?: string | null;
  sender_name?: string | null;
}

export function authorKey(m: Groupable): string {
  return m.sender_bot_id || m.sender_name || m.role || "unknown";
}

/** True when this row should paint its author header (new run begins here). */
export function showAuthorHeader(messages: Groupable[], index: number): boolean {
  if (index <= 0) return true;
  return authorKey(messages[index - 1]) !== authorKey(messages[index]);
}

/** Seeded hue per author so agent bubbles/marks carry a stable identity color. */
export function authorHue(key: string): number {
  let h = 0;
  for (let i = 0; i < key.length; i++) h = (h * 31 + key.charCodeAt(i)) >>> 0;
  return h % 360;
}

/** OpenBot ghost variant: replies that are only a code block or only a table
 * render edge-to-edge with no bubble chrome. */
export function isGhostContent(text: string): boolean {
  const t = (text || "").trim();
  return /^```[\s\S]*```\s*$/.test(t) || /^\|.+\n\|[-| :]+\n/.test(t);
}
