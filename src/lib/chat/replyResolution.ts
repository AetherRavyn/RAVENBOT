/** Cross-page resolution of reply targets. */

export interface ReplyTarget {
  id: string;
  reply_to_id?: string | null;
}

/**
 * Reply ids that are not in the loaded transcript and are not already being
 * fetched. Deduplicated so one page cannot request the same target twice.
 */
export function replyTargetsToLoad(
  messages: readonly ReplyTarget[],
  knownIds: ReadonlySet<string>,
  pendingIds: ReadonlySet<string>,
): string[] {
  const out = new Set<string>();
  for (const message of messages) {
    const id = message.reply_to_id ? String(message.reply_to_id) : "";
    if (!id || knownIds.has(id) || pendingIds.has(id)) continue;
    out.add(id);
  }
  return [...out];
}

/** Merge fetched reply targets without duplicating an existing row. */
export function mergeReplyTargets<T extends { id: string }>(
  existing: readonly T[],
  fetched: readonly T[],
): T[] {
  const seen = new Set(existing.map((message) => String(message.id)));
  return [
    ...existing,
    ...fetched.filter((message) => !seen.has(String(message.id))),
  ];
}
