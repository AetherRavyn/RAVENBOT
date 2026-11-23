import { untrack } from "svelte";
import { persistedContextUsage, type ContextMessageLike } from "./contextPersistence";

/**
 * Context-window usage per thread.
 *
 * The runtime emits a `context` event after compaction on every model round.
 * The composer reads the newest value for its thread; keeping this outside the
 * view means switching chats, scrolling, or reopening the room does not lose
 * the meter. A value is an estimate, never a provider billing figure.
 */
export interface ContextUsage {
  botId: string;
  usedTokens: number;
  windowTokens: number;
  compacted: boolean;
  at: number;
  /** Estimated prompt tokens before the last compaction, when live. */
  beforeTokens?: number;
  /** Disjoint estimated buckets, when the runtime supplied them. */
  systemTokens?: number;
  toolTokens?: number;
  memoryTokens?: number;
  messageTokens?: number;
}

class ContextUsageStore {
  byThread = $state<Record<string, ContextUsage>>({});

  ingest(payload: any, now = Date.now()): void {
    const threadId = String(payload?.thread_id || "");
    const used = Number(payload?.used_tokens);
    const window = Number(payload?.window_tokens);
    if (!threadId || !Number.isFinite(used) || !Number.isFinite(window) || window <= 0) return;
    const optional = (value: unknown): number | undefined => {
      const parsed = Number(value);
      return Number.isFinite(parsed) ? Math.max(0, parsed) : undefined;
    };
    this.byThread = {
      ...untrack(() => this.byThread),
      [threadId]: {
        botId: String(payload?.bot_id || ""),
        usedTokens: Math.max(0, used),
        windowTokens: window,
        compacted: payload?.compacted === true,
        at: now,
        beforeTokens: optional(payload?.before_tokens),
        systemTokens: optional(payload?.system_tokens),
        toolTokens: optional(payload?.tool_tokens),
        memoryTokens: optional(payload?.memory_tokens),
        messageTokens: optional(payload?.message_tokens),
      },
    };
  }

  /**
   * Seed the meter from the newest persisted assistant message.
   *
   * Called when a transcript loads. A later live `context` event still wins,
   * and the final assistant message writes the value that survives the next
   * reload.
   */
  hydrate(threadId: string, messages: readonly ContextMessageLike[]): void {
    const usage = persistedContextUsage(messages);
    if (!usage) return;
    // The merge reads the store it is about to write. A caller effect would
    // otherwise depend on `byThread`, so its own write would re-trigger it
    // forever. The read is an implementation detail, not a dependency of the
    // transcript effect.
    const previous = untrack(() => this.byThread[threadId]);
    // A transcript load can race the run that just finished. The persisted
    // message is a snapshot from when it was written; a live `context` event
    // that arrived after it is newer and must not be replaced by the older
    // estimate, or the meter visibly jumps backwards.
    if (previous && previous.at > usage.at) return;
    this.byThread = {
      ...untrack(() => this.byThread),
      [threadId]: usage,
    };
  }

  forThread(threadId: string | null | undefined): ContextUsage | undefined {
    return threadId ? this.byThread[threadId] : undefined;
  }
}

export const contextUsage = new ContextUsageStore();
