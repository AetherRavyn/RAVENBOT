<script lang="ts">
  /**
   * Opt-in enterprise export over OTLP/HTTP logs.
   *
   * The card only ever receives safe metadata. Headers are write-only: the
   * backend stores them in the OS keychain and returns `headers_configured`,
   * never the values, so an operator cannot read a saved bearer token back.
   */
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import {
    CloudUpload,
    Loader2,
    RefreshCw,
    Send,
    ShieldCheck,
    Trash2,
  } from "@lucide/svelte";

  interface OtelStatus {
    enabled: boolean;
    endpoint: string;
    service_name: string;
    interval_secs: number;
    batch_size: number;
    headers_configured: boolean;
    audit_cursor: number;
    run_cursor: { updated_at: string; id: string } | null;
    last_attempt_at: string | null;
    last_success_at: string | null;
    last_error: string | null;
    last_audit_count: number;
    last_run_count: number;
  }

  interface OtelReport {
    attempted: boolean;
    audit_count: number;
    run_count: number;
    delivered: boolean;
    endpoint: string;
    status_code: number | null;
    error: string | null;
    next_retry_in_secs: number | null;
  }

  let enabled = $state(false);
  let endpoint = $state("");
  let serviceName = $state("ravenbot");
  let intervalSecs = $state("60");
  let batchSize = $state("200");
  let headersDraft = $state("");
  let headersConfigured = $state(false);
  let headersClearPending = $state(false);
  let lastAttemptAt = $state<string | null>(null);
  let lastSuccessAt = $state<string | null>(null);
  let lastError = $state<string | null>(null);
  let lastAuditCount = $state(0);
  let lastRunCount = $state(0);
  let loading = $state(true);
  let busy = $state(false);
  let exporting = $state(false);
  let testing = $state(false);
  let notice = $state("");
  let error = $state("");

  function applyStatus(status: OtelStatus) {
    enabled = status.enabled;
    endpoint = status.endpoint || "";
    serviceName = status.service_name || "ravenbot";
    intervalSecs = String(status.interval_secs || 60);
    batchSize = String(status.batch_size || 200);
    headersConfigured = status.headers_configured;
    lastAttemptAt = status.last_attempt_at || null;
    lastSuccessAt = status.last_success_at || null;
    lastError = status.last_error || null;
    lastAuditCount = status.last_audit_count || 0;
    lastRunCount = status.last_run_count || 0;
  }

  async function load() {
    loading = true;
    error = "";
    try {
      const status = await invoke<OtelStatus>("get_otel_export_status");
      applyStatus(status);
    } catch (cause) {
      error = String(cause);
    } finally {
      loading = false;
    }
  }

  async function save() {
    if (busy) return;
    busy = true;
    notice = "";
    error = "";
    try {
      const headers = headersClearPending
        ? ""
        : headersDraft.trim()
          ? headersDraft
          : null;
      const status = await invoke<OtelStatus>("set_otel_export_config", {
        enabled,
        endpoint: endpoint.trim(),
        serviceName: serviceName.trim(),
        intervalSecs: Number(intervalSecs),
        batchSize: Number(batchSize),
        headers,
      });
      applyStatus(status);
      headersDraft = "";
      headersClearPending = false;
      notice = t("otel.saved");
    } catch (cause) {
      error = String(cause);
    } finally {
      busy = false;
    }
  }

  function clearHeaders() {
    headersDraft = "";
    headersClearPending = true;
    notice = "";
    error = "";
  }

  async function exportNow() {
    if (exporting) return;
    exporting = true;
    notice = "";
    error = "";
    try {
      const report = await invoke<OtelReport>("export_otel_now");
      await load();
      const count = report.audit_count + report.run_count;
      if (report.error) {
        error = t("otel.exportFailed", { error: report.error });
      } else if (count === 0) {
        notice = t("otel.exportEmpty");
      } else {
        notice = t("otel.exported", { count });
      }
    } catch (cause) {
      error = String(cause);
    } finally {
      exporting = false;
    }
  }

  async function testConnection() {
    if (testing) return;
    testing = true;
    notice = "";
    error = "";
    try {
      const report = await invoke<OtelReport>("test_otel_export");
      if (report.error) {
        error = t("otel.testFailed", { error: report.error });
      } else {
        notice = t("otel.testOk", { endpoint: report.endpoint });
      }
    } catch (cause) {
      error = String(cause);
    } finally {
      testing = false;
    }
  }

  function fmtTime(value: string | null): string {
    if (!value) return t("otel.never");
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

<section
  class="space-y-2"
  aria-label={t("otel.title")}
  aria-busy={busy || exporting || testing}
>
  <div class="flex items-start justify-between gap-2">
    <div>
      <p class="text-[11px] font-semibold text-[var(--text-primary)]">
        <CloudUpload class="mr-1 inline size-3.5" aria-hidden="true" />
        {t("otel.title")}
      </p>
      <p class="mt-0.5 text-[11px] leading-4 text-[var(--text-tertiary)]">
        {t("otel.description")}
      </p>
    </div>
    <button
      type="button"
      class="icon-btn size-6 shrink-0"
      onclick={() => void load()}
      disabled={loading || busy || exporting || testing}
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

  <div class="grid gap-1.5 sm:grid-cols-2">
    <label class="text-[11px] text-[var(--text-secondary)] sm:col-span-2" for="otel-endpoint">
      <span class="mb-0.5 block">{t("otel.endpoint")}</span>
      <input
        id="otel-endpoint"
        class="h-7 w-full rounded-lg border border-[var(--hairline)] bg-[var(--surface-1)] px-2 font-mono text-[11px] text-[var(--text-primary)] placeholder:text-[var(--text-muted)]"
        placeholder={t("otel.endpointPlaceholder")}
        aria-describedby="otel-endpoint-hint"
        value={endpoint}
        oninput={(event) => (endpoint = event.currentTarget.value)}
        disabled={busy || loading}
      />
    </label>
    <p id="otel-endpoint-hint" class="text-[11px] leading-4 text-[var(--text-muted)] sm:col-span-2">
      {t("otel.endpointHint")}
    </p>

    <label class="text-[11px] text-[var(--text-secondary)]" for="otel-service">
      <span class="mb-0.5 block">{t("otel.serviceName")}</span>
      <input
        id="otel-service"
        class="h-7 w-full rounded-lg border border-[var(--hairline)] bg-[var(--surface-1)] px-2 text-[11px] text-[var(--text-primary)]"
        value={serviceName}
        oninput={(event) => (serviceName = event.currentTarget.value)}
        disabled={busy || loading}
      />
    </label>

    <div class="grid grid-cols-2 gap-1.5">
      <label class="text-[11px] text-[var(--text-secondary)]" for="otel-interval">
        <span class="mb-0.5 block">{t("otel.interval")}</span>
        <input
          id="otel-interval"
          type="number"
          min="10"
          max="3600"
          step="1"
          class="h-7 w-full rounded-lg border border-[var(--hairline)] bg-[var(--surface-1)] px-2 text-[11px] tabular-nums text-[var(--text-primary)]"
          value={intervalSecs}
          oninput={(event) => (intervalSecs = event.currentTarget.value)}
          disabled={busy || loading}
        />
      </label>
      <label class="text-[11px] text-[var(--text-secondary)]" for="otel-batch">
        <span class="mb-0.5 block">{t("otel.batch")}</span>
        <input
          id="otel-batch"
          type="number"
          min="1"
          max="500"
          step="1"
          class="h-7 w-full rounded-lg border border-[var(--hairline)] bg-[var(--surface-1)] px-2 text-[11px] tabular-nums text-[var(--text-primary)]"
          value={batchSize}
          oninput={(event) => (batchSize = event.currentTarget.value)}
          disabled={busy || loading}
        />
      </label>
    </div>

    <label
      class="flex items-center gap-1.5 text-[11px] text-[var(--text-secondary)] sm:col-span-2"
      for="otel-enabled"
    >
      <input
        id="otel-enabled"
        type="checkbox"
        class="size-3.5 rounded border-[var(--hairline)]"
        checked={enabled}
        onchange={(event) => (enabled = event.currentTarget.checked)}
        disabled={busy || loading}
      />
      {t("otel.enabled")}
    </label>
  </div>

  <div class="space-y-1">
    <div class="flex flex-wrap items-center gap-1.5">
      <label class="text-[11px] text-[var(--text-secondary)]" for="otel-headers">
        {t("otel.headers")}
      </label>
      {#if headersConfigured && !headersClearPending}
        <span
          class="inline-flex h-5 items-center gap-1 rounded-full border border-[var(--success-text)]/25 bg-[var(--success-soft)] px-1.5 text-[11px] font-semibold text-[var(--success-text)]"
        >
          <ShieldCheck class="size-3" aria-hidden="true" />
          {t("otel.headersConfigured")}
        </span>
      {/if}
      {#if headersConfigured || headersDraft.trim()}
        <button
          type="button"
          class="inline-flex h-5 items-center gap-1 rounded-full border border-[var(--hairline)] bg-[var(--surface-1)] px-1.5 text-[11px] font-semibold text-[var(--text-secondary)] hover:bg-[var(--surface-2)] cursor-pointer disabled:opacity-50"
          onclick={clearHeaders}
          disabled={busy || loading}
        >
          <Trash2 class="size-3" aria-hidden="true" />
          {t("otel.clearHeaders")}
        </button>
      {/if}
    </div>
    <textarea
      id="otel-headers"
      class="min-h-16 w-full rounded-lg border border-[var(--hairline)] bg-[var(--surface-1)] px-2 py-1.5 font-mono text-[11px] text-[var(--text-primary)] placeholder:text-[var(--text-muted)]"
      rows="3"
      spellcheck="false"
      autocomplete="off"
      placeholder={t("otel.headersPlaceholder")}
      value={headersDraft}
      oninput={(event) => {
        headersDraft = event.currentTarget.value;
        if (headersDraft.trim()) headersClearPending = false;
      }}
      disabled={busy || loading}
    ></textarea>
    <p class="text-[11px] leading-4 text-[var(--text-muted)]">
      {t("otel.headersHint")}
    </p>
    {#if headersClearPending}
      <p class="text-[11px] leading-4 text-[var(--warning-text)]" role="status">
        {t("otel.headersClearPending")}
      </p>
    {/if}
  </div>

  <div class="flex flex-wrap items-center gap-1.5">
    <button
      type="button"
      class="inline-flex h-7 items-center gap-1.5 rounded-lg border border-[var(--brand)]/30 bg-[var(--brand-soft)] px-2.5 text-[11px] font-semibold text-[var(--brand-text)] hover:bg-[var(--surface-2)] cursor-pointer disabled:opacity-50"
      onclick={() => void save()}
      disabled={busy || loading}
    >
      {#if busy}
        <Loader2 class="size-3.5 animate-spin" />
        {t("otel.saving")}
      {:else}
        {t("otel.save")}
      {/if}
    </button>
    <button
      type="button"
      class="inline-flex h-7 items-center gap-1.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-1)] px-2.5 text-[11px] font-semibold text-[var(--text-secondary)] hover:bg-[var(--surface-2)] hover:text-[var(--text-primary)] cursor-pointer disabled:opacity-50"
      onclick={() => void exportNow()}
      disabled={exporting || busy || loading}
    >
      {#if exporting}
        <Loader2 class="size-3.5 animate-spin" />
        {t("otel.exporting")}
      {:else}
        <Send class="size-3.5" />
        {t("otel.exportNow")}
      {/if}
    </button>
    <button
      type="button"
      class="inline-flex h-7 items-center gap-1.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-1)] px-2.5 text-[11px] font-semibold text-[var(--text-secondary)] hover:bg-[var(--surface-2)] hover:text-[var(--text-primary)] cursor-pointer disabled:opacity-50"
      onclick={() => void testConnection()}
      disabled={testing || busy || loading}
    >
      {#if testing}
        <Loader2 class="size-3.5 animate-spin" />
        {t("otel.testing")}
      {:else}
        {t("otel.test")}
      {/if}
    </button>
  </div>

  <div class="space-y-0.5 text-[11px] leading-4 text-[var(--text-muted)]">
    <p>{t("otel.lastAttempt", { time: fmtTime(lastAttemptAt) })}</p>
    <p>{t("otel.lastSuccess", { time: fmtTime(lastSuccessAt) })}</p>
    <p>
      {t("otel.counts", {
        audit: lastAuditCount,
        runs: lastRunCount,
      })}
    </p>
    <p>{t("otel.localOnly")}</p>
  </div>

  {#if lastError && !error}
    <p class="flex items-start gap-1.5 text-[11px] text-[var(--danger-text)]" role="alert">
      <span class="break-words">{t("otel.exportFailed", { error: lastError })}</span>
    </p>
  {/if}
  {#if notice}
    <p class="text-[11px] text-[var(--success-text)] break-words" role="status">{notice}</p>
  {/if}
  {#if error}
    <p class="text-[11px] text-[var(--danger-text)] break-words" role="alert">{error}</p>
  {/if}
</section>
