// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { invoke, type InvokeArgs } from "@tauri-apps/api/core";
import TasksView from "./TasksView.svelte";
import { agentTasks, type AgentTask, type AgentTaskState } from "$lib/agentTasks.svelte";
import { workspace } from "$lib/workspace.svelte";
import { sessionRole } from "$lib/sessionRole.svelte";
import { setLocale } from "$lib/i18n";
import { a11yViolations } from "$lib/test/a11y";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

const invokeMock = vi.mocked(invoke);
let tasks: AgentTask[] = [];

function task(id: string, state: AgentTaskState, overrides: Partial<AgentTask> = {}): AgentTask {
  return {
    id,
    owner_bot_id: "bot-1",
    target_bot_id: "bot-2",
    owner_thread_id: "thread-owner",
    owner_run_id: null,
    actor_id: null,
    actor_name: null,
    actor_role: null,
    actor_workspace_id: null,
    thread_id: `thread-${id}`,
    run_id: null,
    office_id: null,
    title: `Task ${id}`,
    instruction: "Read the logs and report the first error.",
    expected_result: "One error and its path.",
    state,
    resources: [],
    depends_on: [],
    workspace: "/tmp/work",
    depth: 1,
    auto_allow: false,
    approval_policy: null,
    result: state === "completed" ? "The error is in worker.rs:42." : null,
    error: state === "failed" ? "provider exploded" : null,
    created_at: "2026-11-28T10:00:00Z",
    started_at: null,
    completed_at: null,
    updated_at: "2026-11-28T10:00:00Z",
    ...overrides,
  };
}

describe("TasksView", () => {
  beforeEach(() => {
    cleanup();
    setLocale("en");
    workspace.dest = "home";
    workspace.selectedBotId = null;
    workspace.selectedRoomId = null;
    sessionRole.role = "owner";
    workspace.bots = [
      { id: "bot-1", name: "Atlas", avatar_style: "raven-native" },
      { id: "bot-2", name: "Nova", avatar_style: "raven-native" },
    ];
    tasks = [task("a", "running"), task("b", "failed")];
    agentTasks.items = [...tasks];
    agentTasks.error = null;
    invokeMock.mockReset();
    invokeMock.mockImplementation(async (command: string, args?: InvokeArgs) => {
      if (command === "list_agent_tasks") return tasks;
      if (command === "cancel_agent_task") {
        const id = String((args as Record<string, unknown> | undefined)?.taskId);
        tasks = tasks.map((item) =>
          item.id === id ? { ...item, state: "cancelled" as const } : item,
        );
        return true;
      }
      return null;
    });
  });

  afterEach(() => {
    cleanup();
    agentTasks.items = [];
    agentTasks.error = null;
    workspace.dest = "home";
    workspace.selectedBotId = null;
    workspace.bots = [];
    sessionRole.role = "owner";
    vi.restoreAllMocks();
  });

  it("lists cross-agent tasks with state, owner, target and result", async () => {
    const { container } = render(TasksView);
    await waitFor(() => expect(container.textContent).toContain("Task a"));

    expect(container.textContent).toContain("Task b");
    expect(container.textContent).toContain("Running");
    expect(container.textContent).toContain("Failed");
    expect(container.textContent).toContain("Nova");
    expect(container.textContent).toContain("from Atlas");
    expect(container.textContent).toContain("provider exploded");
    expect(container.querySelectorAll("[data-task-state]")).toHaveLength(2);
    expect(await a11yViolations(container)).toEqual([]);
  });

  it("filters to failed tasks", async () => {
    const { container } = render(TasksView);
    await waitFor(() => expect(container.textContent).toContain("Task a"));
    await fireEvent.click(screen.getByRole("button", { name: /^Failed/ }));
    expect(container.textContent).toContain("Task b");
    expect(container.textContent).not.toContain("Task a");
  });

  it("opens the task's target thread", async () => {
    const { container } = render(TasksView);
    await waitFor(() => expect(container.textContent).toContain("Task a"));
    await fireEvent.click(screen.getByRole("button", { name: "Open — Task a" }));
    expect(workspace.selectedBotId).toBe("bot-2");
    expect(workspace.dest).toBe("agents");
  });

  it("cancels an active task and refreshes its row", async () => {
    const { container } = render(TasksView);
    await waitFor(() => expect(container.textContent).toContain("Task a"));
    await fireEvent.click(screen.getByRole("button", { name: "Cancel — Task a" }));
    await waitFor(() => expect(agentTasks.items[0].state).toBe("cancelled"));
    expect(invokeMock).toHaveBeenCalledWith("cancel_agent_task", { taskId: "a" });
    expect(container.querySelector("[data-task-state='cancelled']")).not.toBeNull();
  });

  it("does not offer cancel to a read-only viewer", async () => {
    sessionRole.role = "viewer";
    const { container } = render(TasksView);
    await waitFor(() => expect(container.textContent).toContain("Task a"));
    const cancel = screen.getByRole("button", { name: "Cancel — Task a" }) as HTMLButtonElement;
    expect(cancel.disabled).toBe(true);
  });

  it("shows the empty state when a filter has no rows", async () => {
    tasks = [task("a", "running")];
    agentTasks.items = [...tasks];
    const { container } = render(TasksView);
    await waitFor(() => expect(container.textContent).toContain("Task a"));
    await fireEvent.click(screen.getByRole("button", { name: /^Failed/ }));
    expect(container.textContent).toContain("No tasks yet.");
  });
});
