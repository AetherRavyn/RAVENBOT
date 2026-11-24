// @vitest-environment jsdom
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { render, cleanup } from "@testing-library/svelte";
import ToolTraceList from "$lib/components/ToolTraceList.svelte";
import ImageGeneration from "$lib/components/chat/ImageGeneration.svelte";
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

  it("shows a generated image with its prompt, provider and open link", () => {
    const tools = [
      {
        name: "image_gen",
        arguments: { prompt: "a red fox in snow", size: "1024x1024" },
        result: {
          success: true,
          output: {
            provider: "openai",
            prompt: "a red fox in snow",
            url: "https://example.com/fox.png",
          },
        },
        duration_ms: 1200,
      },
    ];
    const { container } = render(ToolTraceList, { props: { tools } });
    const card = container.querySelector('.image-gen[data-state="completed"]');
    expect(card).not.toBeNull();
    expect(card?.querySelector("img")?.getAttribute("src")).toBe("https://example.com/fox.png");
    expect(card?.querySelector(".image-gen__prompt")?.textContent).toContain("a red fox");
    expect(card?.querySelector(".image-gen__meta")?.textContent).toContain("openai");
    expect(card?.querySelector(".image-gen__open")?.getAttribute("href")).toBe(
      "https://example.com/fox.png",
    );
  });

  it("shows a failed image generation instead of a blank box", () => {
    const tools = [
      {
        name: "image_gen",
        arguments: { prompt: "a red fox in snow" },
        result: { success: false, error: "provider rate limit" },
        is_error: true,
      },
    ];
    const { container } = render(ToolTraceList, { props: { tools } });
    const card = container.querySelector('.image-gen[data-state="failed"]');
    expect(card).not.toBeNull();
    expect(card?.textContent).toContain("provider rate limit");
    expect(card?.querySelector("img")).toBeNull();
  });

  it("renders the live generating state", () => {
    const { container } = render(ImageGeneration, {
      props: { state: "generating", prompt: "a red fox in snow", size: "1024x1024" },
    });
    expect(container.querySelector('.image-gen[data-state="generating"]')).not.toBeNull();
    expect(container.textContent).toContain("Generating image");
    expect(container.querySelector("img")).toBeNull();
  });

  it("renders a shell command as a terminal card", async () => {
    const tools = [
      {
        name: "shell_exec",
        arguments: { command: "cargo test -q" },
        result: {
          success: true,
          output: { stdout: "test result: ok. 3 passed", stderr: "", exit_code: 0 },
        },
        duration_ms: 820,
      },
    ];
    const { container, getByRole } = render(ToolTraceList, { props: { tools } });
    const card = container.querySelector(".tool-card--command");
    expect(card?.textContent).toContain("cargo test -q");
    expect(card?.textContent).toContain("test result: ok");
    expect(card?.querySelector('[data-ok="true"]')?.textContent).toContain("exit 0");
    expect(card?.getAttribute("data-expanded")).toBe("false");

    await (await import("@testing-library/svelte")).fireEvent.click(
      getByRole("button", { expanded: false }),
    );
    expect(card?.getAttribute("data-expanded")).toBe("true");
  });

  it("renders an edit as a diff card with its file counts", () => {
    const diff = "--- a/src/a.ts\n+++ b/src/a.ts\n@@ -1 +1 @@\n-old\n+new";
    const tools = [
      {
        name: "file_edit",
        arguments: { patch: diff },
        result: {
          success: true,
          output: {
            applied: true,
            file_changes: [
              { path: "src/a.ts", lines_added: 1, lines_deleted: 1, diff },
            ],
          },
        },
      },
    ];
    const { container } = render(ToolTraceList, { props: { tools } });
    const card = container.querySelector(".tool-card--diff");
    expect(card?.textContent).toContain("1 file changed");
    expect(card?.textContent).toContain("src/a.ts");
    expect(card?.querySelector(".tools__document")).toBeNull();
    expect(card?.querySelector(".dif")).not.toBeNull();
  });

  it("renders a file read with a line-number window", () => {
    const tools = [
      {
        name: "file_read",
        arguments: { path: "src/main.ts" },
        result: {
          success: true,
          output: {
            path: "/ws/src/main.ts",
            content: "first\nsecond",
            offset: 10,
            returned_lines: 2,
            total_lines: 40,
            next_offset: null,
          },
        },
      },
    ];
    const { container } = render(ToolTraceList, { props: { tools } });
    const card = container.querySelector(".tool-card--read");
    expect(card?.textContent).toContain("/ws/src/main.ts");
    expect(card?.textContent).toContain("lines 10–11 of 40");
    const lineNumbers = [...(card?.querySelectorAll(".tool-card__line-no") ?? [])].map(
      (el) => el.textContent,
    );
    expect(lineNumbers).toEqual(["10", "11"]);
    expect(card?.textContent).toContain("second");
  });

  it("renders search matches as a result card", () => {
    const tools = [
      {
        name: "code_search",
        arguments: { pattern: "TODO" },
        result: {
          success: true,
          output: {
            pattern: "TODO",
            path: ".",
            matches: ["src/a.ts:4:TODO fix"],
            count: 1,
            truncated: false,
          },
        },
      },
    ];
    const { container } = render(ToolTraceList, { props: { tools } });
    const card = container.querySelector(".tool-card--search");
    expect(card?.textContent).toContain("TODO");
    expect(card?.textContent).toContain("1 result");
    expect(card?.textContent).toContain("src/a.ts:4:TODO fix");
  });

  it("keeps the generic body when a specialised card has no data", async () => {
    const tools = [
      {
        name: "file_edit",
        arguments: { patch: "bad" },
        result: { success: false, error: "patch did not match" },
        is_error: true,
      },
    ];
    const { container, getByRole } = render(ToolTraceList, { props: { tools } });
    expect(container.querySelector(".tool-card--diff")).toBeNull();
    await (await import("@testing-library/svelte")).fireEvent.click(
      getByRole("button", { expanded: false }),
    );
    expect(container.querySelector(".tools__body")?.textContent).toContain(
      "patch did not match",
    );
  });

  it("does not invent a card for an ordinary JSON result", () => {
    const tools = [
      {
        name: "memory_search",
        arguments: { query: "hello" },
        result: { success: true, output: { results: [] } },
      },
    ];
    const { container } = render(ToolTraceList, { props: { tools } });
    expect(container.querySelector(".tools__document")).toBeNull();
  });
});
