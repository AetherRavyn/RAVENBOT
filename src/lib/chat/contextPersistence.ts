/**
 * Restore context-window usage from a persisted assistant message.
 *
 * The runtime emits a live `context` event while a run is executing. That event
 * is gone when the window closes, so the assistant message also carries the
 * estimate it was produced with. Reading the newest one keeps the composer
 * meter meaningful after a reload, before the next run emits anything.
 */
export interface PersistedContextUsage {
  botId: string;
  usedTokens: number;
  windowTokens: number;
  compacted: boolean;
  at: number;
  systemTokens?: number;
  toolTokens?: number;
  memoryTokens?: number;
  messageTokens?: number;
}

export interface ContextMessageLike {
  thread_id?: string;
  sender_bot_id?: string | null;
  context_used_tokens?: number | null;
  context_window_tokens?: number | null;
  context_compacted?: boolean | null;
  context_system_tokens?: number | null;
  context_tool_tokens?: number | null;
  context_memory_tokens?: number | null;
  context_message_tokens?: number | null;
  created_at?: string | null;
}

export function persistedContextUsage(
  messages: readonly ContextMessageLike[],
): PersistedContextUsage | null {
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    const used = message?.context_used_tokens;
    const window = message?.context_window_tokens;
    if (typeof used !== "number" || !Number.isFinite(used)) continue;
    if (typeof window !== "number" || !Number.isFinite(window) || window <= 0) continue;
    const parsed = message.created_at ? Date.parse(message.created_at) : Number.NaN;
    const bucket = (value: number | null | undefined) =>
      typeof value === "number" && Number.isFinite(value) ? Math.max(0, value) : undefined;
    return {
      botId: String(message.sender_bot_id || ""),
      usedTokens: Math.max(0, used),
      windowTokens: window,
      compacted: message.context_compacted === true,
      at: Number.isFinite(parsed) ? parsed : 0,
      systemTokens: bucket(message.context_system_tokens),
      toolTokens: bucket(message.context_tool_tokens),
      memoryTokens: bucket(message.context_memory_tokens),
      messageTokens: bucket(message.context_message_tokens),
    };
  }
  return null;
}
