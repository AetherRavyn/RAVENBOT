import { describe, expect, it } from "vitest";
import {
  islandAgentName,
  islandDotTone,
  islandMood,
  latestIslandItem,
  type IslandAttentionItem,
} from "./attentionIsland";

function item(overrides: Partial<IslandAttentionItem> = {}): IslandAttentionItem {
  return {
    kind: "approval",
    label: "Run a command",
    detail: "shell_exec",
    thread_id: "thread-1",
    bot_id: "bot-1",
    bot_name: "Nova",
    avatar_style: "raven-native",
    state: "pending",
    risk: "low",
    created_at: "2026-01-01T10:00:00Z",
    ...overrides,
  };
}

describe("attention island mapping", () => {
  it("picks the newest item by timestamp", () => {
    const older = item({ created_at: "2026-01-01T10:00:00Z", label: "older" });
    const newer = item({ created_at: "2026-01-01T10:05:00Z", label: "newer" });
    expect(latestIslandItem([older, newer])?.label).toBe("newer");
  });

  it("treats a missing timestamp as the oldest item", () => {
    const dated = item({ created_at: "2026-01-01T10:00:00Z", label: "dated" });
    const undated = item({ created_at: null, label: "undated" });
    expect(latestIslandItem([undated, dated])?.label).toBe("dated");
    expect(latestIslandItem([undated])?.label).toBe("undated");
  });

  it("returns null for an empty island", () => {
    expect(latestIslandItem([])).toBeNull();
    expect(latestIslandItem(null)).toBeNull();
  });

  it("trims the agent name and leaves a queue without one", () => {
    expect(islandAgentName(item({ bot_name: "  Nova  " }))).toBe("Nova");
    expect(islandAgentName(item({ bot_name: null }))).toBe("");
    expect(islandAgentName(null)).toBe("");
  });

  it("maps the item state onto the roster's mood vocabulary", () => {
    expect(islandMood(item({ state: "pending" }))).toBe("waiting");
    expect(islandMood(item({ state: "running" }))).toBe("working");
    expect(islandMood(item({ state: "failed" }))).toBe("failed");
    expect(islandMood(item({ state: null }))).toBe("waiting");
    expect(islandMood(null)).toBe("waiting");
  });

  it("raises the dot for risk and failure before ordinary waiting", () => {
    expect(islandDotTone(item({ risk: "critical" }), 1)).toBe("danger");
    expect(islandDotTone(item({ risk: "high" }), 0)).toBe("danger");
    expect(islandDotTone(item({ state: "failed" }), 1)).toBe("danger");
    expect(islandDotTone(item({ state: "pending" }), 1)).toBe("warning");
    expect(islandDotTone(item({ state: "running" }), 0)).toBe("info");
    expect(islandDotTone(item({ state: "completed" }), 0)).toBe("success");
  });

  it("shows the all-clear without an item", () => {
    expect(islandDotTone(null, 0)).toBe("success");
    expect(islandDotTone(null, 2)).toBe("warning");
  });
});
