<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { t } from "$lib/i18n";
  import RavenAvatar from "$lib/components/RavenAvatar.svelte";
  import {
    Activity,
    AlertTriangle,
    CheckCircle2,
    Coins,
    DollarSign,
    Loader2,
    RefreshCw,
    Zap,
  } from "@lucide/svelte";

  interface UsageAgent {
    bot_id: string;
    runs: number;
    tokens: number;
    cost: number;
    failed: number;
    completed: number;
  }

  interface UsageSummary {
    days: number;
    total_runs: number;
    total_tokens: number;
    total_cost: number;
    failed_runs: number;
    completed_runs: number;
    agents: UsageAgent[];
  }

  let { bots = [] }: { bots?: any[] } = $props();

  let days = $state(30);
  let data = $state<UsageSummary | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let request = 0;
  let lastLoadedDays = 30;

  const periods = [
    { days: 7, labelKey: "usage.period7" },
    { days: 30, labelKey: "usage.period30" },
    { days: 0, labelKey: "usage.periodAll" },
  ] as const;

  function botFor(id: string): any | undefined {
    return bots.find((bot) => bot.id === id);
  }

  function botName(id: string): string {
    return botFor(id)?.name || t("ui.fallbackAgent");
  }

  async function load() {
    const current = ++request;
    loading = true;
    error = null;
    try {
      const summary = await invoke<UsageSummary>("get_usage_summary", { days });
      if (current === request) data = summary;
    } catch (cause) {
      if (current === request) {
        error = String(cause);
        data = null;
      }
    } finally {
      if (current === request) loading = false;
    }
  }

  onMount(() => {
    void load();
    return () => {
      request += 1;
    };
  });

  $effect(() => {
    const selected = days;
    if (selected === lastLoadedDays) return;
    lastLoadedDays = selected;
    void load();
  });

  let successRate = $derived(
    data && data.total_runs > 0 ? (data.completed_runs / data.total_runs) * 100 : 0,
  );
  let maxCost = $derived(
    Math.max(0.0001, ...(data?.agents ?? []).map((agent) => agent.cost)),
  );

  function money(value: number): string {
    if (value >= 1000) return `$${value.toFixed(2)}`;
    return `$${value.toFixed(4)}`;
  }
</script>

