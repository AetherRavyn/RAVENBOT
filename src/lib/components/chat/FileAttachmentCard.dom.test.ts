// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import FileAttachmentCard from "./FileAttachmentCard.svelte";

describe("FileAttachmentCard", () => {
  beforeEach(() => {
    cleanup();
    // jsdom has no object-URL implementation; the card only needs it to exist
    // when the download button is pressed.
    URL.createObjectURL = vi.fn(() => "blob:attachment");
    URL.revokeObjectURL = vi.fn();
  });
  afterEach(cleanup);

  it("shows the file name, kind, and decoded size", () => {
    const { container, getByText } = render(FileAttachmentCard, {
      props: {
        att: {
          name: "audit-report.pdf",
          mime_type: "application/pdf",
          data: "aGVsbG8=",
          size: 5,
        },
      },
    });

    expect(getByText("audit-report.pdf")).toBeTruthy();
    expect(getByText("PDF · 5 B")).toBeTruthy();
    expect(container.querySelector(".file-attachment")?.getAttribute("aria-label")).toBe(
      "Download audit-report.pdf",
    );
  });

  it("derives the size when the stored row predates the byte count", () => {
    const { getByText } = render(FileAttachmentCard, {
      props: {
        att: { name: "notes.txt", mime_type: "text/plain", data: "aGVsbG8=" },
      },
    });
    expect(getByText("TXT · 5 B")).toBeTruthy();
  });
});
