<script lang="ts">
  /**
   * One place to answer "what is waiting on me across the whole fleet?"
   *
   * The list is a projection of approvals, questions, office queue rows, and
   * computed office budget alerts. A row never answers a gate in place: it
   * opens the owning conversation or office, where the full context lives.
   */
  import { onMount } from "svelte";
  import {
    ChevronRight,
    CircleHelp,
    DollarSign,
    Inbox,
    ListOrdered,
    Loader2,
    RefreshCw,
    ShieldAlert,
  } from "@lucide/svelte";
  import { t } from "$lib/i18n";
  import { localeState } from "$lib/i18n/state.svelte";
  import {
    attention,
    relativeAttentionTime,
    type AttentionFilter,
    type AttentionItem,
  } from "$lib/attention.svelte";
  import { workspace } from "$lib/workspace.svelte";
  import { requestThreadFocus } from "$lib/chat/messageFocus";

  let filter = $state<AttentionFilter>("all");
  let now = $state(Date.now());

  const counts = $derived({
    all: attention.items.length,
    approval: attention.items.filter((item) => item.kind === "approval").length,
    question: attention.items.filter((item) => item.kind === "question").length,
    queue: attention.items.filter((item) => item.kind === "queue").length,
    alert: attention.items.filter((item) => item.kind === "alert").length,
  });

  const filtered = $derived(
    filter === "all"
      ? attention.items
      : attention.items.filter((item) => item.kind === filter),
  );

  onMount(() => {
    void attention.refresh();
    const timer = setInterval(() => (now = Date.now()), 30_000);
    return () => clearInterval(timer);
  });

  function kindLabel(item: AttentionItem): string {
    if (item.kind === "approval") return t("attention.approval");
    if (item.kind === "question") return t("attention.question");
    if (item.kind === "alert") return t("attention.alert");
    return t("attention.queued");
  }

  function titleOf(item: AttentionItem): string {
    if (item.kind === "queue") {
      return item.state === "failed"
        ? t("attention.failedQueue")
        : t("attention.queuedMessage");
    }
    if (item.kind === "alert") {
      return item.state === "critical"
        ? t("attention.budgetExhausted")
        : t("attention.budgetWarning");
    }
    return item.title;
  }

  function money(value: number): string {
    return value >= 1000 ? `$${value.toFixed(2)}` : `$${value.toFixed(4)}`;
  }

  function detailOf(item: AttentionItem): string {
    if (item.kind === "alert" && item.budget != null && item.cost != null) {
      return t("attention.budgetDetail", {
        spent: money(item.cost),
        budget: money(item.budget),
        percent: Math.round(item.percent ?? 0),
      });
    }
    return item.detail;
  }

  function whereOf(item: AttentionItem): string {
    if (item.office_name) return item.office_name;
    return item.bot_name || t("attention.agent");
  }

  function iconOf(item: AttentionItem) {
    if (item.kind === "approval") return ShieldAlert;
    if (item.kind === "question") return CircleHelp;
    if (item.kind === "alert") return DollarSign;
    return ListOrdered;
  }

  function openItem(item: AttentionItem) {
    if (item.kind === "alert") {
      if (item.office_id) workspace.selectRoom(item.office_id);
      return;
    }
    if (item.kind === "queue") {
      if (item.office_id) workspace.selectRoom(item.office_id);
      return;
    }
    if (!item.bot_id) return;
    workspace.selectBot(item.bot_id);
    if (item.thread_id) {
      requestThreadFocus({ botId: item.bot_id, threadId: item.thread_id });
    }
  }

  function relative(item: AttentionItem): string {
    return relativeAttentionTime(item.created_at, localeState.current, now);
  }
</script>

