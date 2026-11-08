// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import ChatMessageRow from "./ChatMessageRow.svelte";
import { setLocale } from "$lib/i18n";

describe("ChatMessageRow", () => {
  beforeEach(() => {
    cleanup();
    setLocale("en");
    // The row smooths its height with a ResizeObserver, which jsdom omits.
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

  it("shows persisted reactions and removes one on click", async () => {
    const onReact = vi.fn();
    const { container, getByLabelText } = render(ChatMessageRow, {
      props: {
        isUser: false,
        text: "The build is green.",
        reactions: ["👍", "🎉"],
        onReact,
      },
    });

    expect(container.querySelectorAll(".message-reaction-chip")).toHaveLength(2);
    await fireEvent.click(getByLabelText("Remove 👍 reaction"));
    expect(onReact).toHaveBeenCalledWith("👍");
  });

  it("opens the palette and reports the chosen emoji", async () => {
    const onReact = vi.fn();
    const { container, getByLabelText } = render(ChatMessageRow, {
      props: { isUser: false, text: "Ship it.", onReact },
    });

    await fireEvent.click(getByLabelText("Add reaction"));
    expect(container.querySelectorAll(".message-reaction-choice")).toHaveLength(6);

    await fireEvent.click(getByLabelText("React with 🎉"));
    expect(onReact).toHaveBeenCalledWith("🎉");
    // Choosing closes the palette rather than leaving it over the message.
    expect(container.querySelector(".message-reaction-palette")).toBeNull();
  });

  it("shows a quoted reply and jumps to the quoted message", async () => {
    const onJumpToReply = vi.fn();
    const { getByLabelText, getByText } = render(ChatMessageRow, {
      props: {
        isUser: true,
        text: "Retrying now.",
        replyTo: {
          id: "message-7",
          author: "Scout",
          text: "The deploy failed on staging.",
        },
        onJumpToReply,
      },
    });

    expect(getByText("Scout")).toBeTruthy();
    expect(getByText("The deploy failed on staging.")).toBeTruthy();

    await fireEvent.click(getByLabelText("Reply to Scout"));
    expect(onJumpToReply).toHaveBeenCalledWith("message-7");
  });

  it("starts a reply from the row action", async () => {
    const onReply = vi.fn();
    const { getByLabelText } = render(ChatMessageRow, {
      props: { isUser: false, text: "Ready.", onReply },
    });

    await fireEvent.click(getByLabelText("Reply"));
    expect(onReply).toHaveBeenCalledTimes(1);
  });

  it("collapses a very long agent report until the reader expands it", async () => {
    const text = `${"A long finding with detail. ".repeat(160)}\n\nFinal line.`;
    const { container, getByText, queryByText } = render(ChatMessageRow, {
      props: { isUser: false, text, fullWidthAgent: true },
    });

    expect(container.querySelector(".message-body--collapsed")).not.toBeNull();
    expect(getByText("Show full answer")).toBeTruthy();

    await fireEvent.click(getByText("Show full answer"));
    expect(container.querySelector(".message-body--collapsed")).toBeNull();
    expect(queryByText("Show less")).toBeTruthy();
  });

  it("does not collapse a normal-length reply", () => {
    const { container } = render(ChatMessageRow, {
      props: { isUser: false, text: "The build is green." },
    });
    expect(container.querySelector(".message-expand")).toBeNull();
    expect(container.querySelector(".message-body--collapsed")).toBeNull();
  });

  it("carries the search hit state and a stable row id", () => {
    const { container } = render(ChatMessageRow, {
      props: {
        isUser: false,
        text: "Retry the deploy.",
        rowId: "message-42",
        searchHit: true,
        searchCurrent: true,
      },
    });

    const row = container.querySelector("#message-42");
    expect(row?.classList.contains("message-search-hit")).toBe(true);
    expect(row?.classList.contains("message-search-hit--current")).toBe(true);
  });
});
