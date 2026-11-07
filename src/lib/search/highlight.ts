/** Case-insensitive search helpers shared by the palette and settings search. */

export interface HighlightPart {
  text: string;
  match: boolean;
}

export function normalizedSearch(value: string): string {
  return value.trim().replace(/\s+/g, " ").toLocaleLowerCase();
}

/** True when any part contains the query, ignoring case. */
export function matchesQuery(parts: readonly (string | null | undefined)[], query: string): boolean {
  const needle = normalizedSearch(query);
  if (!needle) return true;
  return parts.some((part) => (part ?? "").toLocaleLowerCase().includes(needle));
}

/** Split text into runs so the matching part can be rendered with emphasis. */
export function highlightParts(text: string, query: string): HighlightPart[] {
  const needle = normalizedSearch(query);
  if (!needle) return [{ text, match: false }];
  const lowered = text.toLocaleLowerCase();
  const parts: HighlightPart[] = [];
  let cursor = 0;
  let index = lowered.indexOf(needle);
  while (index >= 0) {
    if (index > cursor) parts.push({ text: text.slice(cursor, index), match: false });
    parts.push({ text: text.slice(index, index + needle.length), match: true });
    cursor = index + needle.length;
    index = lowered.indexOf(needle, cursor);
  }
  if (cursor < text.length) parts.push({ text: text.slice(cursor), match: false });
  return parts;
}
