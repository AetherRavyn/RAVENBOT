// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor, within } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import RunsPanel from "$lib/components/RunsPanel.svelte";
import { setLocale } from "$lib/i18n";
import { a11yViolations } from "$lib/test/a11y";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);

const completedRun = {
  id: "run-87654321",
  bot_id: "bot-1",
  thread_id: "thread-1",
  state: "completed",
  tokens_consumed: 42,
  cost_estimate: 0.001,
  created_at: "2026-11-17T11:00:00Z",
  updated_at: "2026-11-17T11:01:00Z",
  completed_at: "2026-11-17T11:01:00Z",
  outcome: null,
  actor_name: "swadhin",
  actor_role: "operator",
};

const emptyConflicts = { skipped: [], conflicts: [] };

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

  it("previews and reverts the files a run wrote", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "list_runs") return [completedRun];
      if (command === "preview_run_revert") {
        return {
          restored: ["/tmp/note.txt"],
          deleted: ["/tmp/created.txt"],
          ...emptyConflicts,
        };
      }
      if (command === "revert_run_files") {
        return {
          restored: ["/tmp/note.txt"],
          deleted: ["/tmp/created.txt"],
          ...emptyConflicts,
        };
      }
      throw new Error(`unexpected command ${command}`);
    });
    const { getByText, getByTitle } = render(RunsPanel);
    await waitFor(() => expect(getByText("completed")).toBeTruthy());

    await fireEvent.click(getByTitle("Revert this run"));
    await waitFor(() => expect(getByText("Revert this run")).toBeTruthy());
    const dialog = document.querySelector('[role="dialog"]') as HTMLElement;
    expect(dialog).toBeTruthy();
    await fireEvent.click(within(dialog).getByRole("button", { name: "Revert files" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("revert_run_files", {
        runId: completedRun.id,
        force: false,
      }),
    );
  });

  it("requires an explicit force when a newer write blocks the revert", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "list_runs") return [completedRun];
      if (command === "preview_run_revert") {
        return {
          restored: ["/tmp/note.txt"],
          deleted: [],
          skipped: [],
          conflicts: [
            { path: "/tmp/note.txt", reason: "A later run wrote this file." },
          ],
        };
      }
      if (command === "revert_run_files") {
        return {
          restored: ["/tmp/note.txt"],
          deleted: [],
          ...emptyConflicts,
        };
      }
      throw new Error(`unexpected command ${command}`);
    });
    const { getByText, getByTitle } = render(RunsPanel);
    await waitFor(() => expect(getByText("completed")).toBeTruthy());

    await fireEvent.click(getByTitle("Revert this run"));
    await waitFor(() =>
      expect(getByText("A later run wrote this file.")).toBeTruthy(),
    );
    const dialog = document.querySelector('[role="dialog"]') as HTMLElement;
    await fireEvent.click(within(dialog).getByRole("button", { name: "Revert anyway" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("revert_run_files", {
        runId: completedRun.id,
        force: true,
      }),
    );
  });

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

  it("shows which account started a run", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "list_runs") return [completedRun];
      throw new Error(`unexpected command ${command}`);
    });
    const { getByText } = render(RunsPanel);
    await waitFor(() => expect(getByText("by swadhin (operator)")).toBeTruthy());
  });
});
