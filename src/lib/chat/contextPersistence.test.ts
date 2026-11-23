import { describe, expect, it } from "vitest";
import { persistedContextUsage } from "./contextPersistence";

describe("persistedContextUsage", () => {
  it("returns null until a message carries both values", () => {
    expect(persistedContextUsage([])).toBeNull();
    expect(
      persistedContextUsage([
        { context_used_tokens: 10 },
        { context_window_tokens: 1000 },
      ]),
    ).toBeNull();
  });

  it("uses the newest assistant message that has a valid estimate", () => {
    const usage = persistedContextUsage([
      {
        sender_bot_id: "old-bot",
        context_used_tokens: 800,
        context_window_tokens: 1000,
        created_at: "2026-01-01T00:00:00Z",
      },
      { context_used_tokens: 20, context_window_tokens: 0 },
      {
        sender_bot_id: "new-bot",
        context_used_tokens: 640,
        context_window_tokens: 8192,
        context_compacted: true,
        context_system_tokens: 100,
        context_tool_tokens: 20,
        context_memory_tokens: 30,
        context_message_tokens: 490,
        created_at: "2026-01-02T00:00:00Z",
      },
    ]);

    expect(usage).toEqual({
      botId: "new-bot",
      usedTokens: 640,
      windowTokens: 8192,
      compacted: true,
      at: Date.parse("2026-01-02T00:00:00Z"),
      systemTokens: 100,
      toolTokens: 20,
      memoryTokens: 30,
      messageTokens: 490,
    });
  });

  it("clamps a negative estimate and tolerates a missing timestamp", () => {
    const usage = persistedContextUsage([
      { context_used_tokens: -5, context_window_tokens: 100 },
    ]);
    expect(usage?.usedTokens).toBe(0);
    expect(usage?.compacted).toBe(false);
    expect(usage?.at).toBe(0);
  });
});
