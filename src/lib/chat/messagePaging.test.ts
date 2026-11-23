import { describe, expect, it } from "vitest";
import { mergeOlderMessages, oldestMessageId } from "./messagePaging";

describe("message paging", () => {
  it("prepends older messages and drops duplicates", () => {
    const existing = [
      { id: "3", text: "three" },
      { id: "4", text: "four" },
    ];
    const older = [
      { id: "1", text: "one" },
      { id: "2", text: "two" },
      { id: "3", text: "three" },
    ];
    expect(mergeOlderMessages(existing, older).map((message) => message.id)).toEqual([
      "1",
      "2",
      "3",
      "4",
    ]);
  });

  it("returns no cursor for an empty transcript", () => {
    expect(oldestMessageId([])).toBeNull();
    expect(oldestMessageId([{ id: "a" }, { id: "b" }])).toBe("a");
  });
});
