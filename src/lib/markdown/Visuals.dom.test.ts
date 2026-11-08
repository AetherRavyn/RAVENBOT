// @vitest-environment jsdom
/**
 * The visual fences, mounted.
 *
 * The lexer proves a `chart` token exists; this proves the renderer turns it
 * into actual SVG and that a malformed chart fails visibly instead of taking
 * the message down. The slide deck is tested through a real button click, not
 * by reading its state, because the navigation is the feature.
 */
import { describe, it, expect, beforeEach } from "vitest";
import { render, cleanup, fireEvent, waitFor } from "@testing-library/svelte";
import MarkdownMessage from "./MarkdownMessage.svelte";
import ImageLightbox from "$lib/components/chat/ImageLightbox.svelte";
import ImageGallery from "$lib/components/chat/ImageGallery.svelte";
import { setLocale } from "$lib/i18n";

beforeEach(() => {
  cleanup();
  setLocale("en");
  // jsdom does not implement the modal dialog methods.
  const dialog = HTMLDialogElement.prototype as unknown as {
    showModal?: () => void;
    close?: () => void;
  };
  if (!dialog.showModal) {
    dialog.showModal = function showModal(this: HTMLDialogElement) {
      this.setAttribute("open", "");
    };
  }
  if (!dialog.close) {
    dialog.close = function close(this: HTMLDialogElement) {
      this.removeAttribute("open");
    };
  }
});

const renderMd = (content: string) => render(MarkdownMessage, { props: { content } });

