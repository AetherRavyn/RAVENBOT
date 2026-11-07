import { describe, expect, it } from "vitest";
import {
  REACTION_PALETTE,
  groupReactions,
  isPersistedMessageId,
  toggleReaction,
} from "./reactions";
import { matchingMessageIds, messageKey, messageText, stepMatchIndex } from "./search";

describe("reaction toggles", () => {
  it("adds a reaction once and removes it on the second click", () => {
    expect(toggleReaction([], "👍")).toEqual(["👍"]);
    expect(toggleReaction(["👍"], "👍")).toEqual([]);
  });

  it("keeps the order stable while adding different reactions", () => {
    expect(toggleReaction(["👍"], "🎉")).toEqual(["👍", "🎉"]);
    expect(toggleReaction(["👍", "🎉"], "👍")).toEqual(["🎉"]);
  });

  it("ignores an empty palette value", () => {
    expect(toggleReaction(["👍"], "  ")).toEqual(["👍"]);
  });

  it("offers a small, fixed palette", () => {
    expect(REACTION_PALETTE.length).toBeGreaterThanOrEqual(4);
    expect(new Set(REACTION_PALETTE).size).toBe(REACTION_PALETTE.length);
  });

  it("only offers reactions on messages the backend can address", () => {
    expect(isPersistedMessageId("7c9e6679-7425-40de-944b-e07fc1f90ae7")).toBe(true);
    expect(isPersistedMessageId("temp-1710000000-1")).toBe(false);
    expect(isPersistedMessageId("")).toBe(false);
    expect(isPersistedMessageId(null)).toBe(false);
  });
});

describe("groupReactions", () => {
  it("groups rows by message in the order the database returned them", () => {
    expect(
      groupReactions([
        { message_id: "m1", emoji: "👍" },
        { message_id: "m2", emoji: "🎉" },
        { message_id: "m1", emoji: "❤️" },
      ]),
    ).toEqual({ m1: ["👍", "❤️"], m2: ["🎉"] });
  });

  it("drops rows that cannot be placed on a message", () => {
    expect(groupReactions([{ message_id: "", emoji: "👍" }])).toEqual({});
  });
});

describe("in-thread search", () => {
  const messages = [
    { id: "a", content: "The deploy failed on staging." },
    { id: "b", content: { type: "Text", text: "Retry the deploy after the fix." } },
    { id: "c", content: { type: "Checklist", items: [] } },
  ];

  it("reads text from both plain and structured messages", () => {
    expect(messageText(messages[0])).toBe("The deploy failed on staging.");
    expect(messageText(messages[1])).toBe("Retry the deploy after the fix.");
    expect(messageText(messages[2])).toContain('"type":"Checklist"');
  });

  it("matches case-insensitively and ignores surrounding whitespace", () => {
    expect(matchingMessageIds(messages, "  DEPLOY ")).toEqual(["a", "b"]);
  });

  it("does not treat an empty query as match-all", () => {
    expect(matchingMessageIds(messages, "")).toEqual([]);
    expect(matchingMessageIds(messages, "   ")).toEqual([]);
  });

  it("uses the same stable key the row renders with", () => {
    expect(messageKey({ id: "a" })).toBe("a");
    expect(messageKey({ created_at: "2026-01-01" })).toBe("2026-01-01");
  });

  it("wraps navigation at both ends", () => {
    expect(stepMatchIndex(0, 3, 1)).toBe(1);
    expect(stepMatchIndex(2, 3, 1)).toBe(0);
    expect(stepMatchIndex(0, 3, -1)).toBe(2);
    expect(stepMatchIndex(0, 0, 1)).toBe(0);
  });
});
