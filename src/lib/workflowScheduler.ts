/**
 * Due-workflow and inbound-trigger runner.
 *
 * The backend owns schedules and webhook routing. This runner executes claimed
 * or triggered DAGs through the same `execute_graph` command as the plan
 * canvas, then closes the workflow-run row. It only runs while the desktop UI
 * is alive; there is no separate daemon.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { notify } from "$lib/toast";
import { t } from "$lib/i18n";
import { workflowRunOutcome } from "$lib/workflowRuns";

interface WorkflowTask {
  bot_id: string;
  label: string;
  depends_on: number[];
  condition?: {
    source: number;
    operator: string;
    value: string;
  } | null;
  data_from?: number[] | null;
  retries?: number;
  output_schema?: string | null;
  loop_iterations?: number;
  loop_until?: {
    operator: string;
    value: string;
  } | null;
  loop_group?: string | null;
  loop_group_terminal?: boolean;
  loop_group_iterations?: number;
  loop_group_stop?: {
    operator: string;
    value: string;
  } | null;
  node_type?: string;
}

interface ClaimedWorkflow {
  run_id: string;
  workflow_id: string;
  name: string;
  version: number;
  goal: string;
  tasks: WorkflowTask[];
  office_id: string;
  next_run_at: string;
}

interface WorkflowRecord {
  id: string;
  name: string;
  goal: string;
  tasks: WorkflowTask[];
  office_id: string | null;
  version: number;
}

interface WorkflowRunJob {
  run_id: string;
  workflow_id: string;
  name: string;
  goal: string;
  tasks: WorkflowTask[];
  office_id: string;
}

type RunSource = "scheduled" | "triggered";

let timer: ReturnType<typeof setInterval> | null = null;
let triggerUnlisten: UnlistenFn | null = null;
let running = false;
const triggerQueue: string[] = [];

async function executeRun(job: WorkflowRunJob, source: RunSource) {
  try {
    const result = await invoke<any>("execute_graph", {
      orchestratorBotId: "00000000-0000-0000-0000-000000000000",
      goal: job.goal,
      tasks: job.tasks.map((task) => ({
        bot_id: task.bot_id,
        instruction: `Goal: ${job.goal}\n\nYour task: ${task.label}`,
        depends_on: task.depends_on,
        condition: task.condition ?? null,
        data_from: task.data_from ?? null,
        retries: task.retries ?? 0,
        output_schema: task.output_schema ?? null,
        loop_iterations: task.loop_iterations ?? 1,
        loop_until: task.loop_until ?? null,
        loop_group: task.loop_group ?? null,
        loop_group_terminal: task.loop_group_terminal ?? false,
        loop_group_iterations: task.loop_group_iterations ?? 2,
        loop_group_stop: task.loop_group_stop ?? null,
        node_type: task.node_type ?? "agent",
      })),
      chatroomId: job.office_id,
    });
    const outcome = workflowRunOutcome(result);
    const failed = outcome.stateName === "failed";
    await invoke("finish_workflow_run", {
      runId: job.run_id,
      stateName: outcome.stateName,
      threadId: result?.thread_id ?? null,
      detail: outcome.detail,
    }).catch(() => undefined);
    if (failed) {
      if (source === "scheduled") {
        await invoke("mark_workflow_schedule_error", {
          workflowId: job.workflow_id,
          error: outcome.detail,
        }).catch(() => undefined);
      }
      notify(
        t(
          source === "scheduled"
            ? "workflows.scheduledFailed"
            : "workflows.triggeredFailed",
          { name: job.name },
        ),
        "error",
      );
      return;
    }
    notify(
      t(
        source === "scheduled"
          ? "workflows.scheduledCompleted"
          : "workflows.triggeredCompleted",
        { name: job.name },
      ),
      "success",
    );
  } catch (error) {
    await invoke("finish_workflow_run", {
      runId: job.run_id,
      stateName: "failed",
      threadId: null,
      detail: String(error),
    }).catch(() => undefined);
    if (source === "scheduled") {
      await invoke("mark_workflow_schedule_error", {
        workflowId: job.workflow_id,
        error: String(error),
      }).catch(() => undefined);
    }
    notify(
      t(
        source === "scheduled"
          ? "workflows.scheduledFailed"
          : "workflows.triggeredFailed",
        { name: job.name },
      ),
      "error",
    );
  }
}

async function tick() {
  if (running) return;
  running = true;
  try {
    const job = await invoke<ClaimedWorkflow | null>("claim_due_workflow");
    if (job) await executeRun(job, "scheduled");
  } catch {
    /* the scheduler retries on the next tick */
  } finally {
    running = false;
    if (triggerQueue.length > 0) void drainTriggers();
  }
}

async function runTriggered(workflowId: string) {
  const workflow = await invoke<WorkflowRecord | null>("get_workflow", {
    workflowId,
  });
  if (!workflow || !workflow.office_id) return;
  const runId = await invoke<string>("begin_workflow_run", {
    workflowId,
    workflowVersion: workflow.version,
    goal: workflow.goal,
  });
  await executeRun(
    {
      run_id: runId,
      workflow_id: workflow.id,
      name: workflow.name,
      goal: workflow.goal,
      tasks: workflow.tasks,
      office_id: workflow.office_id,
    },
    "triggered",
  );
}

async function drainTriggers() {
  if (running) return;
  while (triggerQueue.length > 0) {
    running = true;
    const workflowId = triggerQueue.shift();
    if (!workflowId) {
      running = false;
      continue;
    }
    try {
      await runTriggered(workflowId);
    } catch (error) {
      notify(t("workflows.triggeredFailed", { name: workflowId }), "error");
    } finally {
      running = false;
    }
  }
}

export function startWorkflowScheduler(): void {
  if (timer) return;
  timer = setInterval(() => void tick(), 60_000);
  // Run once at startup, not only on the first interval. An interrupted
  // workflow is re-queued by reconciliation, and waiting a full minute to
  // notice that is a restart that looks like nothing happened.
  void tick();
  void listen<{ workflow_id?: string }>("workflow-trigger", (event) => {
    const workflowId = event.payload?.workflow_id;
    if (!workflowId) return;
    triggerQueue.push(workflowId);
    void drainTriggers();
  })
    .then((unlisten) => {
      triggerUnlisten = unlisten;
    })
    .catch(() => undefined);
}

export function stopWorkflowScheduler(): void {
  if (timer) clearInterval(timer);
  timer = null;
  triggerQueue.length = 0;
  if (triggerUnlisten) {
    triggerUnlisten();
    triggerUnlisten = null;
  }
}
