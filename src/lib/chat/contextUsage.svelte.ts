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
}

class ContextUsageStore {
  byThread = $state<Record<string, ContextUsage>>({});

  ingest(payload: any, now = Date.now()): void {
    const threadId = String(payload?.thread_id || "");
    const used = Number(payload?.used_tokens);
    const window = Number(payload?.window_tokens);
    if (!threadId || !Number.isFinite(used) || !Number.isFinite(window) || window <= 0) return;
    this.byThread = {
      ...this.byThread,
      [threadId]: {
        botId: String(payload?.bot_id || ""),
        usedTokens: Math.max(0, used),
        windowTokens: window,
        compacted: payload?.compacted === true,
        at: now,
      },
    };
  }

  forThread(threadId: string | null | undefined): ContextUsage | undefined {
    return threadId ? this.byThread[threadId] : undefined;
  }
}

export const contextUsage = new ContextUsageStore();
