<script lang="ts">
  // The persistent office board (planner → kanban → live dependency graph).
  // Fed by ChatRoomView from the office run's live events
  // (plan_ready / node_open / node_finished / graph_status) and rehydrated
  // from the returned checklist when a run finished while the room was away.
  import { t } from "$lib/i18n";
  import { entrance } from "$lib/chat/entrance";
  import RavenAvatar from "$lib/components/RavenAvatar.svelte";
  import PlanDag from "$lib/components/chat/PlanDag.svelte";
  import type { DagTask } from "$lib/chat/dag";
  import {
    Workflow,
    ChevronDown,
    Loader2,
    CheckCircle2,
    XCircle,
    Circle,
    MinusCircle,
    ListChecks,
  } from "@lucide/svelte";

  export interface BoardNode {
    nodeId: string;
    botId: string;
    label: string;
    /** Node ids this task waits on (resolved deps from `plan_ready`). */
    dependsOn: string[];
    state: "pending" | "running" | "done" | "failed" | "skipped";
    preview?: string;
    /** Thread this node streams on (from `node_open`); a re-opened room uses
     it to check the run's liveness and re-query parked approvals/questions. */
    nodeThreadId?: string;
  }

  export interface BotTodo {
    id: string;
    task: string;
    done: boolean;
  }

  interface Props {
    goal: string;
    nodes: BoardNode[];
    members: any[];
    runActive: boolean;
    /** Per-bot self-tracked checklists (`todo` tool), keyed by bot id. */
    todos?: Record<string, BotTodo[]>;
  }

  let { goal, nodes, members, runActive, todos }: Props = $props();

  let collapsed = $state(false);
  // Re-expand for a new run; auto-collapse when the run ends so the
  // conversation gets the vertical space back.
  let sawActive = $state(false);
  $effect(() => {
    if (runActive) {
      sawActive = true;
      collapsed = false;
    } else if (sawActive) {
      sawActive = false;
      collapsed = true;
    }
  });

  const columns = $derived([
    { key: "upNext", label: t("room.board.upNext"), items: nodes.filter((n) => n.state === "pending") },
    { key: "working", label: t("room.board.working"), items: nodes.filter((n) => n.state === "running") },
    { key: "done", label: t("room.board.done"), items: nodes.filter((n) => n.state === "done") },
    {
      key: "issues",
      label: t("room.board.issues"),
      items: [...nodes.filter((n) => n.state === "failed"), ...nodes.filter((n) => n.state === "skipped")],
    },
  ]);

  const doneCount = $derived(
    nodes.filter((n) => n.state === "done" || n.state === "skipped").length
  );

  // PlanDag works on index-based deps; the board's order is stable per run,
  // so translate node-id deps to indices here.
  const dagTasks = $derived.by<DagTask[]>(() => {
    const idx = new Map(nodes.map((n, i) => [n.nodeId, i]));
    return nodes.map((n) => ({
      label: n.label,
      botId: n.botId,
      dependsOn: n.dependsOn.map((d) => idx.get(d) ?? -1).filter((i) => i >= 0),
    }));
  });
  const hasDeps = $derived(nodes.some((n) => n.dependsOn.length > 0));

  function memberFor(botId: string) {
    return members.find((m: any) => (m.bot?.id ?? m.bot_id) === botId);
  }
  function nameFor(botId: string): string {
    const m = memberFor(botId);
    return m?.bot?.name || m?.rank || "?";
  }
</script>

