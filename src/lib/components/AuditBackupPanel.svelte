<script lang="ts">
  /**
   * Scheduled signed audit backups.
   *
   * The backend writes the backup before automatic retention runs. If a due
   * backup fails, retention is paused so entries cannot disappear before they
   * are backed up.
   */
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { t } from "$lib/i18n";
  import {
    AlertTriangle,
    FolderOpen,
    HardDriveDownload,
    Loader2,
    RefreshCw,
  } from "@lucide/svelte";

  interface ExportSchedule {
    directory: string | null;
    interval_hours: number;
    last_at: string | null;
    last_error: string | null;
    exported?: {
      path: string;
      count: number;
    } | null;
  }

  let directory = $state("");
  let interval = $state("0");
  let lastAt = $state<string | null>(null);
  let lastError = $state<string | null>(null);
  let loading = $state(true);
  let busy = $state(false);
  let notice = $state("");
  let error = $state("");

  async function load() {
    loading = true;
    error = "";
    try {
      const schedule = await invoke<ExportSchedule>("get_audit_export_schedule");
      directory = schedule.directory || "";
      interval = String(schedule.interval_hours || 0);
      lastAt = schedule.last_at || null;
      lastError = schedule.last_error || null;
    } catch (cause) {
      error = String(cause);
    } finally {
      loading = false;
    }
  }

  async function chooseFolder() {
    try {
      const selected = await openDialog({
        directory: true,
        multiple: false,
      });
      const path = Array.isArray(selected) ? selected[0] : selected;
      if (path) directory = path;
    } catch (cause) {
      error = String(cause);
    }
  }

  async function save() {
    if (busy) return;
    const hours = Number(interval);
    if (hours > 0 && !directory.trim()) {
      error = t("audit.backupNeedFolder");
      return;
    }
    busy = true;
    notice = "";
    error = "";
    try {
      const schedule = await invoke<ExportSchedule>("set_audit_export_schedule", {
        directory: directory.trim() || null,
        intervalHours: hours,
      });
      directory = schedule.directory || "";
      interval = String(schedule.interval_hours || 0);
      lastAt = schedule.last_at || null;
      lastError = schedule.last_error || null;
      if (schedule.exported?.path) {
        notice = t("audit.backupSaved", {
          count: schedule.exported.count,
          path: schedule.exported.path,
        });
      } else {
        notice = t("audit.backupScheduled");
      }
    } catch (cause) {
      error = String(cause);
    } finally {
      busy = false;
    }
  }

  function fmtTime(value: string | null): string {
    if (!value) return t("audit.backupNever");
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    return date.toLocaleString(undefined, {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  }

  onMount(() => void load());
</script>

<section class="space-y-2" aria-label={t("audit.backupTitle")}>
  <div class="flex items-start justify-between gap-2">
    <div>
      <p class="text-[11px] font-semibold text-[var(--text-primary)]">
        <HardDriveDownload class="mr-1 inline size-3.5" aria-hidden="true" />
        {t("audit.backupTitle")}
      </p>
      <p class="mt-0.5 text-[11px] leading-4 text-[var(--text-tertiary)]">
        {t("audit.backupDesc")}
      </p>
    </div>
    <button
      type="button"
      class="icon-btn size-6 shrink-0"
      onclick={() => void load()}
      disabled={loading || busy}
      title={t("audit.refresh")}
      aria-label={t("audit.refresh")}
    >
      {#if loading}
        <Loader2 class="size-3.5 animate-spin" />
      {:else}
        <RefreshCw class="size-3.5" />
      {/if}
    </button>
  </div>

  <div class="flex flex-wrap items-end gap-1.5">
    <label class="min-w-48 flex-1 text-[11px] text-[var(--text-secondary)]">
      <span class="mb-0.5 block">{t("audit.backupFolder")}</span>
      <input
        class="h-7 w-full rounded-lg border border-[var(--hairline)] bg-[var(--surface-1)] px-2 font-mono text-[11px] text-[var(--text-primary)]"
        value={directory}
        readonly
        placeholder={t("audit.backupNone")}
        aria-label={t("audit.backupFolder")}
      />
    </label>
    <button
      type="button"
      class="inline-flex h-7 items-center gap-1.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-1)] px-2.5 text-[11px] font-semibold text-[var(--text-secondary)] hover:bg-[var(--surface-2)] hover:text-[var(--text-primary)] cursor-pointer"
      onclick={() => void chooseFolder()}
      disabled={busy}
    >
      <FolderOpen class="size-3.5" />
      {t("audit.backupChoose")}
    </button>
    <label class="text-[11px] text-[var(--text-secondary)]">
      <span class="mb-0.5 block">{t("audit.backupInterval")}</span>
      <select
        class="h-7 rounded-lg border border-[var(--hairline)] bg-[var(--surface-1)] px-2 text-[11px] text-[var(--text-primary)]"
        value={interval}
        onchange={(event) => (interval = event.currentTarget.value)}
        disabled={busy}
      >
        <option value="0">{t("audit.backupOff")}</option>
        <option value="24">{t("audit.backupDaily")}</option>
        <option value="168">{t("audit.backupWeekly")}</option>
      </select>
    </label>
    <button
      type="button"
      class="inline-flex h-7 items-center gap-1.5 rounded-lg border border-[var(--brand)]/30 bg-[var(--brand-soft)] px-2.5 text-[11px] font-semibold text-[var(--brand-text)] hover:bg-[var(--surface-2)] cursor-pointer disabled:opacity-50"
      onclick={() => void save()}
      disabled={busy || loading}
    >
      {#if busy}
        <Loader2 class="size-3.5 animate-spin" />
        {t("audit.backupSaving")}
      {:else}
        {t("audit.backupSave")}
      {/if}
    </button>
  </div>

  <p class="text-[11px] text-[var(--text-muted)]">
    {t("audit.backupLast", { time: fmtTime(lastAt) })}
  </p>
  {#if lastError}
    <p class="flex items-start gap-1.5 text-[11px] text-[var(--danger-text)]" role="alert">
      <AlertTriangle class="mt-0.5 size-3.5 shrink-0" />
      <span class="break-words">{t("audit.backupError", { error: lastError })}</span>
    </p>
  {/if}
  {#if notice}
    <p class="text-[11px] text-[var(--success-text)] break-words" role="status">{notice}</p>
  {/if}
  {#if error}
    <p class="text-[11px] text-[var(--danger-text)] break-words" role="alert">{error}</p>
  {/if}
</section>
