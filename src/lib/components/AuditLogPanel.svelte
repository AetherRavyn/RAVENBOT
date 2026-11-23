<script lang="ts">
  /**
   * Global audit log.
   *
   * The runtime already recorded tool calls; this is the surface that makes
   * them inspectable across every agent, with model spend and file/network
   * access alongside them. Rows are projected by the backend — tool arguments
   * never leave the database — and a thread id opens the owning conversation.
   */
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import AuditBackupPanel from "$lib/components/AuditBackupPanel.svelte";
  import { t } from "$lib/i18n";
  import { requestThreadFocus } from "$lib/chat/messageFocus";
  import { workspace } from "$lib/workspace.svelte";
  import type { AuditLogEntry, AuditLogPage } from "$lib/audit";
  import {
    Bot,
    Brain,
    ChevronLeft,
    ChevronRight,
    Cpu,
    Download,
    ExternalLink,
    FileText,
    Globe,
    HardDriveDownload,
    Loader2,
    Pause,
    Play,
    RefreshCw,
    Search,
    ShieldAlert,
    ShieldCheck,
    Square,
    Trash2,
    Wrench,
  } from "@lucide/svelte";

  interface Props {
    bots?: any[];
    onClose?: () => void;
  }

  let { bots = [], onClose }: Props = $props();

  const PAGE_SIZE = 50;
  type Range = "24h" | "7d" | "30d" | "all";

  let entries = $state<AuditLogEntry[]>([]);
  let total = $state(0);
  let loading = $state(false);
  let error = $state("");
  let botFilter = $state("");
  let kindFilter = $state("all");
  let range = $state<Range>("7d");
  let search = $state("");
  let offset = $state(0);
  let searchTimer: ReturnType<typeof setTimeout> | null = null;
  let requestSeq = 0;
  let exportBusy = $state(false);
  let exportNotice = $state("");
  let exportDigest = $state("");
  let exportError = $state("");
  let verifyBusy = $state(false);
  let verifyNotice = $state("");
  let verifyError = $state("");
  let showRetention = $state(false);
  let showBackup = $state(false);
  let retentionPolicy = $state("0");
  let retentionDraft = $state("0");
  let policyBusy = $state(false);
  let policyNotice = $state("");
  let policyError = $state("");
  let retentionDays = $state("90");
  let pruning = $state(false);
  let retentionNotice = $state("");
  let retentionError = $state("");

  const knownBots = $derived(new Set(bots.map((bot: any) => String(bot.id))));
  const hasPrev = $derived(offset > 0);
  const hasNext = $derived(offset + entries.length < total);
  const pageStart = $derived(entries.length ? offset + 1 : 0);
  const pageEnd = $derived(offset + entries.length);

  function rangeSince(): string | null {
    const now = Date.now();
    switch (range) {
      case "24h":
        return new Date(now - 24 * 60 * 60 * 1000).toISOString();
      case "7d":
        return new Date(now - 7 * 24 * 60 * 60 * 1000).toISOString();
      case "30d":
        return new Date(now - 30 * 24 * 60 * 60 * 1000).toISOString();
      default:
        return null;
    }
  }

  async function load() {
    const seq = ++requestSeq;
    loading = true;
    error = "";
    try {
      const page = await invoke<AuditLogPage>("list_audit_entries", {
        botId: botFilter || null,
        kinds: kindFilter === "all" ? [] : [kindFilter],
        search: search.trim() || null,
        since: rangeSince(),
        until: null,
        limit: PAGE_SIZE,
        offset,
      });
      if (seq !== requestSeq) return;
      entries = page.entries;
      total = page.total;
    } catch (cause) {
      if (seq !== requestSeq) return;
      error = String(cause);
      entries = [];
      total = 0;
    } finally {
      if (seq === requestSeq) loading = false;
    }
  }

  function resetAndLoad() {
    offset = 0;
    void load();
  }

  function queueSearch() {
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = setTimeout(() => {
      searchTimer = null;
      resetAndLoad();
    }, 250);
  }

  function goPage(direction: -1 | 1) {
    const next = offset + direction * PAGE_SIZE;
    if (next < 0 || next >= total) return;
    offset = next;
    void load();
  }

  function currentFilters() {
    return {
      botId: botFilter || null,
      kinds: kindFilter === "all" ? [] : [kindFilter],
      search: search.trim() || null,
      since: rangeSince(),
      until: null,
    };
  }

  async function exportAudit() {
    if (exportBusy) return;
    exportBusy = true;
    exportNotice = "";
    exportDigest = "";
    exportError = "";
    try {
      const path = await saveDialog({
        defaultPath: `ravenbot-audit-${new Date().toISOString().slice(0, 10)}.json`,
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!path) return;
      const result = await invoke<any>("export_audit_log", {
        path,
        ...currentFilters(),
      });
      exportNotice = t("audit.exported", {
        count: result.count,
        path: result.path,
      });
      exportDigest = String(result.sha256 || "");
    } catch (cause) {
      exportError = String(cause);
    } finally {
      exportBusy = false;
    }
  }

  async function verifyExport() {
    if (verifyBusy) return;
    verifyBusy = true;
    verifyNotice = "";
    verifyError = "";
    try {
      const selected = await openDialog({
        multiple: false,
        directory: false,
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      const path = Array.isArray(selected) ? selected[0] : selected;
      if (!path) return;
      const result = await invoke<any>("verify_audit_export", { path });
      verifyNotice = t(
        result.trusted === true ? "audit.verified" : "audit.verifiedOther",
        {
          count: result.count,
          time: fmtTime(String(result.exported_at || "")),
        },
      );
    } catch (cause) {
      verifyError = String(cause);
    } finally {
      verifyBusy = false;
    }
  }

  async function loadRetentionPolicy() {
    try {
      const days = await invoke<number>("get_audit_retention");
      retentionPolicy = String(days);
      retentionDraft = String(days);
    } catch {
      /* keep the safe forever default */
    }
  }

  async function saveRetentionPolicy() {
    if (policyBusy) return;
    policyBusy = true;
    policyNotice = "";
    policyError = "";
    try {
      const result = await invoke<{ days: number; deleted: number }>(
        "set_audit_retention",
        { days: Number(retentionDraft) },
      );
      retentionPolicy = String(result.days);
      retentionDraft = String(result.days);
      policyNotice =
        result.days === 0
          ? t("audit.policyForeverSaved")
          : t("audit.policySaved", { days: result.days });
      if (result.deleted > 0) {
        policyNotice += ` ${t("audit.policyDeleted", { count: result.deleted })}`;
        await load();
      }
    } catch (cause) {
      policyError = String(cause);
    } finally {
      policyBusy = false;
    }
  }

  async function pruneAudit() {
    if (pruning) return;
    pruning = true;
    retentionNotice = "";
    retentionError = "";
    try {
      const deleted = await invoke<number>("prune_audit_log", {
        olderThanDays: Number(retentionDays),
      });
      showRetention = false;
      retentionNotice =
        deleted === 0
          ? t("audit.prunedNone")
          : t("audit.pruned", { count: deleted });
      if (deleted > 0) await load();
    } catch (cause) {
      retentionError = String(cause);
    } finally {
      pruning = false;
    }
  }

  onMount(() => {
    void load();
    void loadRetentionPolicy();
    return () => {
      if (searchTimer) clearTimeout(searchTimer);
    };
  });

  function kindLabel(kind: string): string {
    switch (kind) {
      case "ModelCall":
        return t("audit.kind.model");
      case "ToolCall":
        return t("audit.kind.tool");
      case "NetworkRequest":
        return t("audit.kind.network");
      case "FileRead":
        return t("audit.kind.fileRead");
      case "FileWrite":
        return t("audit.kind.fileWrite");
      case "PermissionGranted":
        return t("audit.kind.granted");
      case "PermissionDenied":
        return t("audit.kind.denied");
      case "BotStarted":
        return t("audit.kind.started");
      case "BotPaused":
        return t("audit.kind.paused");
      case "BotStopped":
        return t("audit.kind.stopped");
      case "MemoryChanged":
        return t("audit.kind.memory");
      case "MemoryEmbeddingsUpdated":
        return t("audit.kind.embeddings");
      default:
        return kind;
    }
  }

  function memoryActionLabel(action: string | null): string {
    switch (action) {
      case "created":
        return t("audit.memoryCreated");
      case "updated":
        return t("audit.memoryUpdated");
      case "deleted":
        return t("audit.memoryDeleted");
      default:
        return action || t("audit.memoryUpdated");
    }
  }

  function actorLabel(entry: AuditLogEntry): string {
    if (entry.bot_name) return entry.bot_name;
    if (entry.kind === "MemoryChanged" && entry.memory_scope === "office") {
      return t("audit.officeMemory");
    }
    if (entry.kind === "MemoryEmbeddingsUpdated") {
      return t("audit.memorySettings");
    }
    return t("audit.deletedAgent");
  }

  function kindIcon(kind: string) {
    switch (kind) {
      case "ModelCall":
        return Cpu;
      case "ToolCall":
        return Wrench;
      case "NetworkRequest":
        return Globe;
      case "FileRead":
      case "FileWrite":
        return FileText;
      case "PermissionGranted":
        return ShieldCheck;
      case "PermissionDenied":
        return ShieldAlert;
      case "BotPaused":
        return Pause;
      case "BotStopped":
        return Square;
      case "BotStarted":
        return Play;
      case "MemoryChanged":
      case "MemoryEmbeddingsUpdated":
        return Brain;
      default:
        return Bot;
    }
  }

  function kindTone(kind: string): string {
    switch (kind) {
      case "PermissionDenied":
        return "border-[var(--danger-border)] bg-[var(--danger-soft)] text-[var(--danger-text)]";
      case "ModelCall":
        return "border-[var(--brand)]/25 bg-[var(--brand-soft)] text-[var(--brand-text)]";
      case "PermissionGranted":
        return "border-[var(--success-text)]/25 bg-[var(--success-soft)] text-[var(--success-text)]";
      default:
        return "border-[var(--hairline)] bg-[var(--surface-2)] text-[var(--text-secondary)]";
    }
  }

  function fmtTime(iso: string): string {
    const date = new Date(iso);
    if (Number.isNaN(date.getTime())) return iso;
    return date.toLocaleString(undefined, {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
  }

  function formatBytes(size: number): string {
    if (size < 1024) return `${size} B`;
    if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
    return `${(size / (1024 * 1024)).toFixed(1)} MB`;
  }

  function canOpenThread(entry: AuditLogEntry): boolean {
    return Boolean(entry.thread_id && entry.bot_id && knownBots.has(entry.bot_id));
  }

  function openThread(entry: AuditLogEntry) {
    if (!entry.thread_id) return;
    requestThreadFocus({ botId: entry.bot_id, threadId: entry.thread_id });
    workspace.selectBot(entry.bot_id);
    onClose?.();
  }

  function shortId(value: string | null): string {
    return value ? value.slice(0, 8) : "";
  }
</script>

<div class="space-y-3">
  <div class="flex flex-wrap items-start justify-between gap-2">
    <div>
      <h3 class="text-xs font-bold uppercase tracking-wider text-[var(--text-tertiary)]">
        {t("audit.title")}
      </h3>
      <p class="mt-1 max-w-2xl text-[11px] leading-5 text-[var(--text-muted)]">
        {t("audit.description")}
      </p>
    </div>
    <div class="flex flex-wrap items-center justify-end gap-1.5">
      <button
        type="button"
        class="inline-flex h-7 items-center gap-1.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-1)] px-2.5 text-[11px] font-semibold text-[var(--text-secondary)] hover:bg-[var(--surface-2)] hover:text-[var(--text-primary)] cursor-pointer disabled:opacity-50"
        onclick={() => void exportAudit()}
        disabled={exportBusy}
        title={t("audit.export")}
      >
        {#if exportBusy}
          <Loader2 class="size-3.5 animate-spin" />
        {:else}
          <Download class="size-3.5" />
        {/if}
        {t("audit.export")}
      </button>
      <button
        type="button"
        class="inline-flex h-7 items-center gap-1.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-1)] px-2.5 text-[11px] font-semibold text-[var(--text-secondary)] hover:bg-[var(--surface-2)] hover:text-[var(--text-primary)] cursor-pointer disabled:opacity-50"
        onclick={() => void verifyExport()}
        disabled={verifyBusy}
        title={t("audit.verify")}
      >
        {#if verifyBusy}
          <Loader2 class="size-3.5 animate-spin" />
        {:else}
          <ShieldCheck class="size-3.5" />
        {/if}
        {t("audit.verify")}
      </button>
      <button
        type="button"
        class="inline-flex h-7 items-center gap-1.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-1)] px-2.5 text-[11px] font-semibold text-[var(--text-secondary)] hover:bg-[var(--surface-2)] hover:text-[var(--text-primary)] cursor-pointer disabled:opacity-50"
        onclick={() => {
          showBackup = !showBackup;
          showRetention = false;
        }}
        disabled={loading}
        aria-expanded={showBackup}
        title={t("audit.backupTitle")}
      >
        <HardDriveDownload class="size-3.5" />
        {t("audit.backup")}
      </button>
      <button
        type="button"
        class="inline-flex h-7 items-center gap-1.5 rounded-lg border border-[var(--danger-border)] bg-[var(--danger-soft)] px-2.5 text-[11px] font-semibold text-[var(--danger-text)] hover:bg-[var(--surface-2)] cursor-pointer disabled:opacity-50"
        onclick={() => {
          showRetention = !showRetention;
          showBackup = false;
        }}
        disabled={pruning}
        aria-expanded={showRetention}
        title={t("audit.retentionTitle")}
      >
        <Trash2 class="size-3.5" />
        {t("audit.retention")}
      </button>
      <button
        type="button"
        class="inline-flex h-7 items-center gap-1.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-1)] px-2.5 text-[11px] font-semibold text-[var(--text-secondary)] hover:bg-[var(--surface-2)] hover:text-[var(--text-primary)] cursor-pointer disabled:opacity-50"
        onclick={() => void load()}
        disabled={loading}
        title={t("audit.refresh")}
        aria-label={t("audit.refresh")}
      >
        {#if loading}
          <Loader2 class="size-3.5 animate-spin" />
        {:else}
          <RefreshCw class="size-3.5" />
        {/if}
        {t("audit.refresh")}
      </button>
    </div>
  </div>

  {#if exportNotice}
    <div class="rounded-lg border border-[var(--success-text)]/30 bg-[var(--success-soft)] px-3 py-2 text-[11px] text-[var(--success-text)]" role="status">
      <p class="break-words">{exportNotice}</p>
      {#if exportDigest}
        <p class="mt-1 break-all font-mono text-[11px] opacity-80">SHA-256 {exportDigest}</p>
      {/if}
    </div>
  {/if}
  {#if exportError}
    <p class="rounded-lg border border-[var(--danger-border)] bg-[var(--danger-soft)] px-3 py-2 text-[11px] text-[var(--danger-text)] break-words" role="alert">
      {exportError}
    </p>
  {/if}
  {#if verifyNotice}
    <p class="rounded-lg border border-[var(--success-text)]/30 bg-[var(--success-soft)] px-3 py-2 text-[11px] text-[var(--success-text)] break-words" role="status">
      {verifyNotice}
    </p>
  {/if}
  {#if verifyError}
    <p class="rounded-lg border border-[var(--danger-border)] bg-[var(--danger-soft)] px-3 py-2 text-[11px] text-[var(--danger-text)] break-words" role="alert">
      {verifyError}
    </p>
  {/if}

  <div class="flex flex-wrap items-center gap-1.5">
    <label class="sr-only" for="audit-bot-filter">{t("audit.allAgents")}</label>
    <select
      id="audit-bot-filter"
      class="h-8 max-w-44 rounded-lg border border-[var(--hairline)] bg-[var(--surface-1)] px-2 text-[11px] text-[var(--text-primary)]"
      value={botFilter}
      onchange={(event) => {
        botFilter = event.currentTarget.value;
        resetAndLoad();
      }}
    >
      <option value="">{t("audit.allAgents")}</option>
      {#each bots as bot (bot.id)}
        <option value={bot.id}>{bot.name}</option>
      {/each}
    </select>

    <label class="sr-only" for="audit-kind-filter">{t("audit.allKinds")}</label>
    <select
      id="audit-kind-filter"
      class="h-8 rounded-lg border border-[var(--hairline)] bg-[var(--surface-1)] px-2 text-[11px] text-[var(--text-primary)]"
      value={kindFilter}
      onchange={(event) => {
        kindFilter = event.currentTarget.value;
        resetAndLoad();
      }}
    >
      <option value="all">{t("audit.allKinds")}</option>
      <option value="ModelCall">{t("audit.kind.model")}</option>
      <option value="ToolCall">{t("audit.kind.tool")}</option>
      <option value="NetworkRequest">{t("audit.kind.network")}</option>
      <option value="FileRead">{t("audit.kind.fileRead")}</option>
      <option value="FileWrite">{t("audit.kind.fileWrite")}</option>
      <option value="PermissionGranted">{t("audit.kind.granted")}</option>
      <option value="PermissionDenied">{t("audit.kind.denied")}</option>
      <option value="BotStarted">{t("audit.kind.started")}</option>
      <option value="BotPaused">{t("audit.kind.paused")}</option>
      <option value="BotStopped">{t("audit.kind.stopped")}</option>
      <option value="MemoryChanged">{t("audit.kind.memory")}</option>
      <option value="MemoryEmbeddingsUpdated">{t("audit.kind.embeddings")}</option>
    </select>

    <label class="sr-only" for="audit-range">{t("audit.range24h")}</label>
    <select
      id="audit-range"
      class="h-8 rounded-lg border border-[var(--hairline)] bg-[var(--surface-1)] px-2 text-[11px] text-[var(--text-primary)]"
      value={range}
      onchange={(event) => {
        range = event.currentTarget.value as Range;
        resetAndLoad();
      }}
    >
      <option value="24h">{t("audit.range24h")}</option>
      <option value="7d">{t("audit.range7d")}</option>
      <option value="30d">{t("audit.range30d")}</option>
      <option value="all">{t("audit.rangeAll")}</option>
    </select>

    <div class="relative min-w-48 flex-1">
      <Search
        class="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-[var(--text-muted)]"
        aria-hidden="true"
      />
      <input
        type="search"
        class="h-8 w-full rounded-lg border border-[var(--hairline)] bg-[var(--surface-1)] pl-8 pr-2 text-[11px] text-[var(--text-primary)] placeholder:text-[var(--text-muted)]"
        placeholder={t("audit.search")}
        aria-label={t("audit.searchLabel")}
        value={search}
        oninput={(event) => {
          search = event.currentTarget.value;
          queueSearch();
        }}
      />
    </div>
  </div>

  <p class="text-[11px] leading-4 text-[var(--text-muted)]">
    {t("audit.argumentsRedacted")}
  </p>

  {#if showBackup}
    <div class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] p-3">
      <AuditBackupPanel />
    </div>
  {/if}

  {#if showRetention}
    <div
      class="rounded-xl border border-[var(--danger-border)] bg-[var(--danger-soft)] p-3 space-y-2"
      role="group"
      aria-label={t("audit.retentionTitle")}
    >
      <div>
        <p class="text-[11px] font-semibold text-[var(--danger-text)]">
          {t("audit.retentionTitle")}
        </p>
      </div>
      <div class="space-y-1">
        <p class="text-[11px] font-semibold text-[var(--text-secondary)]">
          {t("audit.autoRetention")}
        </p>
        <p class="text-[11px] leading-4 text-[var(--text-secondary)]">
          {t("audit.autoRetentionHint")}
        </p>
        <div class="flex flex-wrap items-center gap-1.5">
          <label class="text-[11px] text-[var(--text-secondary)]" for="audit-auto-retention">
            {t("audit.autoRetention")}
          </label>
          <select
            id="audit-auto-retention"
            class="h-7 rounded-lg border border-[var(--hairline)] bg-[var(--surface-1)] px-2 text-[11px] text-[var(--text-primary)]"
            value={retentionDraft}
            onchange={(event) => (retentionDraft = event.currentTarget.value)}
            disabled={policyBusy}
          >
            <option value="0">{t("audit.retentionForever")}</option>
            <option value="30">{t("audit.days", { count: 30 })}</option>
            <option value="90">{t("audit.days", { count: 90 })}</option>
            <option value="180">{t("audit.days", { count: 180 })}</option>
            <option value="365">{t("audit.days", { count: 365 })}</option>
            <option value="730">{t("audit.days", { count: 730 })}</option>
          </select>
          <button
            type="button"
            class="inline-flex h-7 items-center gap-1.5 rounded-lg border border-[var(--brand)]/30 bg-[var(--brand-soft)] px-2.5 text-[11px] font-semibold text-[var(--brand-text)] hover:bg-[var(--surface-2)] cursor-pointer disabled:opacity-50"
            onclick={() => void saveRetentionPolicy()}
            disabled={policyBusy || retentionDraft === retentionPolicy}
          >
            {#if policyBusy}
              <Loader2 class="size-3.5 animate-spin" />
              {t("audit.savingPolicy")}
            {:else}
              {t("audit.savePolicy")}
            {/if}
          </button>
        </div>
        {#if policyNotice}
          <p class="text-[11px] text-[var(--success-text)] break-words" role="status">
            {policyNotice}
          </p>
        {/if}
        {#if policyError}
          <p class="text-[11px] text-[var(--danger-text)] break-words" role="alert">
            {policyError}
          </p>
        {/if}
      </div>
      <div class="border-t border-[var(--danger-border)]"></div>
      <p class="text-[11px] font-semibold text-[var(--text-secondary)]">
        {t("audit.manualRetention")}
      </p>
      <p class="text-[11px] leading-4 text-[var(--text-secondary)]">
        {t("audit.retentionBody")}
      </p>
      <div class="flex flex-wrap items-center gap-1.5">
        <label class="text-[11px] text-[var(--text-secondary)]" for="audit-retention-days">
          {t("audit.deleteOlderThan")}
        </label>
        <select
          id="audit-retention-days"
          class="h-7 rounded-lg border border-[var(--hairline)] bg-[var(--surface-1)] px-2 text-[11px] text-[var(--text-primary)]"
          value={retentionDays}
          onchange={(event) => (retentionDays = event.currentTarget.value)}
          disabled={pruning}
        >
          <option value="30">{t("audit.days", { count: 30 })}</option>
          <option value="90">{t("audit.days", { count: 90 })}</option>
          <option value="180">{t("audit.days", { count: 180 })}</option>
          <option value="365">{t("audit.days", { count: 365 })}</option>
          <option value="730">{t("audit.days", { count: 730 })}</option>
        </select>
        <button
          type="button"
          class="inline-flex h-7 items-center gap-1.5 rounded-lg border border-[var(--danger-border)] bg-[var(--surface-1)] px-2.5 text-[11px] font-semibold text-[var(--danger-text)] hover:bg-[var(--surface-2)] cursor-pointer disabled:opacity-50"
          onclick={() => void pruneAudit()}
          disabled={pruning}
        >
          {#if pruning}
            <Loader2 class="size-3.5 animate-spin" />
          {:else}
            <Trash2 class="size-3.5" />
          {/if}
          {pruning ? t("audit.pruning") : t("audit.prune")}
        </button>
        <button
          type="button"
          class="inline-flex h-7 items-center rounded-lg border border-[var(--hairline)] bg-[var(--surface-1)] px-2.5 text-[11px] font-semibold text-[var(--text-secondary)] hover:bg-[var(--surface-2)] cursor-pointer disabled:opacity-50"
          onclick={() => (showRetention = false)}
          disabled={pruning}
        >{t("ui.cancel")}</button>
      </div>
      {#if retentionError}
        <p class="text-[11px] text-[var(--danger-text)] break-words" role="alert">
          {retentionError}
        </p>
      {/if}
    </div>
  {/if}
  {#if retentionNotice}
    <p class="rounded-lg border border-[var(--success-text)]/30 bg-[var(--success-soft)] px-3 py-2 text-[11px] text-[var(--success-text)]" role="status">
      {retentionNotice}
    </p>
  {/if}

  {#if error}
    <div class="rounded-xl border border-[var(--danger-border)] bg-[var(--danger-soft)] p-3" role="alert">
      <p class="text-[11px] text-[var(--danger-text)] break-words">{error}</p>
      <button
        type="button"
        class="mt-2 inline-flex h-7 items-center rounded-lg border border-[var(--danger-border)] px-2.5 text-[11px] font-semibold text-[var(--danger-text)] hover:bg-[var(--surface-1)] cursor-pointer"
        onclick={() => void load()}
      >{t("audit.refresh")}</button>
    </div>
  {:else if loading && entries.length === 0}
    <div class="flex items-center justify-center gap-2 py-12 text-[11px] text-[var(--text-tertiary)]" role="status">
      <Loader2 class="size-4 animate-spin" />
      {t("audit.loading")}
    </div>
  {:else if entries.length === 0}
    <div class="rounded-xl border border-dashed border-[var(--hairline-strong)] bg-[var(--surface-1)] px-4 py-10 text-center">
      <p class="text-[11px] text-[var(--text-muted)]">{t("audit.empty")}</p>
    </div>
  {:else}
    <div class="max-h-[26rem] space-y-1.5 overflow-y-auto no-scrollbar">
      {#each entries as entry (entry.id)}
        {@const Icon = kindIcon(entry.kind)}
        <div class="rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-3 py-2 space-y-1">
          <div class="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
            <span
              class="inline-flex h-5 shrink-0 items-center gap-1 rounded-full border px-1.5 text-[11px] font-mono {kindTone(entry.kind)}"
            >
              <Icon class="size-3" aria-hidden="true" />
              {kindLabel(entry.kind)}
            </span>
            <span class="truncate text-[11px] font-semibold text-[var(--text-secondary)]">
              {actorLabel(entry)}
            </span>
            <span class="ml-auto shrink-0 text-[11px] tabular-nums text-[var(--text-faint)]">
              {fmtTime(entry.timestamp)}
            </span>
          </div>

          <div class="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-[var(--text-tertiary)]">
            {#if entry.kind === "ModelCall"}
              <span class="font-mono text-[var(--text-secondary)]">{entry.provider}/{entry.model}</span>
              {#if entry.tokens_in !== null}
                <span class="tabular-nums">{entry.tokens_in.toLocaleString()} in</span>
              {/if}
              {#if entry.tokens_out !== null}
                <span class="tabular-nums">{entry.tokens_out.toLocaleString()} out</span>
              {/if}
              {#if entry.cost !== null}
                <span class="tabular-nums">${entry.cost.toFixed(4)}</span>
              {/if}
            {:else if entry.kind === "ToolCall"}
              <span class="font-mono text-[var(--text-secondary)]">{entry.tool_name}</span>
            {:else if entry.kind === "NetworkRequest"}
              <span class="font-mono uppercase">{entry.method}</span>
              <span class="min-w-0 break-all font-mono">{entry.url}</span>
            {:else if entry.kind === "FileRead" || entry.kind === "FileWrite"}
              <span class="min-w-0 break-all font-mono text-[var(--text-secondary)]">{entry.path}</span>
              {#if entry.size !== null}
                <span class="tabular-nums">{formatBytes(entry.size)}</span>
              {/if}
            {:else if entry.kind === "PermissionGranted" || entry.kind === "PermissionDenied"}
              <span class="font-mono text-[var(--text-secondary)]">{entry.permission}</span>
              {#if entry.reason}<span class="min-w-0 break-words">{entry.reason}</span>{/if}
            {:else if entry.kind === "MemoryEmbeddingsUpdated"}
              <span class="font-mono text-[var(--text-secondary)]">{entry.provider}/{entry.model}</span>
              <span class="min-w-0 break-all font-mono">{entry.url}</span>
              {#if entry.size !== null}
                <span>{t("audit.embeddingsReindexed", { count: entry.size })}</span>
              {/if}
            {:else if entry.kind === "MemoryChanged"}
              <span class="font-mono text-[var(--text-secondary)]">
                {entry.memory_scope === "office"
                  ? t("audit.memoryOffice")
                  : t("audit.memoryAgent")}
                · {memoryActionLabel(entry.memory_action)}
              </span>
              {#if entry.category}
                <span class="font-mono">{entry.category}</span>
              {/if}
              {#if entry.memory_id}
                <span class="font-mono">{shortId(entry.memory_id)}</span>
              {/if}
            {/if}
          </div>

          {#if entry.run_id || canOpenThread(entry)}
            <div class="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-[var(--text-muted)]">
              {#if entry.run_id}
                <span class="font-mono">{t("audit.run")} {shortId(entry.run_id)}</span>
              {/if}
              {#if canOpenThread(entry)}
                <button
                  type="button"
                  class="inline-flex min-w-0 items-center gap-1 text-[var(--brand-text)] hover:underline cursor-pointer"
                  onclick={() => openThread(entry)}
                  title={t("audit.openThread")}
                >
                  <ExternalLink class="size-3 shrink-0" aria-hidden="true" />
                  <span class="truncate">
                    {entry.thread_title || t("audit.untitledThread")}
                  </span>
                </button>
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    </div>

    <div class="flex items-center justify-between gap-2 pt-1">
      <p class="text-[11px] tabular-nums text-[var(--text-muted)]">
        {t("audit.page", { from: pageStart, to: pageEnd, total })}
      </p>
      <div class="flex items-center gap-1.5">
        <button
          type="button"
          class="icon-btn size-7"
          disabled={!hasPrev || loading}
          onclick={() => goPage(-1)}
          title={t("audit.prev")}
          aria-label={t("audit.prev")}
        >
          <ChevronLeft class="size-3.5" />
        </button>
        <button
          type="button"
          class="icon-btn size-7"
          disabled={!hasNext || loading}
          onclick={() => goPage(1)}
          title={t("audit.next")}
          aria-label={t("audit.next")}
        >
          <ChevronRight class="size-3.5" />
        </button>
      </div>
    </div>
  {/if}
</div>