{#if nodes.length > 0}
  <section
    class="office-board min-w-0 rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] overflow-hidden"
    aria-label={t("room.board.title")}
  >
    <button
      type="button"
      class="w-full flex items-center gap-2 px-3 py-2 text-left hover:bg-[var(--surface-3)] transition-colors"
      aria-expanded={!collapsed}
      aria-label={collapsed ? t("room.board.show") : t("room.board.hide")}
      onclick={() => (collapsed = !collapsed)}
    >
      <Workflow class="size-4 text-[var(--brand-text)] shrink-0" />
      <span class="text-[13px] font-bold text-[var(--text-primary)] shrink-0">{t("room.board.title")}</span>
      <span class="text-[11px] text-[var(--text-muted)] shrink-0 tabular-nums">
        {t("room.board.progress", { done: String(doneCount), total: String(nodes.length) })}
      </span>
      {#if goal}
        <span class="text-[11px] text-[var(--text-tertiary)] truncate min-w-0">{goal}</span>
      {/if}
      <ChevronDown
        class="size-4 text-[var(--text-muted)] shrink-0 ml-auto transition-transform {collapsed ? '' : 'rotate-180'}"
      />
    </button>

    {#if !collapsed}
      <div class="px-3 pb-3 space-y-3">
        <div class="grid grid-cols-2 lg:grid-cols-4 gap-2 min-w-0">
          {#each columns as col (col.key)}
            <div class="min-w-0 rounded-xl border border-[var(--hairline)] bg-[var(--surface-0)] p-2">
              <div class="flex items-center gap-1.5 px-1 pb-1.5 text-[11px] font-bold text-[var(--text-secondary)]">
                <span class="truncate">{col.label}</span>
                <span class="ml-auto text-[10px] font-normal text-[var(--text-muted)] tabular-nums">{col.items.length}</span>
              </div>
              <div class="space-y-1.5 min-w-0 max-h-44 overflow-y-auto no-scrollbar">
                {#each col.items as n (n.nodeId)}
                  <div use:entrance class="min-w-0 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] p-2">
                    <div class="flex items-center gap-1.5 min-w-0">
                      <!-- A board card is the densest place the app shows who
                           is doing what, so the face reacts here too: a running
                           node's agent looks attentive without a label saying
                           so. -->
                      <RavenAvatar
                        name={nameFor(n.botId)}
                        mood={n.state === "running" ? "working" : n.state === "failed" ? "failed" : "idle"}
                        imageUrl={memberFor(n.botId)?.bot?.avatar_url}
                        decorative
                        class="size-4 rounded-full"
                      />
                      <span class="text-[10px] font-bold text-[var(--text-primary)] truncate">{nameFor(n.botId)}</span>
                      {#if n.state === "running"}
                        <Loader2 class="size-3 animate-spin text-[var(--brand-text)] shrink-0 ml-auto" aria-hidden="true" />
                      {:else if n.state === "done"}
                        <CheckCircle2 class="size-3 text-success shrink-0 ml-auto" aria-hidden="true" />
                      {:else if n.state === "failed"}
                        <XCircle class="size-3 text-danger shrink-0 ml-auto" aria-hidden="true" />
                      {:else if n.state === "skipped"}
                        <MinusCircle class="size-3 text-warning shrink-0 ml-auto" aria-hidden="true" />
                      {:else}
                        <Circle class="size-3 text-[var(--text-faint)] shrink-0 ml-auto" aria-hidden="true" />
                      {/if}
                    </div>
                    <div class="mt-1 text-[11px] text-[var(--text-secondary)] leading-snug line-clamp-2">{n.label}</div>
                    {#if (todos?.[n.botId] ?? []).length > 0}
                      {@const botTodos = todos?.[n.botId] ?? []}
                      {@const todosDone = botTodos.filter((td) => td.done).length}
                      <div class="mt-1 space-y-0.5 min-w-0">
                        <div class="flex items-center gap-1 text-[9px] font-mono text-[var(--text-muted)]">
                          <ListChecks class="size-2.5 shrink-0" />
                          <span class="tabular-nums">{todosDone}/{botTodos.length}</span>
                        </div>
                        {#each botTodos.filter((td) => !td.done).slice(0, 2) as item (item.id)}
                          <div class="text-[10px] text-[var(--text-tertiary)] truncate">◻ {item.task}</div>
                        {/each}
                      </div>
                    {/if}
                    {#if n.preview}
                      <div class="mt-1 text-[10px] text-[var(--text-muted)] leading-snug line-clamp-2">{n.preview}</div>
                    {/if}
                  </div>
                {:else}
                  <div class="text-[10px] text-[var(--text-faint)] px-1 py-2">{t("room.noTasks")}</div>
                {/each}
              </div>
            </div>
          {/each}
        </div>

        {#if hasDeps}
          <div class="min-w-0 rounded-xl border border-[var(--hairline)] bg-[var(--surface-0)] p-2">
            <div class="px-1 pb-1.5 text-[11px] font-bold text-[var(--text-secondary)] flex items-center gap-1.5">
              <span>{t("room.board.dagLive")}</span>
              <span class="text-[10px] font-normal text-[var(--text-muted)]">{t("room.dagTitle")}</span>
            </div>
            <PlanDag tasks={dagTasks} {members} stateFor={(i) => nodes[i]?.state} />
          </div>
        {/if}
      </div>
    {/if}
  </section>
{/if}
