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
  import { notify } from "$lib/toast";
  import { Loader2, AlertTriangle, CheckCircle2, XCircle, Clock, Play, RefreshCw, RotateCcw } from "@lucide/svelte";

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

  interface RevertConflict {
    path: string;
    reason: string;
  }

  interface RevertReport {
    restored: string[];
    deleted: string[];
    skipped: RevertConflict[];
    conflicts: RevertConflict[];
  }

  let runs = $state<Run[]>([]);
  let isLoading = $state(false);
  let loadError = $state<string | null>(null);
  let resumingId = $state<string | null>(null);
  let previewingId = $state<string | null>(null);
  let revertingId = $state<string | null>(null);
  let revertPending = $state<{ run: Run; report: RevertReport } | null>(null);

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

  async function resumeRun(id: string) {
    if (resumingId) return;
    resumingId = id;
    try {
      await invoke("resume_run", { runId: id });
      notify(t("runs.resumed"), "success");
      await load();
    } catch (error) {
      notify(String(error), "error");
    } finally {
      resumingId = null;
    }
  }

  /** Ask what a run revert would do before changing any file. */
  async function previewRevert(run: Run) {
    if (previewingId) return;
    previewingId = run.id;
    try {
      const report = await invoke<RevertReport>("preview_run_revert", { runId: run.id });
      revertPending = { run, report };
    } catch (error) {
      notify(String(error), "info");
    } finally {
      previewingId = null;
    }
  }

  async function confirmRevert(force = false) {
    const pending = revertPending;
    if (!pending || revertingId) return;
    revertingId = pending.run.id;
    try {
      const report = await invoke<RevertReport>("revert_run_files", {
        runId: pending.run.id,
        force,
      });
      if (report.conflicts.length && !force) {
        revertPending = { run: pending.run, report };
        return;
      }
      const changed = report.restored.length + report.deleted.length;
      if (changed > 0) {
        notify(t("runs.revertDone"), "success");
      } else {
        notify(t("runs.revertNothing"), "info");
      }
      if (report.skipped.length) {
        notify(t("runs.revertSkipped", { count: report.skipped.length }), "info");
      }
      revertPending = null;
      await load();
    } catch (error) {
      notify(String(error), "error");
    } finally {
      revertingId = null;
    }
  }

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
            {#if run.state === "paused"}
              <button
                type="button"
                class="shrink-0 inline-flex items-center gap-1 h-6 px-2 rounded-md border border-[var(--brand)]/40 bg-[var(--brand-soft)] text-[11px] text-[var(--brand-text)] hover:bg-[var(--brand-soft)] cursor-pointer disabled:opacity-50"
                onclick={() => resumeRun(run.id)}
                disabled={resumingId === run.id}
                aria-label={t("runs.resume")}
                title={t("runs.resume")}
              >
                {#if resumingId === run.id}
                  <Loader2 class="size-3 animate-spin" />
                  {t("runs.resuming")}
                {:else}
                  <Play class="size-3" />
                  {t("runs.resume")}
                {/if}
              </button>
            {/if}
            {#if ["completed", "failed", "cancelled"].includes(run.state)}
              <button
                type="button"
                class="shrink-0 inline-flex items-center gap-1 h-6 px-2 rounded-md border border-[var(--hairline)] bg-[var(--surface-1)] text-[11px] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)] cursor-pointer disabled:opacity-50"
                onclick={() => void previewRevert(run)}
                disabled={previewingId === run.id}
                aria-label={t("runs.revert")}
                title={t("runs.revertTitle")}
              >
                {#if previewingId === run.id}
                  <Loader2 class="size-3 animate-spin" />
                {:else}
                  <RotateCcw class="size-3" />
                {/if}
                {t("runs.revert")}
              </button>
            {/if}
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

{#if revertPending}
  {@const pending = revertPending}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-50 bg-[var(--scrim)] flex items-center justify-center p-4"
    onclick={() => (revertPending = null)}
  >
    <div
      class="modal-panel w-full max-w-lg p-5 space-y-3"
      role="dialog"
      aria-modal="true"
      aria-labelledby="run-revert-title"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
    >
      <div class="flex items-center gap-2 border-b border-[var(--hairline)] pb-2">
        <RotateCcw class="size-4 text-[var(--brand-text)]" />
        <h2 id="run-revert-title" class="text-sm font-semibold text-[var(--text-primary)]">
          {t("runs.revertTitle")}
        </h2>
      </div>
      <p class="text-xs text-[var(--text-secondary)]">
        {t("runs.revertHint", {
          restored: pending.report.restored.length,
          deleted: pending.report.deleted.length,
        })}
      </p>
      {#if pending.report.conflicts.length}
        <p class="text-[11px] text-danger">{t("runs.revertConflicts")}</p>
        <ul class="space-y-1 max-h-40 overflow-y-auto rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] p-2">
          {#each pending.report.conflicts as conflict}
            <li class="text-[11px] text-[var(--text-secondary)]">
              <code class="font-mono text-[var(--text-primary)] break-all">{conflict.path}</code>
              <span class="block">{conflict.reason}</span>
            </li>
          {/each}
        </ul>
      {/if}
      {#if pending.report.restored.length + pending.report.deleted.length === 0 && !pending.report.conflicts.length}
        <p class="text-[11px] text-[var(--text-muted)]">{t("runs.revertNothing")}</p>
      {/if}
      <div class="flex justify-end gap-2 border-t border-[var(--hairline)] pt-3">
        <button
          type="button"
          class="h-7 px-3 rounded-lg border border-[var(--hairline)] text-[11px] text-[var(--text-secondary)] hover:bg-[var(--surface-3)] cursor-pointer"
          onclick={() => (revertPending = null)}
        >
          {t("projects.cancel")}
        </button>
        {#if pending.report.restored.length + pending.report.deleted.length > 0}
          <button
            type="button"
            class="h-7 px-3 rounded-lg border text-[11px] font-medium cursor-pointer disabled:opacity-50 {pending.report.conflicts.length
              ? 'border-[var(--danger-border)] bg-[var(--danger-soft)] text-[var(--danger)]'
              : 'border-transparent bg-[var(--surface-light)] text-[var(--text-on-light)]'}"
            disabled={revertingId === pending.run.id}
            onclick={() => void confirmRevert(pending.report.conflicts.length > 0)}
          >
            {#if revertingId === pending.run.id}
              <Loader2 class="size-3 animate-spin inline" />
            {/if}
            {pending.report.conflicts.length ? t("runs.revertAnyway") : t("runs.revert")}
          </button>
        {/if}
      </div>
    </div>
  </div>
{/if}
