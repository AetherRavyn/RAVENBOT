// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { KeyboardShortcutManager } from "./index";

afterEach(() => {
  document.body.innerHTML = "";
});

function press(init: KeyboardEventInit, target: EventTarget = document.body) {
  target.dispatchEvent(
    new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init }),
  );
}

describe("KeyboardShortcutManager", () => {
  it("runs a global shortcut and prevents the browser default", () => {
    const manager = new KeyboardShortcutManager();
    const callback = vi.fn();
    const unregister = manager.register("mod+k", callback);
    const event = new KeyboardEvent("keydown", {
      bubbles: true,
      cancelable: true,
      key: "k",
      ctrlKey: true,
    });
    document.body.dispatchEvent(event);
    expect(callback).toHaveBeenCalledTimes(1);
    expect(event.defaultPrevented).toBe(true);
    unregister();
  });

  it("does not consume a printable hint while typing", () => {
    const manager = new KeyboardShortcutManager();
    const callback = vi.fn();
    const unregister = manager.register("shift+?", callback, { ignoreEditable: true });
    const input = document.createElement("input");
    document.body.appendChild(input);

    const typed = new KeyboardEvent("keydown", {
      bubbles: true,
      cancelable: true,
      key: "?",
      shiftKey: true,
    });
    input.dispatchEvent(typed);
    expect(callback).not.toHaveBeenCalled();
    expect(typed.defaultPrevented).toBe(false);

    press({ key: "?", shiftKey: true });
    expect(callback).toHaveBeenCalledTimes(1);
    unregister();
  });

  it("stops listening after unregister", () => {
    const manager = new KeyboardShortcutManager();
    const callback = vi.fn();
    const unregister = manager.register("mod+n", callback);
    unregister();
    press({ key: "n", ctrlKey: true });
    expect(callback).not.toHaveBeenCalled();
  });
});
