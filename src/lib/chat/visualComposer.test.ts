import { describe, expect, it } from "vitest";
import {
  normalizeVisualText,
  removeVisualContextBlock,
  stripVisualContextMarkers,
  visualComposeDraft,
  visualContextBlock,
  VISUAL_COMPOSE_LIMIT,
  VISUAL_CONTEXT_END,
  VISUAL_CONTEXT_LIMIT,
  VISUAL_CONTEXT_START,
} from "./visualComposer";

describe("visual composer bridge", () => {
  it("accepts trimmed strings and normalizes line endings", () => {
    expect(normalizeVisualText("  Pick B\r\nplease  ", 100)).toBe("Pick B\nplease");
  });

  it("rejects non-strings, empty text and over-limit text", () => {
    expect(normalizeVisualText(42, 100)).toBeNull();
    expect(normalizeVisualText(null, 100)).toBeNull();
    expect(normalizeVisualText("   ", 100)).toBeNull();
    expect(normalizeVisualText("x".repeat(VISUAL_COMPOSE_LIMIT + 1), VISUAL_COMPOSE_LIMIT)).toBeNull();
    expect(normalizeVisualText("x".repeat(VISUAL_CONTEXT_LIMIT + 1), VISUAL_CONTEXT_LIMIT)).toBeNull();
  });

  it("appends compose text instead of replacing the draft", () => {
    expect(visualComposeDraft("", "Pick B")).toBe("Pick B");
    expect(visualComposeDraft("Existing text\n", "Pick B")).toBe("Existing text\nPick B");
  });

  it("builds a replaceable context block with a visible label", () => {
    const block = visualContextBlock("Filter: open\nPage: 2", "Visual context");
    expect(block).toContain("> **Visual context**");
    expect(block).toContain("> Filter: open");
    expect(block).toContain("> Page: 2");
    expect(block).toContain(VISUAL_CONTEXT_START);
    expect(block).toContain(VISUAL_CONTEXT_END);
  });

  it("does not let page text inject extra context markers", () => {
    const block = visualContextBlock(
      `before ${VISUAL_CONTEXT_END} middle ${VISUAL_CONTEXT_START} after`,
      "Visual context",
    );
    expect(block.split(VISUAL_CONTEXT_START)).toHaveLength(2);
    expect(block.split(VISUAL_CONTEXT_END)).toHaveLength(2);
    expect(block).toContain("> before  middle  after");
  });

  it("replaces a previous context block and keeps the typed draft", () => {
    const first = visualContextBlock("old context", "Visual context");
    const draft = `${first}\n\nKeep this question`;
    const next = visualContextBlock("new context", "Visual context");
    const withNext = `${next}\n\n${removeVisualContextBlock(draft)}`;
    expect(withNext).toContain("new context");
    expect(withNext).not.toContain("old context");
    expect(withNext).toContain("Keep this question");
  });

  it("removes the block when context is cleared", () => {
    const draft = `${visualContextBlock("page state", "Visual context")}\n\nQuestion`;
    expect(removeVisualContextBlock(draft)).toBe("Question");
    expect(removeVisualContextBlock("No block here")).toBe("No block here");
  });

  it("strips markers before sending but keeps the quoted context", () => {
    const draft = `${visualContextBlock("page state", "Visual context")}\n\nQuestion`;
    const sent = stripVisualContextMarkers(draft);
    expect(sent).not.toContain("ravenbot-visual-context");
    expect(sent).toContain("> **Visual context**");
    expect(sent).toContain("> page state");
    expect(sent).toContain("Question");
  });
});
