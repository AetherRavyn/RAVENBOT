// @vitest-environment jsdom
import { describe, it, expect, beforeEach } from "vitest";
import { render, cleanup } from "@testing-library/svelte";
import ContextMeter from "./ContextMeter.svelte";
import { contextUsage, type ContextUsage } from "$lib/chat/contextUsage.svelte";
import { setLocale } from "$lib/i18n";

beforeEach(() => {
  cleanup();
  setLocale("en");
});

const usage = (usedTokens: number, windowTokens = 100): ContextUsage => ({
  botId: "bot",
  usedTokens,
  windowTokens,
  compacted: false,
  at: 0,
});

describe("ContextMeter", () => {
  it("renders the percentage and level", () => {
    const { container } = render(ContextMeter, { props: { usage: usage(42) } });
    const meter = container.querySelector(".context-meter");
    expect(meter?.textContent).toContain("42%");
    expect(meter?.getAttribute("data-level")).toBe("normal");
  });

  it("warns near the window limit and turns danger above it", async () => {
    const warn = render(ContextMeter, { props: { usage: usage(85) } });
    expect(warn.container.querySelector(".context-meter")?.getAttribute("data-level")).toBe("warning");
    warn.unmount();

    const danger = render(ContextMeter, { props: { usage: usage(96) } });
    expect(danger.container.querySelector(".context-meter")?.getAttribute("data-level")).toBe("danger");
  });

  it("renders nothing before the first context event", () => {
    const { container } = render(ContextMeter, { props: { usage: null, threadId: "missing" } });
    expect(container.querySelector(".context-meter")).toBeNull();
  });

  it("shows the estimated breakdown and compaction delta in the title", () => {
    const detailed = render(ContextMeter, {
      props: {
        usage: {
          ...usage(42),
          compacted: true,
          beforeTokens: 80,
          systemTokens: 10,
          toolTokens: 5,
          memoryTokens: 7,
          messageTokens: 20,
        },
      },
    });
    const title = detailed.container.querySelector(".context-meter")?.getAttribute("title") || "";
    expect(title).toContain("System 10");
    expect(title).toContain("Tools 5");
    expect(title).toContain("Memory 7");
    expect(title).toContain("Messages 20");
    expect(title).toContain("Compacted 80 → 42 tokens");
    const details = detailed.container.querySelector(".context-meter-details")?.textContent || "";
    expect(details).toContain("System 10");
    expect(details).toContain("Compacted 80 → 42 tokens");
  });

  it("maps a live context event into the same breakdown", () => {
    contextUsage.ingest({
      thread_id: "thread-live-breakdown",
      bot_id: "bot",
      used_tokens: 50,
      before_tokens: 90,
      window_tokens: 100,
      compacted: true,
      system_tokens: 12,
      tool_tokens: 6,
      memory_tokens: 8,
      message_tokens: 24,
    });
    const { container } = render(ContextMeter, {
      props: { threadId: "thread-live-breakdown" },
    });
    const title = container.querySelector(".context-meter")?.getAttribute("title") || "";
    expect(title).toContain("System 12");
    expect(title).toContain("Compacted 90 → 50 tokens");
  });

  it("restores the meter from a persisted assistant message", () => {
    contextUsage.hydrate("thread-persisted", [
      { context_used_tokens: 42, context_window_tokens: 100 },
    ]);
    const { container } = render(ContextMeter, { props: { threadId: "thread-persisted" } });
    expect(container.querySelector(".context-meter")?.textContent).toContain("42%");
  });

  it("keeps a newer live estimate over an older persisted snapshot", () => {
    contextUsage.ingest({
      thread_id: "thread-newer-live",
      bot_id: "bot",
      used_tokens: 60,
      window_tokens: 100,
    });
    contextUsage.hydrate("thread-newer-live", [
      {
        context_used_tokens: 42,
        context_window_tokens: 100,
        created_at: "2020-01-01T00:00:00Z",
      },
    ]);
    const { container } = render(ContextMeter, { props: { threadId: "thread-newer-live" } });
    expect(container.querySelector(".context-meter")?.textContent).toContain("60%");
  });
});
