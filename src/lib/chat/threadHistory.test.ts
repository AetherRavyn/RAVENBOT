import { describe, expect, it } from "vitest";
import {
  isPlaceholderThreadTitle,
  readSelectedThread,
  rememberSelectedThread,
  selectedThreadKey,
  threadTitleFromMessage,
} from "./threadHistory";

function memoryStorage() {
  const values = new Map<string, string>();
  return {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => values.set(key, value),
    removeItem: (key: string) => values.delete(key),
  };
}

describe("thread history helpers", () => {
  it("builds a stable per-bot selection key", () => {
    expect(selectedThreadKey("bot-1")).toBe("ravenbot:selected-thread:bot-1");
    expect(selectedThreadKey("bot-1")).not.toBe(selectedThreadKey("bot-2"));
  });

  it("remembers and restores the last thread for a bot", () => {
    const storage = memoryStorage();
    expect(readSelectedThread("bot-1", storage)).toBeNull();

    rememberSelectedThread("bot-1", "thread-7", storage);
    expect(readSelectedThread("bot-1", storage)).toBe("thread-7");

    rememberSelectedThread("bot-1", null, storage);
    expect(readSelectedThread("bot-1", storage)).toBeNull();
  });

  it("does not throw when storage is unavailable or full", () => {
    const broken = {
      getItem: () => {
        throw new Error("denied");
      },
      setItem: () => {
        throw new Error("quota");
      },
      removeItem: () => {
        throw new Error("denied");
      },
    };
    expect(readSelectedThread("bot-1", broken)).toBeNull();
    expect(() => rememberSelectedThread("bot-1", "thread-1", broken)).not.toThrow();
    expect(readSelectedThread(null, broken)).toBeNull();
  });

  it("recognizes generated placeholder titles", () => {
    expect(isPlaceholderThreadTitle("Thread #1")).toBe(true);
    expect(isPlaceholderThreadTitle("thread #42")).toBe(true);
    expect(isPlaceholderThreadTitle("New Thread")).toBe(true);
    expect(isPlaceholderThreadTitle("  ")).toBe(true);
    expect(isPlaceholderThreadTitle("Fix the failing memory test")).toBe(false);
  });

  it("turns the first message into a readable title", () => {
    expect(threadTitleFromMessage("[DeepSearch] When is the next release?")).toBe(
      "When is the next release?",
    );
    expect(threadTitleFromMessage("[Think] [DeepSearch]  Summarize   this repo")).toBe(
      "Summarize this repo",
    );
    expect(threadTitleFromMessage("   ")).toBe("");
  });

  it("truncates long titles on a word boundary", () => {
    const title = threadTitleFromMessage(
      "Please read this entire repository and explain every architectural decision in detail",
      40,
    );
    expect(title.endsWith("…")).toBe(true);
    expect(Array.from(title).length).toBeLessThanOrEqual(41);
    expect(title).not.toContain("  ");
  });
});
