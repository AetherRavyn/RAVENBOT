import { describe, expect, it } from "vitest";
import { replyQuoteFor } from "./replies";

const messages = [
  { id: "a", content: "The deploy failed on staging." },
  { id: "b", content: { text: "Retry after the fix." } },
  { id: "c", content: "" },
];

const authorFor = (message: { id?: string | null }) => (message.id === "a" ? "You" : "Scout");
const unavailable = "Original message unavailable";

describe("replyQuoteFor", () => {
  it("resolves the quoted author and a one-line preview", () => {
    expect(replyQuoteFor(messages, "a", authorFor, unavailable)).toEqual({
      id: "a",
      author: "You",
      text: "The deploy failed on staging.",
    });
    expect(replyQuoteFor(messages, "b", authorFor, unavailable)?.text).toBe(
      "Retry after the fix.",
    );
  });

  it("collapses whitespace so the quote stays a small card", () => {
    const quote = replyQuoteFor(
      [{ id: "x", content: "line one\n\n  line two" }],
      "x",
      authorFor,
      unavailable,
    );
    expect(quote?.text).toBe("line one line two");
  });

  it("keeps an attachment-only quote empty rather than inventing text", () => {
    expect(replyQuoteFor(messages, "c", authorFor, unavailable)?.text).toBe("");
  });

  it("marks a deleted original instead of losing the reply relationship", () => {
    expect(replyQuoteFor(messages, "gone", authorFor, unavailable)).toEqual({
      id: "gone",
      author: "",
      text: unavailable,
    });
  });

  it("returns nothing when the row is not a reply", () => {
    expect(replyQuoteFor(messages, null, authorFor, unavailable)).toBeNull();
  });

  it("clips a long quote", () => {
    const quote = replyQuoteFor(
      [{ id: "long", content: "x".repeat(400) }],
      "long",
      authorFor,
      unavailable,
    );
    expect(quote?.text).toHaveLength(180);
    expect(quote?.text.endsWith("…")).toBe(true);
  });
});
