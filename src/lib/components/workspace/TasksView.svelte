<script lang="ts">
  /**
   * The cross-agent task queue.
   *
   * This is the person-facing view of the durable `agent_tasks` ledger: work
   * one agent queued for another, what state it reached, and the thread where
   * it ran. It is deliberately read/stop, not a planner — starting a task is an
   * agent decision, and the composer already has the conversation.
   */
  import { onMount } from "svelte";
  import {
    CheckCircle2,
    ChevronRight,
    CircleSlash,
    Clock,
    Loader2,
    Play,
    RefreshCw,
    ShieldAlert,
    XCircle,
  } from "@lucide/svelte";
  import { t } from "$lib/i18n";
  import { localeState } from "$lib/i18n/state.svelte";
  import {
    agentTasks,
    isActiveTask,
    taskMatchesFilter,
    taskStateKey,
    type AgentTask,
    type AgentTaskState,
    type TaskFilter,
  } from "$lib/agentTasks.svelte";
  import { relativeAttentionTime } from "$lib/attention.svelte";
  import { workspace } from "$lib/workspace.svelte";
  import { sessionRole } from "$lib/sessionRole.svelte";
  import { requestThreadFocus } from "$lib/chat/messageFocus";
  import RavenAvatar from "$lib/components/RavenAvatar.svelte";
  import { notify } from "$lib/toast";

  let filter = $state<TaskFilter>("all");
  let now = $state(Date.now());
  let cancelling = $state<string | null>(null);

  const filtered = $derived(agentTasks.items.filter((task) => taskMatchesFilter(task, filter)));
  const finishedCount = $derived(
    agentTasks.items.filter((task) => taskMatchesFilter(task, "finished")).length,
  );
  const canCancel = $derived(sessionRole.role !== "viewer");

  onMount(() => {
    void agentTasks.refresh();
    const timer = setInterval(() => (now = Date.now()), 30_000);
    return () => clearInterval(timer);
  });

  function botFor(id: string): any {
    return workspace.bots.find((bot: any) => bot.id === id) ?? null;
  }

  function ownerName(task: AgentTask): string {
    return botFor(task.owner_bot_id)?.name || t("attention.agent");
  }

  function targetName(task: AgentTask): string {
    return botFor(task.target_bot_id)?.name || t("attention.agent");
  }

  function openTask(task: AgentTask) {
    workspace.selectBot(task.target_bot_id);
    if (task.thread_id) {
      requestThreadFocus({ botId: task.target_bot_id, threadId: task.thread_id });
    }
  }

  async function cancelTask(task: AgentTask) {
    if (cancelling) return;
    cancelling = task.id;
    try {
      const cancelled = await agentTasks.cancel(task.id);
      if (cancelled) {
        notify(t("tasks.cancelled"), "info");
      }
    } finally {
      cancelling = null;
    }
  }

  function stateIcon(state: AgentTaskState) {
    if (state === "failed") return XCircle;
    if (state === "completed") return CheckCircle2;
    if (state === "cancelled") return CircleSlash;
    if (state === "running") return Play;
    return Clock;
  }

  function stateTone(state: AgentTaskState): string {
    if (state === "failed") {
      return "border-[var(--danger-border)] bg-[var(--danger-soft)] text-[var(--danger-text)]";
    }
    if (state === "completed") {
      return "border-[var(--success-text)]/30 bg-[var(--success-soft)] text-[var(--success-text)]";
    }
    if (state === "cancelled") {
      return "border-[var(--hairline)] bg-[var(--surface-2)] text-[var(--text-muted)]";
    }
    return "border-[var(--warning-border)] bg-[var(--warning-soft)] text-[var(--warning-text)]";
  }

  function detail(task: AgentTask): string {
    if (task.state === "failed") return task.error || t("tasks.noDetail");
    if (task.result) return task.result;
    return task.expected_result || task.instruction || t("tasks.noDetail");
  }

  function relative(task: AgentTask): string {
    return relativeAttentionTime(task.created_at, localeState.current, now);
  }
