// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import OfficeTaskCard from "./OfficeTaskCard.svelte";
import { setLocale } from "$lib/i18n";
import type { BoardNode } from "./OfficeBoard.svelte";

const nodes: BoardNode[] = [
  { nodeId: "1", botId: "b1", label: "Write schema", dependsOn: [], state: "done", nodeThreadId: "t1" },
  { nodeId: "2", botId: "b2", label: "Build API", dependsOn: ["1"], state: "running", nodeThreadId: "t2" },
  { nodeId: "3", botId: "b2", label: "Test API", dependsOn: ["2"], state: "pending" },
];

beforeEach(() => {
  cleanup();
  setLocale("en");
});
afterEach(cleanup);

describe("OfficeTaskCard", () => {
  it("shows progress and the active task", () => {
    const { container, getByText } = render(OfficeTaskCard, {
      props: { nodes, nameFor: (id: string) => (id === "b1" ? "Ada" : "Sam") },
    });
    expect(container.textContent).toContain("1/3 done");
    expect(getByText("Build API")).toBeTruthy();
  });

  it("opens a node thread from the expanded list", async () => {
    const onOpen = vi.fn();
    const { container } = render(OfficeTaskCard, {
      props: { nodes, nameFor: () => "Ada", onOpen },
    });
    await fireEvent.click(container.querySelector(".office-task-card__head")!);
    const item = container.querySelector(".office-task-card__item[data-state='running']")!;
    await fireEvent.click(item);
    expect(onOpen).toHaveBeenCalledWith(nodes[1]);
  });
});
