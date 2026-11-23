<script lang="ts">
  /**
   * Reusable workflow templates for the office plan canvas.
   *
   * A template is the current goal, tasks and dependency edges. Loading one
   * fills the existing plan editor; dispatch still goes through the normal
   * execute_graph path, so there is no second workflow engine. Every save
   * captures a version and every dispatch can append a run record.
   */
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import {
    CalendarClock,
    FolderOpen,
    GitBranch,
    History,
    Loader2,
    Webhook,
    Play,
    RefreshCw,
    Save,
    Trash2,
  } from "@lucide/svelte";

  interface WorkflowCondition {
    source: number;
    operator: string;
    value: string;
  }

  interface WorkflowLoopStop {
    operator: string;
    value: string;
  }

  interface PlanTask {
    id: string;
    botId: string;
    label: string;
    dependsOn: number[];
    condition: WorkflowCondition | null;
    dataFrom: number[] | null;
    retries: number;
    outputSchema: string;
    loopIterations: number;
    loopUntil: WorkflowLoopStop | null;
    loopGroup: string;
    loopGroupTerminal: boolean;
    loopGroupIterations: number;
    loopGroupStop: WorkflowLoopStop | null;
    nodeType: string;
  }

  interface WorkflowTaskRecord {
    bot_id: string;
    label: string;
    depends_on: number[];
    condition?: WorkflowCondition | null;
    data_from?: number[] | null;
    retries?: number;
    output_schema?: string | null;
    loop_iterations?: number;
    loop_until?: WorkflowLoopStop | null;
    loop_group?: string | null;
    loop_group_terminal?: boolean;
    loop_group_iterations?: number;
    loop_group_stop?: WorkflowLoopStop | null;
    node_type?: string;
  }

  interface WorkflowRecord {
    id: string;
    name: string;
    goal: string;
    tasks: WorkflowTaskRecord[];
    office_id: string | null;
    version: number;
    created_at: string;
    updated_at: string;
  }

  interface WorkflowVersion {
    id: string;
    workflow_id: string;
    version: number;
    name: string;
    goal: string;
    tasks: WorkflowTaskRecord[];
    created_at: string;
  }

  interface WorkflowSchedule {
    workflow_id: string;
    interval_hours: number;
    enabled: boolean;
    next_run_at: string | null;
    last_run_at: string | null;
    last_error: string | null;
    updated_at: string;
  }

  interface WorkflowWebhookStatus {
    enabled: boolean;
    base_url: string;
  }

  interface WorkflowWebhookCreated {
    url: string;
    secret: string;
    header: string;
  }

  interface WorkflowRun {
    id: string;
    workflow_id: string;
    workflow_version: number;
    thread_id: string | null;
    goal: string;
    state: string;
    started_at: string;
    finished_at: string | null;
    detail: string | null;
  }

  interface WorkflowMeta {
    id: string;
    version: number;
    name: string;
  }

  interface Props {
    officeId: string;
    members?: any[];
    goal?: string;
    tasks?: PlanTask[];
    onLoad?: (goal: string, tasks: PlanTask[], meta: WorkflowMeta) => void;
    onRun?: (goal: string, tasks: PlanTask[], meta: WorkflowMeta) => void;
  }

  let {
    officeId,
    members = [],
    goal = "",
    tasks = [],
    onLoad,
    onRun,
  }: Props = $props();

  let open = $state(false);
  let showSave = $state(false);
  let name = $state("");
  let workflows = $state<WorkflowRecord[]>([]);
  let loading = $state(false);
  let saving = $state(false);
  let error = $state("");
  let notice = $state("");
  let detailId = $state<string | null>(null);
  let detailMode = $state<
    "versions" | "runs" | "schedule" | "trigger" | null
  >(null);
  let schedule = $state<WorkflowSchedule | null>(null);
  let scheduleInterval = $state("0");
  let scheduleSaving = $state(false);
  let webhookStatus = $state<WorkflowWebhookStatus | null>(null);
  let webhookCreated = $state<WorkflowWebhookCreated | null>(null);
  let webhookBusy = $state(false);
  let webhookError = $state("");
  let webhookCopied = $state(false);
  let versions = $state<WorkflowVersion[]>([]);
  let runs = $state<WorkflowRun[]>([]);
  let detailLoading = $state(false);

  function memberBotId(member: any): string {
    return String(member?.bot?.id ?? member?.bot_id ?? "");
  }

  function mappedTasks(record: { tasks: WorkflowTaskRecord[] }): {
    tasks: PlanTask[];
    missing: boolean;
  } {
    const available = new Set(members.map(memberBotId));
    const fallback = members.length > 0 ? memberBotId(members[0]) : "";
    let missing = false;
    const mapped = record.tasks.map((task, index) => {
      let botId = task.bot_id;
      if (!available.has(botId)) {
        missing = true;
        botId = fallback || botId;
      }
      return {
        id: String(index),
        botId,
        label: task.label,
        dependsOn: Array.isArray(task.depends_on) ? task.depends_on : [],
        condition: task.condition ?? null,
        dataFrom: task.data_from ?? null,
        retries: task.retries ?? 0,
        outputSchema: task.output_schema ?? "",
        loopIterations: task.loop_iterations ?? 1,
        loopUntil: task.loop_until ?? null,
        loopGroup: task.loop_group ?? "",
        loopGroupTerminal: Boolean(task.loop_group_terminal),
        loopGroupIterations: task.loop_group_iterations ?? 2,
        loopGroupStop: task.loop_group_stop ?? null,
        nodeType: task.node_type ?? "agent",
      };
    });
    return { tasks: mapped, missing };
  }

  async function loadWorkflows() {
    loading = true;
    error = "";
    try {
      workflows = await invoke<WorkflowRecord[]>("list_workflows", {
        officeId,
      });
    } catch (cause) {
      error = String(cause);
    } finally {
      loading = false;
    }
  }

  function toggleList() {
    open = !open;
    if (open) void loadWorkflows();
  }

  async function saveCurrent() {
    if (saving) return;
    if (!name.trim()) {
      error = t("workflows.nameRequired");
      return;
    }
    if (!goal.trim() || tasks.length === 0) {
      error = t("workflows.emptyPlan");
      return;
    }
    saving = true;
    error = "";
    notice = "";
    try {
      const savedName = name.trim();
      const saved = await invoke<{ id: string; version: number }>(
        "save_workflow",
        {
          id: null,
          name: savedName,
          goal: goal.trim(),
          tasks: tasks.map((task) => ({
            botId: task.botId,
            label: task.label,
            dependsOn: task.dependsOn,
            condition: task.condition ?? null,
            dataFrom: task.dataFrom,
            retries: task.retries,
            outputSchema: task.outputSchema.trim()
              ? task.outputSchema
              : null,
            loopIterations: task.loopIterations,
            loopUntil:
              task.loopIterations > 1 && task.loopUntil?.value.trim()
                ? task.loopUntil
                : null,
            loopGroup: task.loopGroup.trim() || null,
            loopGroupTerminal: Boolean(task.loopGroupTerminal),
            loopGroupIterations: task.loopGroupIterations,
            loopGroupStop:
              task.loopGroupTerminal &&
              task.loopGroupIterations > 1 &&
              task.loopGroupStop?.value.trim()
                ? task.loopGroupStop
                : null,
            nodeType: task.nodeType,
          })),
          officeId,
        },
      );
      notice = t("workflows.savedVersion", { version: saved.version });
      showSave = false;
      name = "";
      // The current canvas is now this saved version; a later dispatch should
      // record against the version the user just created.
      onLoad?.(goal, tasks, {
        id: saved.id,
        version: saved.version,
        name: savedName,
      });
      if (open) await loadWorkflows();
    } catch (cause) {
      error = String(cause);
    } finally {
      saving = false;
    }
  }

  function loadWorkflow(workflow: WorkflowRecord | WorkflowVersion) {
    const mapped = mappedTasks(workflow);
    onLoad?.(workflow.goal, mapped.tasks, {
      id: "workflow_id" in workflow ? workflow.workflow_id : workflow.id,
      version: workflow.version,
      name: workflow.name,
    });
    open = false;
    detailId = null;
    detailMode = null;
    notice = mapped.missing
      ? t("workflows.missingAgents")
      : t("workflows.loaded");
  }

  function runWorkflow(workflow: WorkflowRecord | WorkflowVersion) {
    if (!onRun) return;
    if (!confirm(t("workflows.runConfirm", { name: workflow.name }))) return;
    const mapped = mappedTasks(workflow);
    onRun(workflow.goal, mapped.tasks, {
      id: "workflow_id" in workflow ? workflow.workflow_id : workflow.id,
      version: workflow.version,
      name: workflow.name,
    });
    open = false;
    detailId = null;
    detailMode = null;
  }

  async function toggleDetail(
    workflow: WorkflowRecord,
    mode: "versions" | "runs" | "schedule" | "trigger",
  ) {
    if (detailId === workflow.id && detailMode === mode) {
      detailId = null;
      detailMode = null;
      return;
    }
    detailId = workflow.id;
    detailMode = mode;
    detailLoading = true;
    error = "";
    try {
      if (mode === "versions") {
        versions = await invoke<WorkflowVersion[]>("list_workflow_versions", {
          workflowId: workflow.id,
          limit: 20,
        });
      } else if (mode === "runs") {
        runs = await invoke<WorkflowRun[]>("list_workflow_runs", {
          workflowId: workflow.id,
          limit: 20,
        });
      } else if (mode === "schedule") {
        schedule = await invoke<WorkflowSchedule | null>(
          "get_workflow_schedule",
          { workflowId: workflow.id },
        );
        scheduleInterval =
          schedule?.enabled && schedule.interval_hours
            ? String(schedule.interval_hours)
            : "0";
      } else {
        webhookStatus = await invoke<WorkflowWebhookStatus>(
          "get_workflow_webhook_status",
          { workflowId: workflow.id },
        );
        webhookCreated = null;
        webhookError = "";
      }
    } catch (cause) {
      error = String(cause);
    } finally {
      detailLoading = false;
    }
  }

  async function enableWebhook(workflow: WorkflowRecord) {
    if (webhookBusy) return;
    webhookBusy = true;
    webhookError = "";
    notice = "";
    try {
      webhookCreated = await invoke<WorkflowWebhookCreated>(
        "enable_workflow_webhook",
        { workflowId: workflow.id },
      );
      webhookStatus = await invoke<WorkflowWebhookStatus>(
        "get_workflow_webhook_status",
        { workflowId: workflow.id },
      );
      webhookCopied = false;
    } catch (cause) {
      webhookError = String(cause);
    } finally {
      webhookBusy = false;
    }
  }

  async function disableWebhook(workflow: WorkflowRecord) {
    if (webhookBusy) return;
    webhookBusy = true;
    webhookError = "";
    notice = "";
    try {
      await invoke("disable_workflow_webhook", { workflowId: workflow.id });
      webhookStatus = await invoke<WorkflowWebhookStatus>(
        "get_workflow_webhook_status",
        { workflowId: workflow.id },
      );
      webhookCreated = null;
    } catch (cause) {
      webhookError = String(cause);
    } finally {
      webhookBusy = false;
    }
  }

  async function copyWebhook() {
    if (!webhookCreated) return;
    try {
      await navigator.clipboard.writeText(webhookCreated.url);
      webhookCopied = true;
      setTimeout(() => (webhookCopied = false), 1500);
    } catch (cause) {
      webhookError = String(cause);
    }
  }

  async function saveSchedule(workflow: WorkflowRecord) {
    if (scheduleSaving) return;
    scheduleSaving = true;
    error = "";
    notice = "";
    try {
      schedule = await invoke<WorkflowSchedule | null>(
        "set_workflow_schedule",
        {
          workflowId: workflow.id,
          intervalHours: Number(scheduleInterval),
        },
      );
      notice = t("workflows.scheduleSaved");
      await loadWorkflows();
    } catch (cause) {
      error = String(cause);
    } finally {
      scheduleSaving = false;
    }
  }

  async function deleteWorkflow(workflow: WorkflowRecord) {
    if (!confirm(t("workflows.deleteConfirm", { name: workflow.name }))) return;
    try {
      await invoke("delete_workflow", { id: workflow.id });
      notice = t("workflows.deleted");
      await loadWorkflows();
    } catch (cause) {
      error = String(cause);
    }
  }

  function fmtTime(value: string): string {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return "";
    return date.toLocaleString(undefined, {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  }

  function runLabel(state: string): string {
    if (state === "running") return t("workflows.runRunning");
    if (state === "completed") return t("workflows.runCompleted");
    if (state === "failed") return t("workflows.runFailed");
    return t("workflows.runCancelled");
  }
</script>

<div class="space-y-2">
  <div class="flex flex-wrap items-center gap-1.5">
    <button
      type="button"
      class="inline-flex h-7 items-center gap-1.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-2.5 text-[11px] font-semibold text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] cursor-pointer disabled:opacity-50"
      onclick={() => {
        showSave = !showSave;
        error = "";
        notice = "";
      }}
      disabled={!goal.trim() || tasks.length === 0}
    >
      <Save class="size-3.5" />
      {t("workflows.saveCurrent")}
    </button>
    <button
      type="button"
      class="inline-flex h-7 items-center gap-1.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-2.5 text-[11px] font-semibold text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] cursor-pointer"
      onclick={toggleList}
      aria-expanded={open}
    >
      <FolderOpen class="size-3.5" />
      {t("workflows.open")}
    </button>
  </div>

  {#if showSave}
    <div class="space-y-2 rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] p-3">
      <p class="text-[11px] leading-4 text-[var(--text-tertiary)]">
        {t("workflows.hint")}
      </p>
      <input
        class="chat-question__input"
        bind:value={name}
        maxlength={120}
        placeholder={t("workflows.namePlaceholder")}
        aria-label={t("workflows.name")}
        disabled={saving}
      />
      <div class="flex justify-end gap-2">
        <button
          type="button"
          class="browser-secret__cancel"
          onclick={() => (showSave = false)}
          disabled={saving}
        >{t("ui.cancel")}</button>
        <button
          type="button"
          class="browser-secret__submit"
          onclick={() => void saveCurrent()}
          disabled={saving || !name.trim()}
        >
          {#if saving}
            <Loader2 class="size-3.5 animate-spin" />
          {/if}
          {saving ? t("workflows.saving") : t("workflows.save")}
        </button>
      </div>
    </div>
  {/if}

  {#if open}
    <div class="space-y-2 rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] p-3">
      <div class="flex items-center justify-between gap-2">
        <p class="text-[11px] font-semibold text-[var(--text-primary)]">
          {t("workflows.title")}
        </p>
        <button
          type="button"
          class="icon-btn size-6"
          onclick={() => void loadWorkflows()}
          disabled={loading}
          title={t("workflows.refresh")}
          aria-label={t("workflows.refresh")}
        >
          {#if loading}
            <Loader2 class="size-3.5 animate-spin" />
          {:else}
            <RefreshCw class="size-3.5" />
          {/if}
        </button>
      </div>
      {#if loading && workflows.length === 0}
        <p class="text-[11px] text-[var(--text-muted)]" role="status">
          {t("ui.loading")}
        </p>
      {:else if workflows.length === 0}
        <p class="text-[11px] text-[var(--text-muted)]">{t("workflows.empty")}</p>
      {:else}
        <div class="max-h-64 space-y-1.5 overflow-y-auto no-scrollbar">
          {#each workflows as workflow (workflow.id)}
            <div class="rounded-lg border border-[var(--hairline)] bg-[var(--surface-0)] p-2">
              <div class="flex min-w-0 items-start gap-2">
                <div class="min-w-0 flex-1">
                  <p class="truncate text-[11px] font-semibold text-[var(--text-primary)]">
                    {workflow.name}
                  </p>
                  <p class="line-clamp-2 text-[11px] leading-4 text-[var(--text-tertiary)]">
                    {workflow.goal}
                  </p>
                  <p class="mt-0.5 font-mono text-[11px] text-[var(--text-muted)]">
                    v{workflow.version} · {t("workflows.tasks", { count: workflow.tasks.length })} ·
                    {t("workflows.updated", { time: fmtTime(workflow.updated_at) })}
                  </p>
                </div>
                <div class="flex shrink-0 items-center gap-1">
                  {#if onRun}
                    <button
                      type="button"
                      class="icon-btn size-6 text-[var(--brand-text)]"
                      onclick={() => runWorkflow(workflow)}
                      title={t("workflows.run")}
                      aria-label={t("workflows.run")}
                    >
                      <Play class="size-3.5" />
                    </button>
                  {/if}
                  <button
                    type="button"
                    class="inline-flex h-6 items-center rounded-md border border-[var(--brand)]/40 bg-[var(--brand-soft)] px-2 text-[11px] font-semibold text-[var(--brand-text)] cursor-pointer"
                    onclick={() => loadWorkflow(workflow)}
                  >{t("workflows.load")}</button>
                  <button
                    type="button"
                    class="icon-btn size-6 text-[var(--text-muted)]"
                    onclick={() => void toggleDetail(workflow, "trigger")}
                    title={t("workflows.trigger")}
                    aria-label={t("workflows.trigger")}
                  >
                    <Webhook class="size-3.5" />
                  </button>
                  <button
                    type="button"
                    class="icon-btn size-6 text-[var(--text-muted)]"
                    onclick={() => void toggleDetail(workflow, "schedule")}
                    title={t("workflows.schedule")}
                    aria-label={t("workflows.schedule")}
                  >
                    <CalendarClock class="size-3.5" />
                  </button>
                  <button
                    type="button"
                    class="icon-btn size-6 text-[var(--text-muted)]"
                    onclick={() => void toggleDetail(workflow, "versions")}
                    title={t("workflows.versions")}
                    aria-label={t("workflows.versions")}
                  >
                    <GitBranch class="size-3.5" />
                  </button>
                  <button
                    type="button"
                    class="icon-btn size-6 text-[var(--text-muted)]"
                    onclick={() => void toggleDetail(workflow, "runs")}
                    title={t("workflows.runs")}
                    aria-label={t("workflows.runs")}
                  >
                    <History class="size-3.5" />
                  </button>
                  <button
                    type="button"
                    class="icon-btn size-6 text-[var(--text-muted)] hover:text-danger"
                    onclick={() => void deleteWorkflow(workflow)}
                    title={t("workflows.delete")}
                    aria-label={t("workflows.delete")}
                  >
                    <Trash2 class="size-3.5" />
                  </button>
                </div>
              </div>

              {#if detailId === workflow.id}
                <div class="mt-2 border-t border-[var(--hairline)] pt-2">
                  {#if detailLoading}
                    <p class="flex items-center gap-1.5 text-[11px] text-[var(--text-muted)]" role="status">
                      <Loader2 class="size-3.5 animate-spin" /> {t("ui.loading")}
                    </p>
                  {:else if detailMode === "versions"}
                    {#if versions.length === 0}
                      <p class="text-[11px] text-[var(--text-muted)]">{t("workflows.noVersions")}</p>
                    {:else}
                      <div class="space-y-1">
                        {#each versions as version (version.id)}
                          <div class="flex items-center gap-2 rounded-md bg-[var(--surface-1)] px-2 py-1">
                            <span class="font-mono text-[11px] text-[var(--brand-text)]">v{version.version}</span>
                            <span class="min-w-0 flex-1 truncate text-[11px] text-[var(--text-secondary)]">{version.name}</span>
                            <span class="shrink-0 font-mono text-[11px] text-[var(--text-muted)]">{fmtTime(version.created_at)}</span>
                            {#if onRun}
                              <button
                                type="button"
                                class="icon-btn size-5 text-[var(--brand-text)]"
                                onclick={() => runWorkflow(version)}
                                title={t("workflows.run")}
                                aria-label={t("workflows.run")}
                              >
                                <Play class="size-3" />
                              </button>
                            {/if}
                            <button
                              type="button"
                              class="inline-flex h-5 items-center rounded border border-[var(--hairline)] px-1.5 text-[11px] font-semibold text-[var(--text-secondary)] cursor-pointer"
                              onclick={() => loadWorkflow(version)}
                            >{t("workflows.load")}</button>
                          </div>
                        {/each}
                      </div>
                    {/if}
                  {:else if detailMode === "trigger"}
                    <div class="space-y-2">
                      <p class="text-[11px] leading-4 text-[var(--text-tertiary)]">
                        {t("workflows.triggerDesc")}
                      </p>
                      <p class="text-[11px] {webhookStatus?.enabled
                        ? 'text-[var(--success-text)]'
                        : 'text-[var(--text-muted)]'}">
                        {webhookStatus?.enabled
                          ? t("workflows.triggerEnabled")
                          : t("workflows.triggerDisabled")}
                      </p>
                      {#if webhookCreated}
                        <p class="text-[11px] leading-4 text-[var(--warning-text)]">
                          {t("workflows.triggerOnce")}
                        </p>
                        <div class="flex flex-wrap items-center gap-2">
                          <input
                            readonly
                            value={webhookCreated.url}
                            aria-label={t("workflows.triggerUrl")}
                            class="h-7 min-w-48 flex-1 rounded-md border border-[var(--hairline)] bg-[var(--surface-2)] px-2 font-mono text-[11px] text-[var(--text-secondary)]"
                          />
                          <button
                            type="button"
                            class="inline-flex h-7 items-center rounded-md border border-[var(--hairline)] bg-[var(--surface-2)] px-2.5 text-[11px] font-semibold text-[var(--text-secondary)] cursor-pointer"
                            onclick={() => void copyWebhook()}
                          >
                            {webhookCopied
                              ? t("workflows.copied")
                              : t("workflows.copy")}
                          </button>
                        </div>
                        <p class="break-all font-mono text-[11px] text-[var(--text-muted)]">
                          {t("workflows.triggerHeader", {
                            header: webhookCreated.header,
                          })}
                        </p>
                      {/if}
                      <div class="flex flex-wrap items-center gap-2">
                        <button
                          type="button"
                          class="inline-flex h-7 items-center gap-1.5 rounded-md border border-[var(--brand)]/40 bg-[var(--brand-soft)] px-2.5 text-[11px] font-semibold text-[var(--brand-text)] cursor-pointer disabled:opacity-50"
                          onclick={() => void enableWebhook(workflow)}
                          disabled={webhookBusy}
                        >
                          {#if webhookBusy}
                            <Loader2 class="size-3.5 animate-spin" />
                          {/if}
                          {webhookStatus?.enabled
                            ? t("workflows.rotateTrigger")
                            : t("workflows.enableTrigger")}
                        </button>
                        {#if webhookStatus?.enabled}
                          <button
                            type="button"
                            class="inline-flex h-7 items-center rounded-md border border-[var(--danger-border)] bg-[var(--danger-soft)] px-2.5 text-[11px] font-semibold text-[var(--danger-text)] cursor-pointer disabled:opacity-50"
                            onclick={() => void disableWebhook(workflow)}
                            disabled={webhookBusy}
                          >
                            {t("workflows.disableTrigger")}
                          </button>
                        {/if}
                      </div>
                      {#if webhookError}
                        <p class="break-words text-[11px] text-[var(--danger-text)]" role="alert">
                          {webhookError}
                        </p>
                      {/if}
                    </div>
                  {:else if detailMode === "schedule"}
                    <div class="space-y-2">
                      <p class="text-[11px] leading-4 text-[var(--text-tertiary)]">
                        {t("workflows.scheduleDesc")}
                      </p>
                      <div class="flex flex-wrap items-center gap-2">
                        <label class="text-[11px] text-[var(--text-secondary)]">
                          <span class="sr-only">{t("workflows.schedule")}</span>
                          <select
                            class="h-7 rounded-md border border-[var(--hairline)] bg-[var(--surface-1)] px-2 text-[11px] text-[var(--text-primary)]"
                            value={scheduleInterval}
                            onchange={(event) =>
                              (scheduleInterval = event.currentTarget.value)}
                            disabled={scheduleSaving}
                          >
                            <option value="0">{t("workflows.scheduleOff")}</option>
                            <option value="24">{t("workflows.scheduleDaily")}</option>
                            <option value="168">{t("workflows.scheduleWeekly")}</option>
                          </select>
                        </label>
                        <button
                          type="button"
                          class="inline-flex h-7 items-center rounded-md border border-[var(--brand)]/40 bg-[var(--brand-soft)] px-2.5 text-[11px] font-semibold text-[var(--brand-text)] cursor-pointer disabled:opacity-50"
                          onclick={() => void saveSchedule(workflow)}
                          disabled={scheduleSaving}
                        >
                          {#if scheduleSaving}
                            <Loader2 class="size-3.5 animate-spin" />
                          {/if}
                          {t("workflows.scheduleSave")}
                        </button>
                      </div>
                      {#if schedule?.enabled && schedule.next_run_at}
                        <p class="font-mono text-[11px] text-[var(--text-muted)]">
                          {t("workflows.scheduleNext", {
                            time: fmtTime(schedule.next_run_at),
                          })}
                        </p>
                      {/if}
                      {#if schedule?.last_run_at}
                        <p class="font-mono text-[11px] text-[var(--text-muted)]">
                          {t("workflows.scheduleLast", {
                            time: fmtTime(schedule.last_run_at),
                          })}
                        </p>
                      {/if}
                      {#if schedule?.last_error}
                        <p class="text-[11px] text-[var(--danger-text)] break-words" role="alert">
                          {t("workflows.scheduleError", {
                            error: schedule.last_error,
                          })}
                        </p>
                      {/if}
                    </div>
                  {:else if detailMode === "runs"}
                    {#if runs.length === 0}
                      <p class="text-[11px] text-[var(--text-muted)]">{t("workflows.noRuns")}</p>
                    {:else}
                      <div class="space-y-1">
                        {#each runs as run (run.id)}
                          <div class="rounded-md bg-[var(--surface-1)] px-2 py-1">
                            <div class="flex items-center gap-2">
                              <span class="font-mono text-[11px] text-[var(--text-muted)]">v{run.workflow_version}</span>
                              <span class="text-[11px] font-semibold {run.state === 'completed'
                                ? 'text-[var(--success-text)]'
                                : run.state === 'failed'
                                  ? 'text-[var(--danger-text)]'
                                  : run.state === 'running'
                                    ? 'text-[var(--brand-text)]'
                                    : 'text-[var(--warning-text)]'}">{runLabel(run.state)}</span>
                              <span class="ml-auto shrink-0 font-mono text-[11px] text-[var(--text-muted)]">{fmtTime(run.started_at)}</span>
                            </div>
                            {#if run.thread_id}
                              <p class="truncate font-mono text-[11px] text-[var(--text-muted)]">
                                {t("workflows.thread")} {run.thread_id.slice(0, 8)}
                              </p>
                            {/if}
                            {#if run.detail}
                              <p class="line-clamp-2 text-[11px] text-[var(--text-tertiary)]">{run.detail}</p>
                            {/if}
                          </div>
                        {/each}
                      </div>
                    {/if}
                  {/if}
                </div>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    </div>
  {/if}

  {#if notice}
    <p class="text-[11px] leading-4 text-[var(--success-text)]" role="status">{notice}</p>
  {/if}
  {#if error}
    <p class="text-[11px] leading-4 text-[var(--danger-text)] break-words" role="alert">{error}</p>
  {/if}
</div>