</script>

<div class="flex-1 min-h-0 overflow-y-auto bg-[var(--surface-0)]">
  <div class="mx-auto w-full max-w-4xl p-5 sm:p-7 space-y-5">
    <header class="flex flex-wrap items-start justify-between gap-3">
      <div class="min-w-0">
        <h1 class="text-lg font-bold tracking-tight text-[var(--text-primary)]">
          {t("tasks.title")}
        </h1>
        <p class="mt-1 text-xs leading-5 text-[var(--text-tertiary)]">
          {t("tasks.subtitle")}
        </p>
      </div>
      <button
        type="button"
        class="inline-flex h-8 items-center gap-1.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-1)] px-2.5 text-[11px] font-semibold text-[var(--text-secondary)] hover:bg-[var(--surface-2)] hover:text-[var(--text-primary)] cursor-pointer disabled:opacity-50"
        disabled={agentTasks.loading}
        onclick={() => void agentTasks.refresh()}
      >
        {#if agentTasks.loading}
          <Loader2 class="size-3.5 animate-spin" />
        {:else}
          <RefreshCw class="size-3.5" />
        {/if}
        {t("attention.refresh")}
      </button>
    </header>

    <div class="flex flex-wrap items-center gap-1.5" role="group" aria-label={t("tasks.title")}>
      {#each [
        { id: "all" as const, label: t("tasks.all"), count: agentTasks.items.length },
        { id: "active" as const, label: t("tasks.active"), count: agentTasks.activeCount },
        { id: "failed" as const, label: t("tasks.failed"), count: agentTasks.failedCount },
        { id: "finished" as const, label: t("tasks.finished"), count: finishedCount },
      ] as tab (tab.id)}
        <button
          type="button"
          aria-pressed={filter === tab.id}
          class="inline-flex h-7 items-center gap-1.5 rounded-full border px-2.5 text-[11px] font-semibold cursor-pointer {filter === tab.id
            ? 'border-[var(--brand)] bg-[var(--brand-soft)] text-[var(--brand-text)]'
            : 'border-[var(--hairline)] bg-[var(--surface-1)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)]'}"
          onclick={() => (filter = tab.id)}
        >
          {tab.label}
          {#if tab.count}
            <span class="font-mono text-[11px] opacity-80">{tab.count}</span>
          {/if}
        </button>
      {/each}
    </div>

    {#if agentTasks.error && agentTasks.items.length === 0}
      <div class="rounded-xl border border-[var(--danger-border)] bg-[var(--danger-soft)] p-4" role="alert">
        <p class="text-sm font-semibold text-[var(--danger-text)]">{t("tasks.error")}</p>
        <p class="mt-1 text-xs text-[var(--text-secondary)] break-words">{agentTasks.error}</p>
        <button
          type="button"
          class="mt-3 inline-flex h-7 items-center rounded-lg border border-[var(--danger-border)] px-2.5 text-[11px] font-semibold text-[var(--danger-text)] hover:bg-[var(--surface-1)] cursor-pointer"
          onclick={() => void agentTasks.refresh()}
        >
          {t("attention.retry")}
        </button>
      </div>
    {:else if agentTasks.loading && agentTasks.items.length === 0}
      <div class="flex items-center justify-center gap-2 py-16 text-xs text-[var(--text-tertiary)]" role="status">
        <Loader2 class="size-4 animate-spin" />
        {t("ui.loading")}
      </div>
    {:else if filtered.length === 0}
      <div class="flex flex-col items-center justify-center gap-3 rounded-2xl border border-dashed border-[var(--hairline-strong)] bg-[var(--surface-1)] px-6 py-14 text-center">
        <span class="flex size-12 items-center justify-center rounded-2xl border border-[var(--hairline)] bg-[var(--surface-2)] text-[var(--text-tertiary)]">
          <CircleSlash class="size-5" strokeWidth={1.7} />
        </span>
        <div class="space-y-1">
          <p class="text-sm font-semibold text-[var(--text-primary)]">{t("tasks.empty")}</p>
          <p class="max-w-sm text-xs leading-5 text-[var(--text-tertiary)]">{t("tasks.emptyHint")}</p>
        </div>
      </div>
    {:else}
      <div class="space-y-2">
        {#each filtered as task (task.id)}
          {@const Icon = stateIcon(task.state)}
          {@const target = botFor(task.target_bot_id)}
          {@const targetLabel = targetName(task)}
          {@const ownerLabel = ownerName(task)}
          <div
            class="flex min-w-0 items-start gap-3 rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] p-3"
            data-task-state={task.state}
          >
            <span class="mt-0.5 shrink-0">
              <RavenAvatar
                name={targetLabel}
                imageUrl={target?.avatar_url}
                style={target?.avatar_style}
                decorative
                class="size-9 rounded-xl"
              />
            </span>

            <div class="min-w-0 flex-1 space-y-1">
              <button
                type="button"
                class="block max-w-full truncate text-left text-[13px] font-semibold text-[var(--text-primary)] hover:text-[var(--brand-text)] cursor-pointer"
                onclick={() => openTask(task)}
              >
                {task.title}
              </button>
              <div class="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 text-[11px] text-[var(--text-muted)]">
                <span class="inline-flex items-center gap-1 rounded-full border px-1.5 py-0.5 font-mono text-[11px] {stateTone(task.state)}">
                  <Icon class="size-3" />
                  {t(taskStateKey(task.state))}
                </span>
                <span class="truncate normal-case tracking-normal text-[var(--text-tertiary)]">{targetLabel}</span>
                {#if task.owner_bot_id !== task.target_bot_id}
                  <span aria-hidden="true">·</span>
                  <span class="truncate">{t("tasks.from", { name: ownerLabel })}</span>
                {/if}
                <span aria-hidden="true">·</span>
                <span>{relative(task)}</span>
              </div>
              <p class="line-clamp-2 text-xs leading-5 text-[var(--text-secondary)]">{detail(task)}</p>
            </div>

            <div class="flex shrink-0 flex-col items-end gap-1.5">
              <button
                type="button"
                class="inline-flex h-7 items-center gap-1 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-2 text-[11px] font-semibold text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] cursor-pointer disabled:opacity-50"
                disabled={!task.thread_id}
                aria-label={`${t("tasks.open")} — ${task.title}`}
                onclick={() => openTask(task)}
              >
                {t("tasks.open")}
                <ChevronRight class="size-3" />
              </button>
              {#if isActiveTask(task.state)}
                <button
                  type="button"
                  class="inline-flex h-7 items-center gap-1 rounded-lg px-2 text-[11px] font-semibold text-[var(--danger-text)] hover:bg-[var(--danger-soft)] cursor-pointer disabled:opacity-50"
                  disabled={cancelling === task.id || !canCancel}
                  aria-label={`${t("tasks.cancel")} — ${task.title}`}
                  onclick={() => void cancelTask(task)}
                >
                  {#if cancelling === task.id}
                    <Loader2 class="size-3 animate-spin" />
                  {:else}
                    <XCircle class="size-3" />
                  {/if}
                  {t("tasks.cancel")}
                </button>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    {/if}

    {#if agentTasks.error && agentTasks.items.length > 0}
      <div class="flex items-center gap-2 rounded-lg border border-[var(--warning-border)] bg-[var(--warning-soft)] px-2.5 py-2 text-[11px] text-[var(--warning-text)]" role="alert">
        <ShieldAlert class="size-3.5 shrink-0" />
        <span class="min-w-0 flex-1 truncate">{agentTasks.error}</span>
        <button
          type="button"
          class="shrink-0 font-semibold underline underline-offset-2 cursor-pointer"
          onclick={() => void agentTasks.refresh()}
        >
          {t("attention.retry")}
        </button>
      </div>
    {/if}
  </div>
</div>
