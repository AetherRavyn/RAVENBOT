import { describe, expect, it } from "vitest";
import { applyMention, filterMentionItems, mentionAtCaret } from "./mentions";

describe("composer mentions", () => {
  it("reads the token before the caret only at a word boundary", () => {
    expect(mentionAtCaret("ask @sam", 8)).toEqual({
      sigil: "@",
      query: "sam",
      start: 4,
      end: 8,
    });
    expect(mentionAtCaret("mail me@sam", 10)).toBeNull();
    expect(mentionAtCaret("read #src/ma", 12)).toMatchObject({
      sigil: "#",
      query: "src/ma",
    });
  });

  it("replaces the token and leaves a trailing space", () => {
    const token = mentionAtCaret("ask @sa", 7)!;
    expect(applyMention("ask @sa now", token, "Sam")).toEqual({
      text: "ask @Sam now",
      caret: 8,
    });
  });

  it("filters on label and detail, with a bounded list", () => {
    const items = [
      { id: "a", label: "Alice", detail: "Design", kind: "agent" as const },
      { id: "b", label: "Bob", detail: "Backend", kind: "agent" as const },
      { id: "c", label: "Carol", detail: "Alice's reviewer", kind: "agent" as const },
    ];
    expect(filterMentionItems(items, "ali").map((item) => item.id)).toEqual(["a", "c"]);
    expect(filterMentionItems(items, "", 2)).toHaveLength(2);
  });
});
