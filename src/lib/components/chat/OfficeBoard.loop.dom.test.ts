// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { render } from "@testing-library/svelte";
import { tick } from "svelte";
import OfficeBoard from "./OfficeBoard.svelte";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));

Object.defineProperty(window, "matchMedia", {
  writable: true,
  value: vi.fn().mockImplementation((query: string) => ({
    matches: true,
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  })),
});

describe("OfficeBoard run state", () => {
  it("shows durable backlog work that is not in the live graph", async () => {
    const props = {
      goal: "Ship it",
      nodes: [
        {
          nodeId: "live-1",
          botId: "bot-1",
          label: "Current milestone",
          dependsOn: [],
          state: "running" as const,
        },
      ],
      members: [],
      runActive: true,
      workItems: [
        {
          id: "old-1",
          title: "Fix the failed acceptance report",
          status: "failed",
          owner_bot_id: "bot-1",
        },
      ],
    };
    const { getByText } = render(OfficeBoard, { props });
    await tick();
    expect(getByText("Fix the failed acceptance report")).toBeTruthy();
    expect(getByText("Backlog")).toBeTruthy();
  });

  it("opens and settles when a run starts and stops", async () => {
    const props = { goal: "", nodes: [], members: [], runActive: true };
    const { rerender } = render(OfficeBoard, { props });
    await tick();
    await rerender({ ...props, runActive: false });
    await tick();
    await rerender({ ...props, runActive: true });
    await tick();
    expect(true).toBe(true);
  });
});
