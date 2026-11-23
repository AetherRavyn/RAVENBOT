// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import TranscriptRail from "./TranscriptRail.svelte";
import { setLocale } from "$lib/i18n";

beforeEach(() => {
  cleanup();
  setLocale("en");
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
});

afterEach(cleanup);

function fakeContainer() {
  const container = document.createElement("div");
  Object.defineProperty(container, "scrollHeight", {
    value: 1000,
    configurable: true,
  });
  Object.defineProperty(container, "clientHeight", {
    value: 500,
    configurable: true,
  });
  container.scrollTop = 50;
  container.getBoundingClientRect = () =>
    ({ top: 0, left: 0, width: 600, height: 500, bottom: 500, right: 600, x: 0, y: 0, toJSON: () => ({}) }) as DOMRect;

  for (const [id, top] of [
    ["message-1", 100],
    ["message-2", 700],
  ] as const) {
    const row = document.createElement("div");
    row.id = id;
    row.getBoundingClientRect = () =>
      ({ top, left: 0, width: 600, height: 80, bottom: top + 80, right: 600, x: 0, y: top, toJSON: () => ({}) }) as DOMRect;
    container.appendChild(row);
  }
  document.body.appendChild(container);
  return container;
}

describe("TranscriptRail", () => {
  it("renders ticks and the viewport, and jumps to a message", async () => {
    const container = fakeContainer();
    const onJump = vi.fn();
    const { getByLabelText, container: view } = render(TranscriptRail, {
      props: {
        container,
        entries: [
          { id: "message-1", kind: "user", label: "First message" },
          { id: "message-2", kind: "user", label: "Second message" },
        ],
        onJump,
      },
    });

    const first = await waitFor(() =>
      getByLabelText("First message"),
    );
    await waitFor(() => expect(getByLabelText("Second message")).toBeTruthy());
    const viewport = view.querySelector(".transcript-rail-viewport") as HTMLElement;
    expect(viewport.style.top).toBe("5%");
    expect(viewport.style.height).toBe("50%");

    await fireEvent.click(first);
    expect(onJump).toHaveBeenCalledWith("message-1");
  });

  it("stays hidden when the transcript has no measured height", async () => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    const { container: view } = render(TranscriptRail, {
      props: {
        container,
        entries: [{ id: "message-1", kind: "user", label: "First message" }],
        onJump: vi.fn(),
      },
    });
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(view.querySelector(".transcript-rail")).toBeNull();
  });
});
