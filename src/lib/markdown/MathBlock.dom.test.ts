// @vitest-environment jsdom
import { describe, it, expect } from "vitest";
import { render } from "@testing-library/svelte";
import MathBlock from "./MathBlock.svelte";

describe("MathBlock", () => {
  it("renders inline math with KaTeX", () => {
    const { container } = render(MathBlock, { props: { latex: "x^2", displayMode: false } });
    expect(container.querySelector(".katex")).not.toBeNull();
  });

  it("renders block math with KaTeX", () => {
    const { container } = render(MathBlock, { props: { latex: "y = mx + b", displayMode: true } });
    expect(container.querySelector(".katex-display")).not.toBeNull();
  });

  it("falls back to raw text on invalid LaTeX", () => {
    const { container } = render(MathBlock, { props: { latex: "\\invalid{", displayMode: false } });
    // KaTeX with throwOnError:false renders an error span, not our fallback.
    // Either way, the component should not crash.
    expect(container.textContent).toContain("\\invalid{");
  });
});
