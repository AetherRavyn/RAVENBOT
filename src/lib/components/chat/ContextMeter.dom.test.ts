// @vitest-environment jsdom
import { describe, it, expect, beforeEach } from "vitest";
import { render, cleanup } from "@testing-library/svelte";
import ContextMeter from "./ContextMeter.svelte";
import type { ContextUsage } from "$lib/chat/contextUsage.svelte";
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
});
