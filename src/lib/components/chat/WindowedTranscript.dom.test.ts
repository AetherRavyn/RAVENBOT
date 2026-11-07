// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import WindowedFeedFixture from "./WindowedFeedFixture.svelte";
import { setLocale } from "$lib/i18n";

/**
 * The office-open regression gate.
 *
 * The office used to mount every message in its thread. The real room had 216
 * messages and produced ~20,325 DOM nodes, which was enough synchronous render
 * work to freeze the window. The page now mounts a bounded tail, so this test
 * asserts the two things that made the difference:
 *
 *  1. only the window's worth of rows is mounted, regardless of history size;
 *  2. the resulting DOM stays under a ceiling that a full transcript blew past.
 *
 * Timing is printed and bounded by a generous ceiling. It is a catastrophic
 * regression signal, not a benchmark: CI machines vary too much for a tight
 * millisecond assertion, while the row/node counts are exact.
 */

const CODE = `export function reconcile(lines) {
  return lines.reduce((total, line) => total + line.amount, 0);
}`;

function transcript(count: number) {
  return Array.from({ length: count }, (_, index) => {
    const isUser = index % 5 === 0;
    return {
      id: `msg-${index}`,
      role: isUser ? "user" : "assistant",
      text: isUser
        ? `Checkpoint ${index}: keep the checkout invariant intact.`
        : `### Status ${index}\n\nThe billing pass is still blocked on the missing transaction seam.\n\n\`\`\`ts\n${CODE}\n\`\`\`\n\n- Verify \`sum(lines) - discounts + tax === total\`\n- Verify \`sum(payments) === total\``,
    };
  });
}

describe("windowed office transcript", () => {
  beforeEach(() => {
    cleanup();
    setLocale("en");
    globalThis.ResizeObserver = class {
      observe() {}
      unobserve() {}
      disconnect() {}
    } as unknown as typeof ResizeObserver;
    window.matchMedia = ((query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    })) as unknown as typeof window.matchMedia;
    Element.prototype.animate = vi.fn(() => ({
      cancel: () => {},
      finished: Promise.resolve(),
    })) as unknown as typeof Element.prototype.animate;
  });
  afterEach(cleanup);

  it("mounts only the latest 30 rows of a 240-message office", () => {
    const messages = transcript(240);
    const started = performance.now();
    const { container } = render(WindowedFeedFixture, {
      props: { messages },
    });
    const elapsed = performance.now() - started;
    const rows = container.querySelectorAll(".message-entry").length;
    const nodes = container.querySelectorAll("*").length;
    console.log(
      `OFFICE_WINDOWED_RENDER_MS=${elapsed.toFixed(0)} ROWS=${rows} NODES=${nodes} TOTAL=${messages.length}`,
    );

    expect(rows).toBe(30);
    expect(nodes).toBeLessThan(9_000);
    expect(elapsed).toBeLessThan(3_000);
  });

  it("mounts 50 rows for a one-to-one thread window", () => {
    const { container } = render(WindowedFeedFixture, {
      props: { messages: transcript(240), size: 50 },
    });
    expect(container.querySelectorAll(".message-entry")).toHaveLength(50);
  });
});
