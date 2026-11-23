// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import {
  MAX_SELECTION_QUOTE,
  messageRowFor,
  quoteAsMarkdown,
  selectionQuote,
  selectionToolbarPosition,
} from "./selection";

function transcript(): HTMLElement {
  const root = document.createElement("div");
  root.innerHTML = `
    <div data-message-id="m1"><p id="first">The first message has selectable prose.</p></div>
    <div data-message-id="m2"><p id="second">The second message is separate.</p></div>
  `;
  document.body.append(root);
  return root;
}

function select(node: Node): Selection {
  const range = document.createRange();
  range.selectNodeContents(node);
  const selection = window.getSelection();
  selection?.removeAllRanges();
  selection?.addRange(range);
  return selection as Selection;
}

afterEach(() => {
  window.getSelection()?.removeAllRanges();
  document.body.innerHTML = "";
});

describe("selectionQuote", () => {
  it("reads a passage from one message row", () => {
    const root = transcript();
    const quote = selectionQuote(select(root.querySelector("#first")!), root);
    expect(quote?.messageId).toBe("m1");
    expect(quote?.text).toContain("selectable prose");
  });

  it("rejects a selection that crosses two messages", () => {
    const root = transcript();
    const range = document.createRange();
    range.setStart(root.querySelector("#first")!.firstChild!, 0);
    range.setEnd(root.querySelector("#second")!.firstChild!, 5);
    const selection = window.getSelection()!;
    selection.removeAllRanges();
    selection.addRange(range);
    expect(selectionQuote(selection, root)).toBeNull();
  });

  it("rejects a selection that spans a link", () => {
    const root = document.createElement("div");
    root.innerHTML = `<div data-message-id="m1"><p id="mixed">Read <a href="https://example.com">the docs</a> now.</p></div>`;
    document.body.append(root);
    const quote = selectionQuote(select(root.querySelector("#mixed")!), root);
    expect(quote).toBeNull();
  });

  it("rejects an empty row id and an oversized selection", () => {
    const root = document.createElement("div");
    root.innerHTML = `<div data-message-id=""><p id="blank">No id.</p></div>`;
    document.body.append(root);
    expect(selectionQuote(select(root.querySelector("#blank")!), root)).toBeNull();

    const longRoot = document.createElement("div");
    longRoot.innerHTML = `<div data-message-id="m3"><p id="long">${"x".repeat(MAX_SELECTION_QUOTE + 1)}</p></div>`;
    document.body.append(longRoot);
    expect(selectionQuote(select(longRoot.querySelector("#long")!), longRoot)).toBeNull();
  });

  it("finds the row around a text node", () => {
    const root = transcript();
    const row = messageRowFor(root.querySelector("#first")!.firstChild, root);
    expect(row?.getAttribute("data-message-id")).toBe("m1");
  });
});

describe("selectionToolbarPosition", () => {
  const toolbar = { width: 160, height: 32 };
  const viewport = { width: 800, height: 600 };

  it("sits above a selection with room", () => {
    const position = selectionToolbarPosition(
      { top: 200, right: 400, bottom: 220, left: 300, width: 100, height: 20 },
      toolbar,
      viewport,
    );
    expect(position.placement).toBe("top");
    expect(position.top).toBe(160);
    expect(position.left).toBe(270);
  });

  it("flips below a selection at the top and clamps to the viewport", () => {
    const position = selectionToolbarPosition(
      { top: 4, right: 60, bottom: 24, left: 20, width: 40, height: 20 },
      toolbar,
      viewport,
    );
    expect(position.placement).toBe("bottom");
    expect(position.top).toBe(32);
    expect(position.left).toBe(12);
  });
});

describe("quoteAsMarkdown", () => {
  it("prefixes every line of a selection", () => {
    expect(quoteAsMarkdown("first line\nsecond line")).toBe("> first line\n> second line");
  });
});
