// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";
import VirtualFeedFixture from "./VirtualFeedFixture.svelte";

const CODE = `export function reconcile(lines) {
  return lines.reduce((total, line) => total + line.amount, 0);
}`;

function transcript(count: number) {
  return Array.from({ length: count }, (_, index) => ({
    id: `row-${index}`,
    text: `Message ${index} ${index % 7 === 0 ? CODE : "short body"}`,
  }));
}

function fakeContainer() {
  const container = document.createElement("div");
  Object.defineProperty(container, "clientHeight", {
    value: 400,
    configurable: true,
  });
  Object.defineProperty(container, "scrollHeight", {
    value: 20_000,
    configurable: true,
  });
  container.scrollTop = 0;
  container.getBoundingClientRect = () =>
    ({
      top: 0,
      left: 0,
      width: 600,
      height: 400,
      bottom: 400,
      right: 600,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    }) as DOMRect;
  container.scrollTo = vi.fn() as unknown as typeof container.scrollTo;
  document.body.appendChild(container);
  return container;
}

function fakeListRect(container: HTMLElement) {
  const list = document.querySelector(".virtual-transcript") as HTMLElement;
  list.getBoundingClientRect = () =>
    ({
      top: -container.scrollTop,
      left: 0,
      width: 600,
      height: 0,
      bottom: -container.scrollTop,
      right: 600,
      x: 0,
      y: -container.scrollTop,
      toJSON: () => ({}),
    }) as DOMRect;
  return list;
}

function renderedIndexes(container: HTMLElement): number[] {
  return Array.from(
    container.querySelectorAll<HTMLElement>(".feed-row-content"),
  )
    .map((node) => Number(node.dataset.testIndex))
    .sort((a, b) => a - b);
}

