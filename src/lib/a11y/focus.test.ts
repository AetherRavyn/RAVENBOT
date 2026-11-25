// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { rememberFocus, trapFocus } from "./index";

function mount() {
  document.body.innerHTML = `
    <div id="trap">
      <button id="first">First</button>
      <button id="last">Last</button>
    </div>
    <button id="opener">Opener</button>
  `;
  const container = document.getElementById("trap") as HTMLElement;
  return {
    container,
    first: document.getElementById("first") as HTMLButtonElement,
    last: document.getElementById("last") as HTMLButtonElement,
    opener: document.getElementById("opener") as HTMLButtonElement,
  };
}

afterEach(() => {
  document.body.innerHTML = "";
});

describe("trapFocus", () => {
  it("moves focus to the first control and wraps Tab at the edges", () => {
    const { container, first, last } = mount();
    const cleanup = trapFocus(container);
    expect(document.activeElement).toBe(first);

    last.focus();
    const forward = new KeyboardEvent("keydown", { key: "Tab", bubbles: true, cancelable: true });
    last.dispatchEvent(forward);
    expect(forward.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(first);

    const backward = new KeyboardEvent("keydown", {
      key: "Tab",
      shiftKey: true,
      bubbles: true,
      cancelable: true,
    });
    first.dispatchEvent(backward);
    expect(backward.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(last);
    cleanup();
  });

  it("stops wrapping after cleanup", () => {
    const { container, first, last } = mount();
    const cleanup = trapFocus(container);
    cleanup();
    last.focus();
    const event = new KeyboardEvent("keydown", { key: "Tab", bubbles: true, cancelable: true });
    last.dispatchEvent(event);
    expect(document.activeElement).toBe(last);
    expect(event.defaultPrevented).toBe(false);
    expect(first).toBeTruthy();
  });
});

describe("rememberFocus", () => {
  it("returns focus to the opener", () => {
    const { opener } = mount();
    opener.focus();
    const restore = rememberFocus();
    const other = document.getElementById("first") as HTMLButtonElement;
    other.focus();
    restore();
    expect(document.activeElement).toBe(opener);
  });

  it("does nothing when the opener was removed", () => {
    const { opener } = mount();
    opener.focus();
    const restore = rememberFocus();
    opener.remove();
    expect(() => restore()).not.toThrow();
  });
});
