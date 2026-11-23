// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { drafts, officeDraftKey, threadDraftKey } from "./drafts";

afterEach(() => {
  localStorage.clear();
});

describe("composer drafts", () => {
  it("keeps conversations separate", () => {
    drafts.saveNow(threadDraftKey("bot-1", "thread-a"), "alpha");
    drafts.saveNow(threadDraftKey("bot-1", "thread-b"), "beta");
    drafts.saveNow(officeDraftKey("office-1"), "gamma");

    expect(drafts.load(threadDraftKey("bot-1", "thread-a"))).toBe("alpha");
    expect(drafts.load(threadDraftKey("bot-1", "thread-b"))).toBe("beta");
    expect(drafts.load(officeDraftKey("office-1"))).toBe("gamma");
    expect(drafts.load(threadDraftKey("bot-2", "thread-a"))).toBe("");
  });

  it("removes an empty draft instead of storing whitespace", () => {
    const key = threadDraftKey("bot-1", null);
    drafts.saveNow(key, "draft");
    drafts.saveNow(key, "   ");
    expect(drafts.load(key)).toBe("");
  });

  it("clears a draft", () => {
    const key = officeDraftKey("office-1");
    drafts.saveNow(key, "queued thought");
    drafts.clear(key);
    expect(drafts.load(key)).toBe("");
  });
});
