// @vitest-environment jsdom
import { describe, it, expect, beforeEach } from "vitest";
import { render, cleanup, fireEvent, waitFor } from "@testing-library/svelte";
import TurnTrace from "./TurnTrace.svelte";
import { setLocale } from "$lib/i18n";
import type { ToolTrace, TraceSpan } from "$lib/types";

beforeEach(() => {
  cleanup();
  setLocale("en");
  const dialog = HTMLDialogElement.prototype as unknown as {
    showModal?: () => void;
    close?: () => void;
  };
  if (!dialog.showModal) {
    dialog.showModal = function showModal(this: HTMLDialogElement) {
      this.setAttribute("open", "");
    };
  }
  if (!dialog.close) {
    dialog.close = function close(this: HTMLDialogElement) {
      this.removeAttribute("open");
    };
  }
});

const tools: ToolTrace[] = [
  {
    name: "file_read",
    arguments: { path: "app/main.py" },
    result: { success: true },
    is_error: false,
    duration_ms: 120,
  },
  {
    name: "shell_exec",
    arguments: { command: "pytest" },
    result: { success: false },
    is_error: true,
    duration_ms: 1380,
  },
];

describe("TurnTrace", () => {
  it("summarises the turn and opens a timeline plus tool details", async () => {
    const { container } = render(TurnTrace, {
      props: { tools, reasoning: "Checked the repository before editing." },
    });
    const head = container.querySelector(".turn-trace__head") as HTMLButtonElement;
    expect(head.textContent).toContain("3 steps");
    expect(head.textContent).toContain("1.5s");
    expect(head.textContent).toContain("1 failed");
    expect(head.getAttribute("aria-expanded")).toBe("false");

    await fireEvent.click(head);
    await waitFor(() =>
      expect(container.querySelectorAll(".turn-trace__bar-row").length).toBe(2),
    );
    expect(container.querySelector(".turn-trace__reasoning")?.textContent).toContain(
      "Checked the repository",
    );
    expect(container.textContent).toContain("file_read");
    expect(container.textContent).toContain("shell_exec");
  });

  it("draws model, tool, and memory spans on one timeline", async () => {
    const spans: TraceSpan[] = [
      { id: "m1", kind: "model", label: "Model round 1", started_ms: 0, duration_ms: 1200 },
      { id: "t1", kind: "tool", label: "file_read", started_ms: 1200, duration_ms: 120 },
      {
        id: "mem1",
        kind: "memory",
        label: "Memory saved",
        started_ms: 1400,
        duration_ms: 40,
        failed: true,
        detail: "The office uses the staging database.",
      },
    ];
    const { container } = render(TurnTrace, { props: { tools, spans } });
    expect(container.querySelector(".turn-trace__head")?.textContent).toContain("3 steps");
    await fireEvent.click(container.querySelector(".turn-trace__head")!);
    await waitFor(() =>
      expect(container.querySelectorAll(".turn-trace__bar-row").length).toBe(3),
    );
    expect(container.querySelector('[data-kind="model"]')?.textContent).toContain("Model round 1");
    expect(container.querySelector('[data-kind="memory"]')?.getAttribute("data-failed")).toBe("true");
    expect(container.querySelector('[data-kind="memory"]')?.textContent).toContain(
      "staging database",
    );
  });

  it("opens the full trace dialog with the same spans", async () => {
    const spans: TraceSpan[] = [
      { id: "m1", kind: "model", label: "Model round 1", started_ms: 0, duration_ms: 1200 },
      { id: "t1", kind: "tool", label: "file_read", started_ms: 1200, duration_ms: 120 },
      { id: "c1", kind: "context", label: "Context compacted", started_ms: 1400, duration_ms: 1 },
    ];
    const { container } = render(TurnTrace, { props: { tools, spans } });
    await fireEvent.click(container.querySelector(".turn-trace__head")!);
    await waitFor(() => expect(container.querySelector(".turn-trace__open")).not.toBeNull());
    await fireEvent.click(container.querySelector(".turn-trace__open")!);
    await waitFor(() => expect(container.querySelector(".trace-waterfall")?.hasAttribute("open")).toBe(true));
    expect(container.querySelectorAll(".trace-waterfall__row").length).toBe(3);
    expect(container.querySelector(".trace-waterfall")?.textContent).toContain("Model round 1");
  });

  it("renders nothing for a turn with no work to trace", () => {
    const { container } = render(TurnTrace, { props: { tools: [] } });
    expect(container.querySelector(".turn-trace")).toBeNull();
  });
});