describe("visual fences", () => {
  it("renders a bar chart as SVG with a title and legend", async () => {
    const content = `\`\`\`chart
{
  "type": "bar",
  "title": "Monthly revenue",
  "labels": ["Jan", "Feb", "Mar"],
  "series": [
    { "name": "Revenue", "values": [120, 154, 132] },
    { "name": "Cost", "values": [80, 91, 88] }
  ]
}
\`\`\``;
    const { container } = renderMd(content);
    await waitFor(() => expect(container.querySelector(".md-chart svg")).not.toBeNull());
    expect(container.querySelector(".md-chart-title")?.textContent).toBe("Monthly revenue");
    expect(container.querySelectorAll(".md-chart svg rect").length).toBe(6);
    expect(container.querySelector(".md-chart-download")?.textContent).toContain("SVG");
    expect(container.textContent).toContain("Revenue");
    expect(container.textContent).toContain("Cost");
  });

  it("renders a donut chart with one path per slice", async () => {
    const content = `\`\`\`chart
{
  "type": "donut",
  "title": "Spend",
  "labels": ["Infra", "Payroll", "Tools"],
  "series": [{ "name": "Spend", "values": [42, 31, 12] }]
}
\`\`\``;
    const { container } = renderMd(content);
    await waitFor(() => expect(container.querySelector(".md-chart svg")).not.toBeNull());
    expect(container.querySelectorAll(".md-chart svg path").length).toBe(3);
    expect(container.textContent).toContain("Infra");
  });

  it("shows an error and the source when the chart JSON is invalid", async () => {
    const content = "```chart\n{ not json }\n```";
    const { container } = renderMd(content);
    await waitFor(() => expect(container.querySelector(".md-chart--error")).not.toBeNull());
    expect(container.textContent).toContain("Chart could not be rendered");
    expect(container.querySelector(".md-chart-source")?.textContent).toContain("{ not json }");
  });

  it("renders a script-enabled visual page in an isolated frame", async () => {
    const content = `\`\`\`visual
<!doctype html>
<html><body>
  <h1>Interactive test</h1>
  <script>document.body.dataset.ready = "yes";</script>
</body></html>
\`\`\``;
    const { container } = renderMd(content);
    await waitFor(() => expect(container.querySelector(".md-visual iframe")).not.toBeNull());
    const frame = container.querySelector(".md-visual iframe") as HTMLIFrameElement;
    expect(frame.getAttribute("sandbox")).toBe("allow-scripts");
    expect(frame.getAttribute("srcdoc")).toContain("Interactive test");
    expect(frame.getAttribute("srcdoc")).toContain("__ravenbotVisual");
    // The page's heading must live inside the frame, never in the app DOM.
    expect(container.querySelector(".md-visual h1")).toBeNull();
  });

  it("opens a markdown image in the lightbox", async () => {
    const { container } = renderMd("![a chart](https://example.com/chart.png)");
    const image = container.querySelector(".md-image") as HTMLImageElement;
    expect(image).not.toBeNull();
    const detail = new Promise<{ src: string; alt: string }>((resolve) => {
      window.addEventListener(
        "ravenbot:image",
        (event) => resolve((event as CustomEvent).detail),
        { once: true },
      );
    });
    await fireEvent.click(image);
    await expect(detail).resolves.toEqual({
      src: "https://example.com/chart.png",
      alt: "a chart",
    });
  });

  it("shows one image at its own aspect ratio instead of cropping it", () => {
    const { container } = render(ImageGallery, {
      props: { images: [{ src: "https://example.com/shot.png", alt: "Screenshot" }] },
    });
    expect(container.querySelector(".image-gallery")?.classList.contains("image-gallery--single")).toBe(true);
  });

  it("opens a gallery at the image that was clicked", async () => {
    const { container } = render(ImageGallery, {
      props: {
        images: [
          { src: "https://example.com/one.png", alt: "One" },
          { src: "https://example.com/two.png", alt: "Two" },
        ],
      },
    });
    const buttons = container.querySelectorAll(".image-gallery-item");
    expect(buttons.length).toBe(2);
    const detail = new Promise<any>((resolve) => {
      window.addEventListener("ravenbot:image", (event) => resolve((event as CustomEvent).detail), {
        once: true,
      });
    });
    await fireEvent.click(buttons[1]);
    await expect(detail).resolves.toMatchObject({ index: 1 });
    await expect(detail).resolves.toMatchObject({
      images: [
        { src: "https://example.com/one.png", alt: "One" },
        { src: "https://example.com/two.png", alt: "Two" },
      ],
    });
  });

  it("opens from a ravenbot:image event and closes from the button", async () => {
    const { container } = render(ImageLightbox);
    window.dispatchEvent(
      new CustomEvent("ravenbot:image", {
        detail: { src: "https://example.com/chart.png", alt: "A chart" },
      }),
    );
    await waitFor(() => expect(container.querySelector("dialog")?.hasAttribute("open")).toBe(true));
    expect(container.querySelector("dialog img")?.getAttribute("src")).toBe(
      "https://example.com/chart.png",
    );
    await fireEvent.click(container.querySelector('button[aria-label="Close image"]')!);
    await waitFor(() => expect(container.querySelector("dialog")?.hasAttribute("open")).toBe(false));
  });

  it("steps through a gallery in the lightbox", async () => {
    const { container } = render(ImageLightbox);
    window.dispatchEvent(
      new CustomEvent("ravenbot:image", {
        detail: {
          images: [
            { src: "https://example.com/one.png", alt: "One" },
            { src: "https://example.com/two.png", alt: "Two" },
          ],
          index: 0,
        },
      }),
    );
    await waitFor(() => expect(container.querySelector("dialog")?.hasAttribute("open")).toBe(true));
    expect(container.querySelector("dialog img")?.getAttribute("src")).toBe(
      "https://example.com/one.png",
    );
    expect(container.querySelector(".image-lightbox-count")?.textContent?.trim()).toBe("1 / 2");
    await fireEvent.click(container.querySelector('button[aria-label="Next image"]')!);
    await waitFor(() =>
      expect(container.querySelector("dialog img")?.getAttribute("src")).toBe(
        "https://example.com/two.png",
      ),
    );
    expect(container.querySelector(".image-lightbox-count")?.textContent?.trim()).toBe("2 / 2");
  });

  it("navigates a slide deck with the next button", async () => {
    const content = `\`\`\`slides
# One

First slide.

---

# Two

Second slide.
\`\`\``;
    const { container } = renderMd(content);
    await waitFor(() => expect(container.querySelector(".md-slides")).not.toBeNull());
    expect(container.querySelector(".md-slides-count")?.textContent?.trim()).toBe("1 / 2");
    expect(container.querySelector(".md-slides-page")?.textContent).toContain("One");

    const next = container.querySelector('button[aria-label="Next slide"]');
    expect(next).not.toBeNull();
    await fireEvent.click(next!);
    await waitFor(() =>
      expect(container.querySelector(".md-slides-count")?.textContent?.trim()).toBe("2 / 2"),
    );
    expect(container.querySelector(".md-slides-page")?.textContent).toContain("Two");
    expect(container.querySelector(".md-slides-present")).not.toBeNull();
    expect(container.querySelector(".md-slides-progress span")).not.toBeNull();

    await fireEvent.keyDown(container.querySelector(".md-slides")!, { key: "ArrowLeft" });
    await waitFor(() =>
      expect(container.querySelector(".md-slides-count")?.textContent?.trim()).toBe("1 / 2"),
    );
  });
});
