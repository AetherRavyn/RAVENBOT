/** Cursor paging for a thread transcript. */

export const MESSAGE_PAGE_SIZE = 50;

export interface MessagePage<T> {
  messages: T[];
  has_more: boolean;
}

/** Prepend an older page without duplicating rows already loaded. */
export function mergeOlderMessages<T extends { id: string }>(
  existing: T[],
  older: T[],
): T[] {
  const seen = new Set(existing.map((message) => String(message.id)));
  const fresh = older.filter((message) => !seen.has(String(message.id)));
  return [...fresh, ...existing];
}

/** The cursor for the next older page, or null when nothing is loaded. */
export function oldestMessageId<T extends { id: string }>(messages: T[]): string | null {
  return messages.length > 0 ? String(messages[0].id) : null;
}
