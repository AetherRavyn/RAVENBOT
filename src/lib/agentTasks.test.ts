import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  agentTasks,
  isActiveTask,
  taskMatchesFilter,
  type AgentTask,
  type AgentTaskState,
} from "./agentTasks.svelte";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

const invokeMock = vi.mocked(invoke);

function task(id: string, state: AgentTaskState): AgentTask {
  return {
    id,
    owner_bot_id: "owner",
    target_bot_id: "target",
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
    instruction: "Do the thing",
    expected_result: "The thing is done",
    state,
    resources: [],
    depends_on: [],
    workspace: "/tmp/work",
    depth: 1,
    auto_allow: false,
    approval_policy: null,
    result: state === "completed" ? "done" : null,
    error: state === "failed" ? "provider exploded" : null,
    created_at: "2026-11-28T10:00:00Z",
    started_at: null,
    completed_at: null,
    updated_at: "2026-11-28T10:00:00Z",
  };
}

beforeEach(() => {
  agentTasks.items = [];
  agentTasks.error = null;
  invokeMock.mockReset();
});

afterEach(() => {
  agentTasks.items = [];
  agentTasks.error = null;
  vi.restoreAllMocks();
});

describe("agent task filters", () => {
  it("treats queued, starting and running as active", () => {
    expect(isActiveTask("queued")).toBe(true);
    expect(isActiveTask("starting")).toBe(true);
    expect(isActiveTask("running")).toBe(true);
    expect(isActiveTask("completed")).toBe(false);
    expect(isActiveTask("failed")).toBe(false);
    expect(isActiveTask("cancelled")).toBe(false);
  });

  it("splits failed from finished instead of hiding it", () => {
    expect(taskMatchesFilter(task("a", "failed"), "failed")).toBe(true);
    expect(taskMatchesFilter(task("a", "failed"), "finished")).toBe(false);
    expect(taskMatchesFilter(task("b", "cancelled"), "finished")).toBe(true);
    expect(taskMatchesFilter(task("c", "completed"), "active")).toBe(false);
    expect(taskMatchesFilter(task("d", "running"), "all")).toBe(true);
  });
});

describe("agentTasks store", () => {
  it("loads the fleet list and derives active/failed counts", async () => {
    invokeMock.mockResolvedValue([
      task("a", "queued"),
      task("b", "running"),
      task("c", "failed"),
      task("d", "completed"),
    ]);
    await agentTasks.refresh();
    expect(agentTasks.items).toHaveLength(4);
    expect(agentTasks.activeCount).toBe(2);
    expect(agentTasks.failedCount).toBe(1);
    expect(invokeMock).toHaveBeenCalledWith("list_agent_tasks", { limit: 200 });
    expect(agentTasks.error).toBeNull();
  });

  it("keeps the last list when a refresh fails", async () => {
    invokeMock.mockResolvedValueOnce([task("a", "running")]);
    await agentTasks.refresh();
    invokeMock.mockRejectedValueOnce(new Error("ipc down"));
    await agentTasks.refresh();
    expect(agentTasks.items.map((item) => item.id)).toEqual(["a"]);
    expect(agentTasks.error).toContain("ipc down");
  });

  it("cancels and refreshes the list", async () => {
    invokeMock
      .mockResolvedValueOnce([task("a", "running")])
      .mockResolvedValueOnce(true)
      .mockResolvedValueOnce([task("a", "cancelled")]);
    await agentTasks.refresh();
    expect(await agentTasks.cancel("a")).toBe(true);
    expect(invokeMock).toHaveBeenCalledWith("cancel_agent_task", { taskId: "a" });
    expect(agentTasks.items[0].state).toBe("cancelled");
  });

  it("reports a failed cancel without dropping the row", async () => {
    invokeMock.mockResolvedValueOnce([task("a", "queued")]);
    await agentTasks.refresh();
    invokeMock.mockRejectedValueOnce(new Error("not allowed"));
    expect(await agentTasks.cancel("a")).toBe(false);
    expect(agentTasks.items[0].state).toBe("queued");
    expect(agentTasks.error).toContain("not allowed");
  });
});