beforeEach(() => {
  cleanup();
  document.body.innerHTML = "";
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

describe("VirtualTranscript", () => {
  it("mounts a bounded tail instead of every loaded row", async () => {
    const container = fakeContainer();
    const messages = transcript(240);
    const { container: view } = render(VirtualFeedFixture, {
      props: { items: messages, container, initialRows: 8 },
    });

    await waitFor(() =>
      expect(renderedIndexes(view).length).toBeGreaterThan(0),
    );
    const mounted = renderedIndexes(view);
    expect(mounted.length).toBeLessThan(30);
    expect(mounted.at(-1)).toBe(239);
    expect(view.querySelectorAll(".virtual-transcript__spacer").length).toBeGreaterThan(0);
  });

  it("re-windows around the viewport when the reader scrolls up", async () => {
    const container = fakeContainer();
    const messages = transcript(600);
    const { container: view } = render(VirtualFeedFixture, {
      props: { items: messages, container, initialRows: 8 },
    });
    await waitFor(() =>
      expect(renderedIndexes(view).length).toBeGreaterThan(0),
    );
    await waitFor(() => expect(container.scrollTop).toBe(20_000));

    fakeListRect(container);
    container.scrollTop = 16_000;
    container.dispatchEvent(new Event("scroll"));

    await waitFor(() => {
      const mounted = renderedIndexes(view);
      const first = mounted[0] ?? -1;
      const last = mounted.at(-1) ?? -1;
      expect(first, JSON.stringify(mounted)).toBeGreaterThan(0);
      expect(first).toBeLessThanOrEqual(200);
      expect(last).toBeGreaterThan(150);
      expect(mounted.length).toBeLessThan(40);
    });
  });

  it("keeps the mounted rows when older history is prepended", async () => {
    const container = fakeContainer();
    const messages = transcript(240);
    const { container: view, rerender } = render(VirtualFeedFixture, {
      props: { items: messages, container, initialRows: 8 },
    });
    await waitFor(() =>
      expect(renderedIndexes(view).length).toBeGreaterThan(0),
    );
    await waitFor(() => expect(container.scrollTop).toBe(20_000));
    fakeListRect(container);
    container.scrollTop = 12_000;
    container.dispatchEvent(new Event("scroll"));
    await waitFor(() => expect(renderedIndexes(view)[0]).toBeGreaterThan(0));

    const before = Array.from(
      view.querySelectorAll<HTMLElement>(".feed-row-content"),
    ).map((node) => node.dataset.testKey);

    const older = Array.from({ length: 50 }, (_, index) => ({
      id: `older-${index}`,
      text: `Older ${index}`,
    }));
    await rerender({ items: [...older, ...messages] });
    await waitFor(() =>
      expect(view.querySelectorAll(".feed-row-content").length).toBeGreaterThan(0),
    );

    const after = new Set(
      Array.from(view.querySelectorAll<HTMLElement>(".feed-row-content")).map(
        (node) => node.dataset.testKey,
      ),
    );
    for (const key of before) {
      expect(after.has(key), `${key} stayed mounted`).toBe(true);
    }
  });

  it("keeps following the bottom when a tall message is appended", async () => {
    const container = fakeContainer();
    const messages = transcript(120);
    const { container: view, rerender } = render(VirtualFeedFixture, {
      props: { items: messages, container, initialRows: 8 },
    });
    await waitFor(() => expect(container.scrollTop).toBe(20_000));

    Object.defineProperty(container, "scrollHeight", {
      value: 26_000,
      configurable: true,
    });
    await rerender({
      items: [
        ...messages,
        { id: "row-tall", text: "A tall answer\n".repeat(80) },
      ],
    });
    await waitFor(() => expect(container.scrollTop).toBe(26_000));
    expect(renderedIndexes(view).at(-1)).toBe(120);
  });

  it("pins back to the live tail when jump-to-latest is requested", async () => {
    const container = fakeContainer();
    const messages = transcript(240);
    const { container: view, rerender } = render(VirtualFeedFixture, {
      props: { items: messages, container, initialRows: 8 },
    });
    await waitFor(() => expect(container.scrollTop).toBe(20_000));
    fakeListRect(container);
    container.scrollTop = 8_000;
    container.dispatchEvent(new Event("scroll"));
    await waitFor(() => expect(renderedIndexes(view)[0]).toBeGreaterThan(0));

    await rerender({ pin: 1 });
    await waitFor(() => expect(container.scrollTop).toBe(20_000));
    expect(renderedIndexes(view).at(-1)).toBe(239);
  });

  it("mounts and scrolls to a requested row that is outside the viewport", async () => {
    const container = fakeContainer();
    const messages = transcript(400);
    const { container: view, rerender } = render(VirtualFeedFixture, {
      props: { items: messages, container, initialRows: 8 },
    });
    await waitFor(() =>
      expect(renderedIndexes(view).length).toBeGreaterThan(0),
    );
    expect(renderedIndexes(view).includes(200)).toBe(false);

    await rerender({ target: "row-200" });
    await waitFor(() => expect(renderedIndexes(view).includes(200)).toBe(true));
    await waitFor(() => expect(container.scrollTo).toHaveBeenCalled());
  });

  it("reports every loaded row's virtual position for the rail", async () => {
    const container = fakeContainer();
    const messages = transcript(120);
    let tops: Record<string, number> = {};
    const { container: view } = render(VirtualFeedFixture, {
      props: {
        items: messages,
        container,
        initialRows: 8,
        onLayout: (layout: { tops: Record<string, number> }) => {
          tops = layout.tops;
        },
      },
    });
    await waitFor(() =>
      expect(renderedIndexes(view).length).toBeGreaterThan(0),
    );
    await tick();
    await waitFor(() => expect(Object.keys(tops).length).toBe(120));
    const values = Object.values(tops);
    expect(Math.min(...values)).toBeGreaterThanOrEqual(0);
    expect(Math.max(...values)).toBeLessThanOrEqual(100);
    expect(tops["row-0"]).toBeLessThan(tops["row-119"]);
  });
});
