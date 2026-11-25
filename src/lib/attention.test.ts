import { describe, expect, it } from "vitest";
import {
  awaitingBots,
  awaitingCount,
  relativeAttentionTime,
  type AttentionItem,
} from "./attention.svelte";

function item(
  kind: AttentionItem["kind"],
  bot_id: string | null,
  id: string = kind,
): AttentionItem {
  return {
    id,
    kind,
    bot_id,
    bot_name: bot_id ?? "",
    thread_id: null,
    office_id: null,
    office_name: null,
    title: id,
    detail: "",
    risk: null,
    state: "pending",
    question_kind: null,
    created_at: "2026-11-09T12:00:00.000Z",
  };
}

describe("awaiting gates", () => {
  it("counts approvals and questions, not queue rows or alerts", () => {
    const items = [
      item("approval", "bot-1", "a1"),
      item("question", "bot-1", "q1"),
      item("queue", "bot-1", "c1"),
      item("alert", "bot-2", "w1"),
    ];
    expect(awaitingCount(items)).toBe(2);
    expect([...awaitingBots(items)]).toEqual(["bot-1"]);
  });

  it("ignores rows that cannot identify a bot", () => {
    expect(awaitingBots([item("approval", null, "a1")]).size).toBe(0);
    expect(awaitingCount([])).toBe(0);
  });
});

describe("relativeAttentionTime", () => {
  const now = Date.parse("2026-11-09T12:00:00.000Z");

  it("reads recent times in seconds and minutes", () => {
    expect(relativeAttentionTime("2026-11-09T11:59:45.000Z", "en", now)).toContain(
      "15 seconds",
    );
    expect(relativeAttentionTime("2026-11-09T11:55:00.000Z", "en", now)).toContain(
      "5 minutes",
    );
  });

  it("reads older times in hours and days", () => {
    expect(relativeAttentionTime("2026-11-09T09:00:00.000Z", "en", now)).toContain(
      "3 hours",
    );
    expect(relativeAttentionTime("2026-11-06T12:00:00.000Z", "en", now)).toContain(
      "3 days",
    );
  });

  it("returns nothing for an unparseable timestamp", () => {
    expect(relativeAttentionTime("not-a-date", "en", now)).toBe("");
  });
});
