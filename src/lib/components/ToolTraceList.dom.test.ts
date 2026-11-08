// @vitest-environment jsdom
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { render, cleanup } from "@testing-library/svelte";
import ToolTraceList from "$lib/components/ToolTraceList.svelte";
import { setLocale } from "$lib/i18n";

describe("ToolTraceList exported documents", () => {
  beforeEach(() => {
    cleanup();
    setLocale("en");
  });
  afterEach(cleanup);

  it("shows an Open card for a real exported document", () => {
    const tools = [
      {
        name: "make_document",
        arguments: { format: "pptx" },
        result: {
          success: true,
          output: { path: "/tmp/workspace/documents/q3-review.pptx", format: "pptx", bytes: 12345 },
        },
        duration_ms: 420,
      },
    ];
    const { container } = render(ToolTraceList, { props: { tools } });
    expect(container.querySelector(".tools__document")?.textContent).toContain("q3-review.pptx");
    expect(container.querySelector(".tools__document-format")?.textContent).toContain("pptx");
    expect(container.querySelector(".tools__document button")?.textContent).toContain("Open");
  });

  it("shows rendered images for a designed slide deck", () => {
    const tools = [
      {
        name: "make_slides",
        arguments: { filename: "q3-review" },
        result: {
          success: true,
          output: {
            count: 2,
            folder: "/tmp/workspace/documents/slides/q3-review",
            pptx: "/tmp/workspace/documents/slides/q3-review/q3-review.pptx",
            slides: [
              { index: 1, layout: "title", file: "slide-01.png" },
              { index: 2, layout: "stat", file: "slide-02.png" },
            ],
          },
        },
        duration_ms: 900,
      },
    ];
    const { container } = render(ToolTraceList, { props: { tools } });
    expect(container.querySelector(".tools__deck")).not.toBeNull();
    expect(container.querySelectorAll(".tools__deck-slide")).toHaveLength(2);
    expect(container.querySelector(".tools__deck-slide img")?.getAttribute("src")).toContain(
      "/tmp/workspace/documents/slides/q3-review/slide-01.png",
    );
    expect(container.querySelector(".tools__deck-title")?.textContent).toContain("2 designed slides");
    expect(container.querySelector(".tools__deck-head button")?.textContent).toContain("Open PPTX");
  });

  it("does not invent a card for an ordinary JSON result", () => {
    const tools = [
      {
        name: "file_read",
        arguments: { path: "README.md" },
        result: { success: true, output: { content: "# hello" } },
      },
    ];
    const { container } = render(ToolTraceList, { props: { tools } });
    expect(container.querySelector(".tools__document")).toBeNull();
  });
});
