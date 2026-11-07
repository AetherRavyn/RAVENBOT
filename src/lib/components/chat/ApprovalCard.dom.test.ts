// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import ApprovalCard from "./ApprovalCard.svelte";
import { setLocale } from "$lib/i18n";

describe("ApprovalCard", () => {
  beforeEach(() => {
    cleanup();
    setLocale("en");
  });
  afterEach(cleanup);

  it("shows the tool, the risk, and a readable argument summary", () => {
    const { container, getByText } = render(ApprovalCard, {
      props: {
        botName: "Scout",
        toolName: "file_write",
        toolLabel: "write a file",
        args: { path: "src/app.rs", content: "fn main() {}" },
        risk: "high",
        onDecide: vi.fn(),
      },
    });

    expect(getByText("Scout")).toBeTruthy();
    expect(getByText("High stakes")).toBeTruthy();
    expect(container.querySelector(".chat-interaction__summary")?.textContent).toContain(
      "src/app.rs (+12 chars)",
    );
    expect(container.querySelector(".chat-interaction__note")?.textContent).toContain(
      "High-stakes action",
    );
  });

  it("reports allow and deny as separate decisions", async () => {
    const onDecide = vi.fn();
    const { getByText } = render(ApprovalCard, {
      props: {
        botName: "Scout",
        toolName: "shell_exec",
        toolLabel: "run a command",
        args: { command: "cargo test" },
        risk: "low",
        onDecide,
      },
    });

    await fireEvent.click(getByText("Allow"));
    await fireEvent.click(getByText("Deny"));
    expect(onDecide.mock.calls).toEqual([[true], [false]]);
  });

  it("disables both decisions while the run is being resumed", () => {
    const { getByText } = render(ApprovalCard, {
      props: {
        toolName: "shell_exec",
        deciding: true,
        onDecide: vi.fn(),
      },
    });
    expect((getByText("Allowing…").closest("button") as HTMLButtonElement).disabled).toBe(true);
    expect((getByText("Deny").closest("button") as HTMLButtonElement).disabled).toBe(true);
  });
});
