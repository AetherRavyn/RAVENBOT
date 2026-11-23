// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";
import MessageSelectionToolbar from "./MessageSelectionToolbar.svelte";
import { setLocale } from "$lib/i18n";

function select(node: Node): void {
  const range = document.createRange();
  range.selectNodeContents(node);
  const selection = window.getSelection();
  selection?.removeAllRanges();
  selection?.addRange(range);
}

describe("MessageSelectionToolbar", () => {
  beforeEach(() => {
    cleanup();
    setLocale("en");
  });
  afterEach(() => {
    cleanup();
    window.getSelection()?.removeAllRanges();
    document.body.innerHTML = "";
  });

  it("appears over a passage and reports the row id", async () => {
    const container = document.createElement("div");
    container.innerHTML = `<div data-message-id="m1"><p id="passage">A quotable passage.</p></div>`;
    document.body.append(container);
    const onQuote = vi.fn();
    const onReply = vi.fn();
    const { getByText, queryByText } = render(MessageSelectionToolbar, {
      props: { container, onQuote, onReply },
    });

    expect(queryByText("Quote")).toBeNull();
    select(container.querySelector("#passage")!);
    document.dispatchEvent(new Event("pointerup", { bubbles: true }));
    await tick();
    await waitFor(() => expect(getByText("Quote")).toBeTruthy());

    await fireEvent.click(getByText("Quote"));
    expect(onQuote).toHaveBeenCalledTimes(1);
    expect(onQuote.mock.calls[0][0]).toMatchObject({
      messageId: "m1",
      text: "A quotable passage.",
    });

    select(container.querySelector("#passage")!);
    document.dispatchEvent(new Event("pointerup", { bubbles: true }));
    await tick();
    await waitFor(() => expect(getByText("Reply")).toBeTruthy());
    await fireEvent.click(getByText("Reply"));
    expect(onReply).toHaveBeenCalledWith("m1");
  });

  it("quotes the passage with the keyboard shortcut", async () => {
    const container = document.createElement("div");
    container.innerHTML = `<div data-message-id="m1"><p id="passage">A keyboard quote.</p></div>`;
    document.body.append(container);
    const onQuote = vi.fn();
    render(MessageSelectionToolbar, { props: { container, onQuote } });

    select(container.querySelector("#passage")!);
    document.dispatchEvent(new Event("pointerup", { bubbles: true }));
    await tick();
    await waitFor(() => expect(document.querySelector(".message-selection-toolbar")).not.toBeNull());

    document.dispatchEvent(
      new KeyboardEvent("keydown", {
        key: ".",
        ctrlKey: true,
        shiftKey: true,
        bubbles: true,
      }),
    );
    expect(onQuote).toHaveBeenCalledWith(
      expect.objectContaining({ messageId: "m1", text: "A keyboard quote." }),
    );
  });

  it("does not appear for a selection outside a message row", async () => {
    const container = document.createElement("div");
    container.innerHTML = `<p id="loose">Not in a message.</p>`;
    document.body.append(container);
    const { queryByText } = render(MessageSelectionToolbar, { props: { container } });

    select(container.querySelector("#loose")!);
    document.dispatchEvent(new Event("pointerup", { bubbles: true }));
    await tick();
    await tick();
    expect(queryByText("Quote")).toBeNull();
  });
});
