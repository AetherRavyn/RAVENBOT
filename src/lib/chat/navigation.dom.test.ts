// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup } from "@testing-library/svelte";
import { navigateMessage } from "./navigation";

function row(id: string, top: number, height = 80) {
  const element = document.createElement("div");
  element.className = "message-entry";
  element.id = id;
  element.getBoundingClientRect = () =>
    ({
      top,
      left: 0,
      width: 600,
      height,
      bottom: top + height,
      right: 600,
      x: 0,
      y: top,
      toJSON: () => ({}),
    }) as DOMRect;
  element.scrollIntoView = vi.fn();
  return element;
}

function container(rows: HTMLElement[]) {
  const element = document.createElement("div");
  element.getBoundingClientRect = () =>
    ({
      top: 0,
      left: 0,
      width: 600,
      height: 500,
      bottom: 500,
      right: 600,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    }) as DOMRect;
  for (const child of rows) element.appendChild(child);
  document.body.appendChild(element);
  return element;
}

beforeEach(cleanup);
afterEach(() => {
  cleanup();
  document.body.innerHTML = "";
});

describe("navigateMessage", () => {
  it("scrolls to the adjacent mounted row", () => {
    const first = row("message-a", 0);
    const second = row("message-b", 100);
    const onBoundary = vi.fn();
    const target = navigateMessage(container([first, second]), 1, false, onBoundary);
    expect(target).toBe(second);
    expect(second.scrollIntoView).toHaveBeenCalled();
    expect(onBoundary).not.toHaveBeenCalled();
  });

  it("hands the edge back to the virtualizer instead of stopping", () => {
    const only = row("message-a", 0);
    const onBoundary = vi.fn();
    const target = navigateMessage(container([only]), 1, false, onBoundary);
    expect(target).toBeNull();
    expect(onBoundary).toHaveBeenCalledWith(1, only);
  });
});
