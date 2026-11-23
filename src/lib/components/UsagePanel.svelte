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

  interface UsageOffice {
    office_id: string | null;
    name: string | null;
    budget: number | null;
    runs: number;
    tokens: number;
    cost: number;
    failed: number;
    completed: number;
  }

  interface ToolPriceStatus {
    id: string;
    label: string;
    price_usd: number;
    source: "user" | "catalog" | "unpriced" | "inactive";
    note: string;
  }

  interface UsageSummary {
    days: number;
    total_runs: number;
    total_tokens: number;
    total_cost: number;
    total_spend?: number;
    total_model_cost?: number;
    total_tool_cost?: number;
    total_utility_cost?: number;
    total_utility_tokens?: number;
    failed_runs: number;
    completed_runs: number;
    agents: UsageAgent[];
    offices: UsageOffice[];
    tool_pricing?: ToolPriceStatus[];
  }

  interface DisplayRow {
    key: string;
    label: string;
    runs: number;
    tokens: number;
    cost: number;
    failed: number;
    budget: number | null;
  }

  let { bots = [] }: { bots?: any[] } = $props();

  let days = $state(30);
  let breakdown = $state<"agent" | "office">("agent");
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
  let displayRows = $derived<DisplayRow[]>(
    breakdown === "agent"
      ? (data?.agents ?? []).map((agent) => ({
          key: agent.bot_id,
          label: botName(agent.bot_id),
          runs: agent.runs,
          tokens: agent.tokens,
          cost: agent.cost,
          failed: agent.failed,
          budget: null,
        }))
      : (data?.offices ?? []).map((office) => ({
          key: office.office_id ?? "direct",
          label: office.name || t("usage.direct"),
          runs: office.runs,
          tokens: office.tokens,
          cost: office.cost,
          failed: office.failed,
          budget: office.budget,
        })),
  );
  let maxCost = $derived(
    Math.max(0.0001, ...displayRows.map((row) => row.cost)),
  );
  let totalSpend = $derived(data?.total_spend ?? data?.total_cost ?? 0);
  let spendCards = $derived([
    {
      key: "model",
      label: t("usage.modelSpend"),
      value: data?.total_model_cost ?? 0,
      hint: t("usage.modelSpendHint"),
    },
    {
      key: "utility",
      label: t("usage.utilitySpend"),
      value: data?.total_utility_cost ?? 0,
      hint: t("usage.utilitySpendHint"),
    },
    {
      key: "tools",
      label: t("usage.toolSpend"),
      value: data?.total_tool_cost ?? 0,
      hint: t("usage.toolSpendHint"),
    },
  ]);
  let activeTools = $derived(
    (data?.tool_pricing ?? []).filter((tool) => tool.source !== "inactive"),
  );

  function money(value: number): string {
    if (value >= 1000) return `$${value.toFixed(2)}`;
    return `$${value.toFixed(4)}`;
  }

  function priceSourceLabel(source: ToolPriceStatus["source"]): string {
    if (source === "user") return t("usage.priceUser");
    if (source === "catalog") return t("usage.priceEstimate");
    return t("usage.priceUnpriced");
  }

  function priceTone(source: ToolPriceStatus["source"]): string {
    if (source === "user") return "text-success";
    if (source === "catalog") return "text-warning";
    if (source === "unpriced") return "text-[var(--danger-text)]";
    return "text-[var(--text-muted)]";
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
          {#if loading && !data}<span class="inline-block h-5 w-20 animate-pulse rounded bg-[var(--surface-3)]"></span>{:else}{money(totalSpend)}{/if}
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
      <div class="flex flex-wrap items-center justify-between gap-2 border-b border-[var(--hairline)] px-4 py-3">
        <h4 class="text-xs font-bold uppercase tracking-wider text-[var(--text-tertiary)]">
          {t("usage.spendBreakdown")}
        </h4>
        <span class="font-mono text-[11px] text-[var(--text-muted)]">
          {t("usage.spendTotal", { total: money(totalSpend) })}
        </span>
      </div>
      <div class="grid grid-cols-1 gap-px bg-[var(--hairline)] sm:grid-cols-3">
        {#each spendCards as card (card.key)}
          <div class="bg-[var(--surface-1)] px-4 py-3">
            <div class="text-[11px] font-mono uppercase tracking-wider text-[var(--text-muted)]">{card.label}</div>
            <p class="mt-1 font-mono text-sm tabular-nums text-[var(--text-primary)]">{money(card.value)}</p>
            <p class="mt-0.5 text-[11px] leading-4 text-[var(--text-muted)]">{card.hint}</p>
          </div>
        {/each}
      </div>
      {#if (data?.total_utility_cost ?? 0) > 0}
        <p class="border-t border-[var(--hairline)] px-4 py-2 text-[11px] leading-4 text-[var(--text-muted)]">
          {t("usage.utilityNote")}
        </p>
      {/if}
    </div>

    <div class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)]">
      <div class="flex flex-wrap items-center justify-between gap-2 border-b border-[var(--hairline)] px-4 py-3">
        <h4 class="text-xs font-bold uppercase tracking-wider text-[var(--text-tertiary)]">
          {t("usage.paidTools")}
        </h4>
        <span class="text-[11px] text-[var(--text-muted)]">{t("usage.paidToolsHint")}</span>
      </div>
      {#if activeTools.length === 0}
        <p class="px-4 py-3 text-[11px] leading-4 text-[var(--text-muted)]">
          {t("usage.paidToolsNone")}
        </p>
      {:else}
        <ul>
          {#each activeTools as tool (tool.id)}
            <li class="flex flex-wrap items-center justify-between gap-2 border-b border-[var(--hairline)] px-4 py-2.5 last:border-0">
              <div class="min-w-0">
                <p class="text-xs font-semibold text-[var(--text-primary)]">{tool.label}</p>
                <p class="text-[11px] leading-4 text-[var(--text-muted)]">{tool.note}</p>
              </div>
              <div class="flex items-center gap-2">
                {#if tool.source === "unpriced"}
                  <span class="font-mono text-[11px] text-[var(--danger-text)]">{t("usage.priceUnpriced")}</span>
                {:else}
                  <span class="font-mono text-[11px] tabular-nums text-[var(--text-secondary)]">
                    {money(tool.price_usd)}{t("usage.perCall")}
                  </span>
                {/if}
                <span class="text-[11px] font-mono uppercase tracking-wider {priceTone(tool.source)}">
                  {priceSourceLabel(tool.source)}
                </span>
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    </div>

    <div class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)]">
      <div class="flex flex-wrap items-center justify-between gap-2 border-b border-[var(--hairline)] px-4 py-3">
        <div class="flex flex-wrap items-center gap-2">
          <h4 class="text-xs font-bold uppercase tracking-wider text-[var(--text-tertiary)]">
            {breakdown === "agent" ? t("usage.byAgent") : t("usage.byOffice")}
          </h4>
          <div
            class="flex items-center gap-0.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] p-0.5"
            role="group"
            aria-label={t("usage.breakdown")}
          >
            <button
              type="button"
              class="h-6 rounded-md px-2 text-[11px] font-medium transition-colors {breakdown === "agent"
                ? 'bg-[var(--surface-1)] text-[var(--text-primary)] shadow-sm'
                : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)]'}"
              aria-pressed={breakdown === "agent"}
              onclick={() => (breakdown = "agent")}
            >{t("usage.byAgent")}</button>
            <button
              type="button"
              class="h-6 rounded-md px-2 text-[11px] font-medium transition-colors {breakdown === "office"
                ? 'bg-[var(--surface-1)] text-[var(--text-primary)] shadow-sm'
                : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)]'}"
              aria-pressed={breakdown === "office"}
              onclick={() => (breakdown = "office")}
            >{t("usage.byOffice")}</button>
          </div>
        </div>
        <span class="font-mono text-[11px] text-[var(--text-muted)]">
          {days === 0 ? t("usage.periodAll") : t("usage.window", { days })}
        </span>
      </div>
      {#if breakdown === "office"}
        <p class="border-b border-[var(--hairline)] px-4 py-2 text-[11px] leading-4 text-[var(--text-muted)]">
          {t("usage.officeHint")}
        </p>
      {/if}

      {#if loading && !data}
        <div class="space-y-2 p-4">
          {#each [0, 1, 2] as row (row)}
            <div class="h-10 animate-pulse rounded-lg bg-[var(--surface-2)]"></div>
          {/each}
        </div>
      {:else if !data || displayRows.length === 0}
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
                <th class="px-4 py-2 font-medium">{breakdown === "agent" ? t("usage.agent") : t("usage.office")}</th>
                <th class="px-3 py-2 text-right font-medium">{t("usage.runs")}</th>
                <th class="px-3 py-2 text-right font-medium">{t("usage.tokens")}</th>
                <th class="px-3 py-2 text-right font-medium">{t("usage.failed")}</th>
                <th class="px-4 py-2 text-right font-medium">{t("usage.cost")}</th>
              </tr>
            </thead>
            <tbody>
              {#each displayRows as row (row.key)}
                {@const bot = breakdown === "agent" ? botFor(row.key) : undefined}
                <tr class="border-b border-[var(--hairline)] last:border-0">
                  <td class="px-4 py-2.5">
                    <div class="flex min-w-0 items-center gap-2">
                      <RavenAvatar
                        name={row.label}
                        imageUrl={bot?.avatar_url ?? null}
                        style={bot?.avatar_style ?? null}
                        decorative
                      />
                      <span class="truncate font-medium text-[var(--text-primary)]">{row.label}</span>
                    </div>
                  </td>
                  <td class="px-3 py-2.5 text-right font-mono tabular-nums text-[var(--text-secondary)]">{row.runs.toLocaleString()}</td>
                  <td class="px-3 py-2.5 text-right font-mono tabular-nums text-[var(--text-secondary)]">{row.tokens.toLocaleString()}</td>
                  <td class="px-3 py-2.5 text-right font-mono tabular-nums {row.failed > 0 ? 'text-[var(--danger-text)]' : 'text-[var(--text-muted)]'}">{row.failed.toLocaleString()}</td>
                  <td class="px-4 py-2.5 text-right">
                    <div class="ml-auto flex max-w-40 flex-col items-end gap-1">
                      <span class="font-mono tabular-nums text-[var(--text-primary)]">{money(row.cost)}</span>
                      {#if row.budget !== null && row.budget > 0}
                        <span class="text-[11px] text-[var(--text-muted)]">
                          {t("usage.ofBudget", {
                            percent: Math.round((row.cost / row.budget) * 100),
                            budget: money(row.budget),
                          })}
                        </span>
                      {/if}
                      <span class="h-1 w-full overflow-hidden rounded-full bg-[var(--surface-3)]">
                        <span
                          class="block h-full rounded-full bg-[var(--brand)]"
                          style="width: {Math.max(2, (row.cost / maxCost) * 100)}%"
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
