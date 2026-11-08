// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { tick } from "svelte";
import MarkdownCode from "./MarkdownCode.svelte";
import { setLocale } from "$lib/i18n";

describe("MarkdownCode", () => {
  beforeEach(() => {
    cleanup();
    setLocale("en");
  });
  afterEach(cleanup);

  it("renders one line box per source line", () => {
    const code = "const a = 1;\n\nfunction run() {\n  return a;\n}";
    const { container } = render(MarkdownCode, {
      props: { code, language: "ts" },
    });

    const lines = container.querySelectorAll(".md-code-line");
    expect(lines).toHaveLength(5);
    expect(lines[0].getAttribute("data-line")).toBe("1");
    expect(lines[4].getAttribute("data-line")).toBe("5");
    // Empty source lines are still numbered lines, not gaps.
    expect(lines[1].textContent).toBe("");
  });

  it("keeps line numbers out of the copied text", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });

    const code = "fn main() {\n    println!(\"hi\");\n}";
    const { container, getByText } = render(MarkdownCode, {
      props: { code, language: "rs" },
    });

    // The number is generated in CSS (`::before`), so it is not a text node
    // and cannot enter a selection or a clipboard payload.
    const text = (container.querySelector("code")?.textContent ?? "").trimStart();
    expect(text.startsWith("fn main()")).toBe(true);

    await fireEvent.click(getByText("Copy"));
    expect(writeText).toHaveBeenCalledWith(code);
  });

  it("defers highlighting until the block is near the viewport", async () => {
    let fire: IntersectionObserverCallback | undefined;
    class FakeObserver {
      root = null;
      rootMargin = "";
      thresholds = [];
      constructor(callback: IntersectionObserverCallback) {
        fire = callback;
      }
      observe() {}
      unobserve() {}
      disconnect() {}
      takeRecords(): IntersectionObserverEntry[] {
        return [];
      }
    }
    vi.stubGlobal("IntersectionObserver", FakeObserver);
    try {
      const { container } = render(MarkdownCode, {
        props: { code: "// comment\nconst x = 1;", language: "ts" },
      });
      expect(container.querySelector(".tok-comment")).toBeNull();
      expect(container.textContent).toContain("// comment");

      fire?.(
        [{ isIntersecting: true } as IntersectionObserverEntry],
        {} as IntersectionObserver,
      );
      await tick();
      expect(container.querySelector(".tok-comment")).not.toBeNull();
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("keeps a multi-line token coloured as one token", () => {
    const code = "/* first\n * second\n */\nconst x = 1;";
    const { container } = render(MarkdownCode, {
      props: { code, language: "ts" },
    });
    const comment = [...container.querySelectorAll(".tok-comment")]
      .map((span) => span.textContent ?? "")
      .join("");
    expect(comment).toContain("first");
    expect(comment).toContain("second");
    expect(container.querySelectorAll(".md-code-line")).toHaveLength(4);
  });
});