<div class="flex-1 min-h-0 overflow-y-auto bg-[var(--surface-0)]">
  <div class="mx-auto w-full max-w-4xl p-5 sm:p-7 space-y-5">
    <header class="flex flex-wrap items-start justify-between gap-3">
      <div class="min-w-0">
        <h1 class="text-lg font-bold tracking-tight text-[var(--text-primary)]">
          {t("attention.title")}
        </h1>
        <p class="mt-1 text-xs leading-5 text-[var(--text-tertiary)]">
          {t("attention.subtitle")}
        </p>
      </div>
      <button
        type="button"
        class="inline-flex h-8 items-center gap-1.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-1)] px-2.5 text-[11px] font-semibold text-[var(--text-secondary)] hover:bg-[var(--surface-2)] hover:text-[var(--text-primary)] cursor-pointer disabled:opacity-50"
        disabled={attention.loading}
        onclick={() => void attention.refresh()}
      >
        {#if attention.loading}
          <Loader2 class="size-3.5 animate-spin" />
        {:else}
          <RefreshCw class="size-3.5" />
        {/if}
        {t("attention.refresh")}
      </button>
    </header>

    <div class="flex flex-wrap items-center gap-1.5" role="group" aria-label={t("attention.title")}>
      {#each [
        { id: "all" as const, label: t("attention.all"), count: counts.all },
        { id: "approval" as const, label: t("attention.approvals"), count: counts.approval },
        { id: "question" as const, label: t("attention.questions"), count: counts.question },
        { id: "queue" as const, label: t("attention.queue"), count: counts.queue },
        { id: "alert" as const, label: t("attention.alerts"), count: counts.alert },
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

    {#if attention.error && attention.items.length === 0}
      <div class="rounded-xl border border-[var(--danger-border)] bg-[var(--danger-soft)] p-4" role="alert">
        <p class="text-sm font-semibold text-[var(--danger-text)]">{t("attention.error")}</p>
        <p class="mt-1 text-xs text-[var(--text-secondary)] break-words">{attention.error}</p>
        <button
          type="button"
          class="mt-3 inline-flex h-7 items-center rounded-lg border border-[var(--danger-border)] px-2.5 text-[11px] font-semibold text-[var(--danger-text)] hover:bg-[var(--surface-1)] cursor-pointer"
          onclick={() => void attention.refresh()}
        >
          {t("attention.retry")}
        </button>
      </div>
    {:else if attention.loading && attention.items.length === 0}
      <div class="flex items-center justify-center gap-2 py-16 text-xs text-[var(--text-tertiary)]" role="status">
        <Loader2 class="size-4 animate-spin" />
        {t("ui.loading")}
      </div>
    {:else if filtered.length === 0}
      <div class="flex flex-col items-center justify-center gap-3 rounded-2xl border border-dashed border-[var(--hairline-strong)] bg-[var(--surface-1)] px-6 py-14 text-center">
        <span class="flex size-12 items-center justify-center rounded-2xl border border-[var(--hairline)] bg-[var(--surface-2)] text-[var(--text-tertiary)]">
          <Inbox class="size-5" strokeWidth={1.7} />
        </span>
        <div class="space-y-1">
          <p class="text-sm font-semibold text-[var(--text-primary)]">{t("attention.empty")}</p>
          <p class="max-w-sm text-xs leading-5 text-[var(--text-tertiary)]">{t("attention.emptyHint")}</p>
        </div>
      </div>
    {:else}
      <div class="space-y-2">
        {#each filtered as item (item.kind + ":" + item.id)}
          {@const Icon = iconOf(item)}
          {@const highRisk = item.kind === "approval" && item.risk === "high"}
          {@const criticalAlert = item.kind === "alert" && item.state === "critical"}
          {@const warningAlert = item.kind === "alert" && item.state === "warning"}
          <button
            type="button"
            class="group flex w-full min-w-0 items-start gap-3 rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] p-3 text-left cursor-pointer transition-colors hover:border-[var(--hairline-strong)] hover:bg-[var(--surface-2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--brand)]"
            onclick={() => openItem(item)}
          >
            <span
              class="mt-0.5 flex size-9 shrink-0 items-center justify-center rounded-xl border {criticalAlert
                ? 'border-[var(--danger-border)] bg-[var(--danger-soft)] text-[var(--danger-text)]'
                : highRisk || warningAlert
                  ? 'border-[var(--warning-border)] bg-[var(--warning-soft)] text-[var(--warning-text)]'
                  : 'border-[var(--hairline)] bg-[var(--surface-2)] text-[var(--brand-text)]'}"
              aria-hidden="true"
            >
              <Icon class="size-4" strokeWidth={1.8} />
            </span>

            <span class="min-w-0 flex-1 space-y-1">
              <span class="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 text-[11px] font-mono uppercase tracking-[0.05em] text-[var(--text-muted)]">
                <span>{kindLabel(item)}</span>
                <span aria-hidden="true">·</span>
                <span class="truncate normal-case tracking-normal text-[var(--text-tertiary)]">{whereOf(item)}</span>
                <span aria-hidden="true">·</span>
                <span class="normal-case tracking-normal">{relative(item)}</span>
              </span>
              <span class="block truncate text-[13px] font-semibold text-[var(--text-primary)]">{titleOf(item)}</span>
              <span class="block line-clamp-2 text-xs leading-5 text-[var(--text-secondary)]">{detailOf(item)}</span>
            </span>

            <span class="flex shrink-0 flex-col items-end gap-1.5">
              {#if criticalAlert}
                <span class="inline-flex h-5 items-center rounded-full border border-[var(--danger-border)] bg-[var(--danger-soft)] px-1.5 text-[11px] font-mono text-[var(--danger-text)]">
                  {t("attention.budgetOverBadge")}
                </span>
              {:else if highRisk || warningAlert}
                <span class="inline-flex h-5 items-center rounded-full border border-[var(--warning-border)] bg-[var(--warning-soft)] px-1.5 text-[11px] font-mono text-[var(--warning-text)]">
                  {warningAlert ? t("attention.budgetNearBadge") : t("attention.highStakes")}
                </span>
              {:else if item.state === "failed"}
                <span class="inline-flex h-5 items-center rounded-full border border-[var(--danger-border)] bg-[var(--danger-soft)] px-1.5 text-[11px] font-mono text-[var(--danger-text)]">
                  {t("attention.failed")}
                </span>
              {:else}
                <span class="inline-flex h-5 items-center rounded-full border border-[var(--hairline)] bg-[var(--surface-2)] px-1.5 text-[11px] font-mono text-[var(--text-muted)]">
                  {item.kind === "queue" ? t("attention.pending") : t("attention.open")}
                </span>
              {/if}
              <ChevronRight class="size-3.5 text-[var(--text-muted)] transition-transform group-hover:translate-x-0.5" aria-hidden="true" />
            </span>
          </button>
        {/each}
      </div>
    {/if}
  </div>
</div>
