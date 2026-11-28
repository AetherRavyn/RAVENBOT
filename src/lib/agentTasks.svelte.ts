/**
 * Cross-agent durable tasks.
 *
 * The task loop is already the source of truth; this store is only the UI
 * projection. It listens to `agent-task-changed` rather than polling, keeps the
 * last list when a refresh fails, and exposes the counts the rail and Tasks view
 * share so a badge cannot disagree with the page it points at.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type AgentTaskState =
  | "queued"
  | "starting"
  | "running"
  | "completed"
  | "failed"
  | "cancelled";

export interface AgentTask {
  id: string;
  owner_bot_id: string;
  target_bot_id: string;
  owner_thread_id: string | null;
  owner_run_id: string | null;
  actor_id: string | null;
  actor_name: string | null;
  actor_role: string | null;
  actor_workspace_id: string | null;
  thread_id: string | null;
  run_id: string | null;
  office_id: string | null;
  title: string;
  instruction: string;
  expected_result: string;
  state: AgentTaskState;
  resources: string[];
  depends_on: string[];
  workspace: string | null;
  depth: number;
  auto_allow: boolean;
  approval_policy: string | null;
  result: string | null;
  error: string | null;
  created_at: string;
  started_at: string | null;
  completed_at: string | null;
  updated_at: string;
}

export type TaskFilter = "all" | "active" | "failed" | "finished";

/** Task states that are not yet over. */
export function isActiveTask(state: AgentTaskState): boolean {
  return state === "queued" || state === "starting" || state === "running";
}

/** Filter one task without hiding a failed row behind a generic "finished". */
export function taskMatchesFilter(task: AgentTask, filter: TaskFilter): boolean {
  if (filter === "all") return true;
  if (filter === "active") return isActiveTask(task.state);
  if (filter === "failed") return task.state === "failed";
  return task.state === "completed" || task.state === "cancelled";
}

export function taskStateKey(state: AgentTaskState): `tasks.state.${AgentTaskState}` {
  return `tasks.state.${state}`;
}

class AgentTasks {
  items = $state<AgentTask[]>([]);
  loading = $state(false);
  error = $state<string | null>(null);
  /** Counts are derived here so the rail badge and the page cannot drift. */
  activeCount = $derived(this.items.filter((task) => isActiveTask(task.state)).length);
  failedCount = $derived(this.items.filter((task) => task.state === "failed").length);

  private unlisten: UnlistenFn | null = null;
  private refs = 0;
  private listening = false;
  private refreshTimer: ReturnType<typeof setTimeout> | null = null;

  async refresh(limit = 200): Promise<void> {
    this.loading = true;
    try {
      const next = await invoke<AgentTask[]>("list_agent_tasks", { limit });
      this.items = Array.isArray(next) ? next : [];
      this.error = null;
    } catch (error) {
      // Keep the last list: a transient IPC failure should not make queued work
      // disappear from the only page that shows it.
      this.error = String(error);
    } finally {
      this.loading = false;
    }
  }

  async cancel(taskId: string): Promise<boolean> {
    try {
      const cancelled = await invoke<boolean>("cancel_agent_task", { taskId });
      await this.refresh();
      return Boolean(cancelled);
    } catch (error) {
      this.error = String(error);
      return false;
    }
  }

  /** Subscribe to the task loop's change stream; refcounted like the fleet. */
  async start(): Promise<void> {
    this.refs += 1;
    if (this.unlisten || this.listening) return;
    this.listening = true;
    try {
      const unlisten = await listen("agent-task-changed", () => this.scheduleRefresh());
      if (this.refs === 0) {
        unlisten();
        return;
      }
      this.unlisten = unlisten;
    } catch {
      // The Tasks view still works through its explicit Refresh button.
    } finally {
      this.listening = false;
    }
    await this.refresh();
  }

  stop(): void {
    this.refs = Math.max(0, this.refs - 1);
    if (this.refs > 0) return;
    this.unlisten?.();
    this.unlisten = null;
    if (this.refreshTimer) clearTimeout(this.refreshTimer);
    this.refreshTimer = null;
  }

  private scheduleRefresh(): void {
    if (this.refreshTimer) clearTimeout(this.refreshTimer);
    this.refreshTimer = setTimeout(() => {
      this.refreshTimer = null;
      void this.refresh();
    }, 250);
  }
}

export const agentTasks = new AgentTasks();
