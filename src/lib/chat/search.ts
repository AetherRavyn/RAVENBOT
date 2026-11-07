/**
 * In-thread message search.
 *
 * The search runs over the same text the row renders, not over the raw JSON
 * envelope. A message whose content is structured still has prose in `text`,
 * and a tool result is searchable by its serialized payload — but the query
 * itself never has to know which shape a message arrived in.
 */

export interface SearchableMessage {
  id?: string | null;
  created_at?: string | null;
  content?: unknown;
}

/** The plain text a message contributes to search. */
export function messageText(message: SearchableMessage): string {
  const content = message?.content;
  if (typeof content === "string") return content;
  if (content && typeof content === "object") {
    const text = (content as { text?: unknown }).text;
    if (typeof text === "string") return text;
    try {
      return JSON.stringify(content);
    } catch {
      return "";
    }
  }
  return "";
}

/** A stable key for a message, matching the key the chat list renders with. */
export function messageKey(message: SearchableMessage): string {
  return String(message?.id || message?.created_at || "");
}

/**
 * Ids of messages containing `query`, in conversation order.
 *
 * Case-insensitive, trims the query, and returns nothing for an empty query
 * rather than every message — an empty search box is not "match all".
 */
export function matchingMessageIds(
  messages: readonly SearchableMessage[],
  query: string,
): string[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return [];
  return messages
    .filter((message) => messageText(message).toLowerCase().includes(needle))
    .map(messageKey)
    .filter(Boolean);
}

/** Wrap around at either end, so Next from the last hit returns to the first. */
export function stepMatchIndex(current: number, total: number, delta: number): number {
  if (total <= 0) return 0;
  const next = (current + delta) % total;
  return next < 0 ? next + total : next;
}