<section class="space-y-5" aria-label={t("usage.title")}>
  <div class="flex flex-wrap items-start justify-between gap-3">
    <div>
      <h3 class="flex items-center gap-2 text-lg font-bold text-[var(--text-primary)]">
        <Activity class="size-5 text-[var(--brand-text)]" />
        {t("usage.title")}
      </h3>
      <p class="mt-1 text-sm text-[var(--text-tertiary)]">{t("usage.description")}</p>
    </div>
    <div class="flex flex-wrap items-center gap-1.5">
      <div class="flex items-center gap-0.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] p-0.5" role="group" aria-label={t("usage.period")}>
        {#each periods as period (period.days)}
          <button
            type="button"
            class="h-7 rounded-md px-2.5 text-[11px] font-medium transition-colors {days === period.days
              ? 'bg-[var(--surface-1)] text-[var(--text-primary)] shadow-sm'
              : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)]'}"
            aria-pressed={days === period.days}
            onclick={() => (days = period.days)}
          >
            {t(period.labelKey)}
          </button>
        {/each}
      </div>
      <button
        type="button"
        class="icon-btn size-8 border border-[var(--hairline)] bg-[var(--surface-2)]"
        aria-label={t("usage.refresh")}
        title={t("usage.refresh")}
        disabled={loading}
        onclick={() => void load()}
      >
        {#if loading}<Loader2 class="size-3.5 animate-spin" />{:else}<RefreshCw class="size-3.5" />{/if}
      </button>
    </div>
  </div>

  {#if error}
    <div class="flex items-start gap-2 rounded-xl border border-[var(--danger-border)] bg-[var(--danger-soft)] px-4 py-3 text-[12px] text-[var(--danger-text)]" role="alert">
      <AlertTriangle class="mt-0.5 size-4 shrink-0" />
      <div class="min-w-0">
        <p class="font-semibold">{t("usage.error")}</p>
        <p class="mt-0.5 break-words opacity-80">{error}</p>
        <button type="button" class="mt-2 rounded-md border border-current px-2 py-1 text-[11px] font-semibold" onclick={() => void load()}>
          {t("usage.retry")}
        </button>
      </div>
    </div>
  {:else}
    <div class="grid grid-cols-2 gap-2 sm:grid-cols-4">
      <div class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] p-3">
        <div class="flex items-center gap-1.5 text-[11px] font-mono uppercase tracking-wider text-[var(--text-muted)]">
          <DollarSign class="size-3 text-success" /> {t("usage.totalCost")}
        </div>
        <p class="mt-1.5 text-lg font-semibold tabular-nums text-[var(--text-primary)]">
          {#if loading && !data}<span class="inline-block h-5 w-20 animate-pulse rounded bg-[var(--surface-3)]"></span>{:else}{money(data?.total_cost ?? 0)}{/if}
        </p>
      </div>
      <div class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] p-3">
        <div class="flex items-center gap-1.5 text-[11px] font-mono uppercase tracking-wider text-[var(--text-muted)]">
          <Coins class="size-3 text-[var(--brand-text)]" /> {t("usage.totalTokens")}
        </div>
        <p class="mt-1.5 text-lg font-semibold tabular-nums text-[var(--text-primary)]">
          {#if loading && !data}<span class="inline-block h-5 w-20 animate-pulse rounded bg-[var(--surface-3)]"></span>{:else}{(data?.total_tokens ?? 0).toLocaleString()}{/if}
        </p>
      </div>
      <div class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] p-3">
        <div class="flex items-center gap-1.5 text-[11px] font-mono uppercase tracking-wider text-[var(--text-muted)]">
          <Zap class="size-3 text-warning" /> {t("usage.runs")}
        </div>
        <p class="mt-1.5 text-lg font-semibold tabular-nums text-[var(--text-primary)]">
          {#if loading && !data}<span class="inline-block h-5 w-16 animate-pulse rounded bg-[var(--surface-3)]"></span>{:else}{(data?.total_runs ?? 0).toLocaleString()}{/if}
        </p>
      </div>
      <div class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] p-3">
        <div class="flex items-center gap-1.5 text-[11px] font-mono uppercase tracking-wider text-[var(--text-muted)]">
          <CheckCircle2 class="size-3 text-success" /> {t("usage.successRate")}
        </div>
        <p class="mt-1.5 text-lg font-semibold tabular-nums text-[var(--text-primary)]">
          {#if loading && !data}<span class="inline-block h-5 w-14 animate-pulse rounded bg-[var(--surface-3)]"></span>{:else}{successRate.toFixed(1)}%{/if}
        </p>
      </div>
    </div>

    {#if data && data.total_runs > 0 && data.total_tokens === 0}
      <p class="rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-3 py-2 text-[11px] leading-relaxed text-[var(--text-muted)]">
        {t("usage.tokensUnavailable")}
      </p>
    {/if}

    <div class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)]">
      <div class="flex items-center justify-between border-b border-[var(--hairline)] px-4 py-3">
        <h4 class="text-xs font-bold uppercase tracking-wider text-[var(--text-tertiary)]">{t("usage.byAgent")}</h4>
        <span class="font-mono text-[11px] text-[var(--text-muted)]">
          {days === 0 ? t("usage.periodAll") : t("usage.window", { days })}
        </span>
      </div>

      {#if loading && !data}
        <div class="space-y-2 p-4">
          {#each [0, 1, 2] as row (row)}
            <div class="h-10 animate-pulse rounded-lg bg-[var(--surface-2)]"></div>
          {/each}
        </div>
      {:else if !data || data.agents.length === 0}
        <div class="flex flex-col items-center gap-1 px-4 py-12 text-center">
          <span class="flex size-10 items-center justify-center rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] text-[var(--text-muted)]">
            <Activity class="size-4" />
          </span>
          <p class="mt-1 text-xs font-semibold text-[var(--text-secondary)]">{t("usage.empty")}</p>
          <p class="text-[11px] text-[var(--text-muted)]">{t("usage.emptyHint")}</p>
        </div>
      {:else}
        <div class="overflow-x-auto">
          <table class="w-full text-left text-[11px]">
            <thead class="border-b border-[var(--hairline)] font-mono text-[11px] uppercase tracking-wider text-[var(--text-muted)]">
              <tr>
                <th class="px-4 py-2 font-medium">{t("usage.agent")}</th>
                <th class="px-3 py-2 text-right font-medium">{t("usage.runs")}</th>
                <th class="px-3 py-2 text-right font-medium">{t("usage.tokens")}</th>
                <th class="px-3 py-2 text-right font-medium">{t("usage.failed")}</th>
                <th class="px-4 py-2 text-right font-medium">{t("usage.cost")}</th>
              </tr>
            </thead>
            <tbody>
              {#each data.agents as agent (agent.bot_id)}
                {@const bot = botFor(agent.bot_id)}
                <tr class="border-b border-[var(--hairline)] last:border-0">
                  <td class="px-4 py-2.5">
                    <div class="flex min-w-0 items-center gap-2">
                      <RavenAvatar
                        name={bot?.name ?? botName(agent.bot_id)}
                        imageUrl={bot?.avatar_url ?? null}
                        style={bot?.avatar_style ?? null}
                        decorative
                      />
                      <span class="truncate font-medium text-[var(--text-primary)]">{botName(agent.bot_id)}</span>
                    </div>
                  </td>
                  <td class="px-3 py-2.5 text-right font-mono tabular-nums text-[var(--text-secondary)]">{agent.runs.toLocaleString()}</td>
                  <td class="px-3 py-2.5 text-right font-mono tabular-nums text-[var(--text-secondary)]">{agent.tokens.toLocaleString()}</td>
                  <td class="px-3 py-2.5 text-right font-mono tabular-nums {agent.failed > 0 ? 'text-[var(--danger-text)]' : 'text-[var(--text-muted)]'}">{agent.failed.toLocaleString()}</td>
                  <td class="px-4 py-2.5 text-right">
                    <div class="ml-auto flex max-w-40 flex-col items-end gap-1">
                      <span class="font-mono tabular-nums text-[var(--text-primary)]">{money(agent.cost)}</span>
                      <span class="h-1 w-full overflow-hidden rounded-full bg-[var(--surface-3)]">
                        <span
                          class="block h-full rounded-full bg-[var(--brand)]"
                          style="width: {Math.max(2, (agent.cost / maxCost) * 100)}%"
                        ></span>
                      </span>
                    </div>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>
  {/if}
</section>
