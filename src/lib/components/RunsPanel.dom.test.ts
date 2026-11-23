// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import RunsPanel from "$lib/components/RunsPanel.svelte";
import { setLocale } from "$lib/i18n";
import { a11yViolations } from "$lib/test/a11y";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);

const pausedRun = {
  id: "run-12345678",
  bot_id: "bot-1",
  thread_id: "thread-1",
  state: "paused",
  tokens_consumed: 42,
  cost_estimate: 0.001,
  created_at: "2026-11-17T10:00:00Z",
  updated_at: "2026-11-17T10:01:00Z",
  completed_at: null,
  outcome: null,
};

describe("RunsPanel", () => {
  let listCalls = 0;

  beforeEach(() => {
    cleanup();
    setLocale("en");
    listCalls = 0;
    invokeMock.mockReset();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "list_runs") {
        listCalls += 1;
        return listCalls === 1 ? [pausedRun] : [{ ...pausedRun, state: "completed" }];
      }
      if (command === "resume_run") return null;
      throw new Error(`unexpected command ${command}`);
    });
  });

  afterEach(cleanup);

  it("resumes a paused run from its checkpoint", async () => {
    const { getByLabelText, getByText, container } = render(RunsPanel);
    await waitFor(() => expect(getByText("paused")).toBeTruthy());
    expect(await a11yViolations(container)).toEqual([]);

    await fireEvent.click(getByLabelText("Resume"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("resume_run", {
        runId: "run-12345678",
      }),
    );
    await waitFor(() => expect(getByText("completed")).toBeTruthy());
    expect(listCalls).toBe(2);
  });
});
