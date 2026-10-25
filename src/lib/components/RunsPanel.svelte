<script lang="ts">
  /**
   * Runs & diagnostics panel.
   *
   * Every run RAVENBOT has ever attempted, newest first, with its state, cost,
   * timing and (when it failed) the error that killed it. This is the surface
   * that answers "why did that office run die?" — before it existed, the only
   * trace was a one-line error buried in a message and seven rows saying
   * "app quit while the run was active".
   *
   * Two things this deliberately is not:
   *  - Not a live dashboard. It reads on open; there is no polling, because a
   *    polling diagnostics view is a second source of truth fighting the one
   *    the user is already looking at.
   *  - Not a debugger. It shows what happened, not why in detail. The "why" is
   *    the error string the run recorded, and that is all it needs.
   */
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import { Loader2, AlertTriangle, CheckCircle2, XCircle, Clock, RefreshCw } from "@lucide/svelte";

  interface Run {
    id: string;
    bot_id: string;
    thread_id: string;
    state: string;
    tokens_consumed: number;
    cost_estimate: number;
    created_at: string;
    updated_at: string;
    completed_at: string | null;
    outcome: string | null;
  }

  let runs = $state<Run[]>([]);
  let isLoading = $state(false);
  let loadError = $state<string | null>(null);

  async function load() {
    isLoading = true;
    loadError = null;
    try {
      runs = await invoke<Run[]>("list_runs", { limit: 100 });
    } catch (e) {
      loadError = String(e);
      runs = [];
    } finally {
      isLoading = false;
    }
  }

  // Load on mount.
  $effect(() => { load(); });

  function stateColor(state: string): string {
    switch (state) {
      case "completed": return "text-success";
      case "failed": return "text-danger";
      case "cancelled": return "text-warning";
      case "paused": return "text-[var(--brand-text)]";
      default: return "text-[var(--text-tertiary)]";
    }
  }

  function stateIcon(state: string) {
    switch (state) {
      case "completed": return CheckCircle2;
      case "failed": return XCircle;
      case "cancelled": return AlertTriangle;
      default: return Clock;
    }
  }

  function fmtTime(iso: string): string {
    try {
      const d = new Date(iso);
      return d.toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
    } catch {
      return iso;
    }
  }

  function errMsg(run: Run): string | null {
    if (!run.outcome) return null;
    try {
      const o = JSON.parse(run.outcome);
      if (o?.Failure?.error) return String(o.Failure.error);
      if (o?.error) return String(o.error);
    } catch {}
    return null;
  }
</script>

<div class="space-y-3">
  <div class="flex items-center justify-between">
    <h3 class="text-xs font-bold uppercase tracking-wider text-[var(--text-tertiary)]">Runs & Diagnostics</h3>
    <button
      type="button"
      class="icon-btn size-7"
      onclick={load}
      disabled={isLoading}
      title="Refresh"
      aria-label="Refresh run list"
    >
      {#if isLoading}<Loader2 class="size-3.5 animate-spin" />{:else}<RefreshCw class="size-3.5" />{/if}
    </button>
  </div>

  {#if loadError}
    <p class="text-[11px] text-danger">{loadError}</p>
  {:else if isLoading && runs.length === 0}
    <p class="text-[11px] text-[var(--text-muted)]">Loading…</p>
  {:else if runs.length === 0}
    <p class="text-[11px] text-[var(--text-muted)]">No runs recorded yet.</p>
  {:else}
    <div class="space-y-1.5 max-h-72 overflow-y-auto no-scrollbar">
      {#each runs as run (run.id)}
        {@const Icon = stateIcon(run.state)}
        <div class="rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-3 py-2 space-y-1">
          <div class="flex items-center gap-2 min-w-0">
            <Icon class="size-3.5 shrink-0 {stateColor(run.state)}" />
            <span class="text-[11px] font-mono text-[var(--text-tertiary)] truncate">{run.id.slice(0, 8)}</span>
            <span class="text-[11px] font-semibold {stateColor(run.state)} capitalize shrink-0">{run.state}</span>
            <span class="ml-auto text-[11px] text-[var(--text-faint)] tabular-nums shrink-0">{fmtTime(run.created_at)}</span>
          </div>
          <div class="flex items-center gap-3 text-[11px] font-mono text-[var(--text-muted)]">
            <span>{run.tokens_consumed.toLocaleString()} tok</span>
            <span>${run.cost_estimate.toFixed(4)}</span>
            {#if run.completed_at}
              <span class="ml-auto">{fmtTime(run.completed_at)}</span>
            {/if}
          </div>
          {#if errMsg(run)}
            <p class="text-[11px] text-danger leading-snug break-words">{errMsg(run)}</p>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>
