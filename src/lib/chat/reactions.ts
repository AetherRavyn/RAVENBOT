/**
 * Message reaction helpers.
 *
 * A reaction is a toggle on a single emoji, not a counter. The local-first app
 * has one human at the keyboard, so `["👍"]` means "reacted", and the database
 * primary key `(message_id, emoji)` is what guarantees there is never more than
 * one of the same reaction on a message.
 */

/** The palette offered by the add-reaction button, in display order. */
export const REACTION_PALETTE = ["👍", "❤️", "🎉", "👀", "✅", "🤔"] as const;

export type ReactionPaletteEntry = (typeof REACTION_PALETTE)[number];

/**
 * Add the emoji if absent, remove it if present. Order is preserved so a
 * reaction chip does not jump to the end when the same toggle is repeated.
 */
export function toggleReaction(current: readonly string[], emoji: string): string[] {
  const value = emoji.trim();
  if (!value) return [...current];
  return current.includes(value)
    ? current.filter((entry) => entry !== value)
    : [...current, value];
}

/**
 * Group rows returned by the database into `messageId -> emoji[]`.
 *
 * Kept in insertion order: the query orders by `created_at, rowid`, so chips
 * appear in the order the human placed them rather than in palette order.
 */
export function groupReactions(
  rows: readonly { message_id: string; emoji: string }[],
): Record<string, string[]> {
  const grouped: Record<string, string[]> = {};
  for (const row of rows) {
    if (!row?.message_id || !row.emoji) continue;
    (grouped[row.message_id] ??= []).push(row.emoji);
  }
  return grouped;
}

/**
 * Whether a message id can be persisted.
 *
 * Optimistic user rows use `temp-…` ids until the backend returns the real
 * UUID. Showing the reaction control on one of those invites a write that
 * cannot succeed; hiding it is honest about what is saved.
 */
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export function isPersistedMessageId(value: unknown): boolean {
  return typeof value === "string" && UUID.test(value);
}
