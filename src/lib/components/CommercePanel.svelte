<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import { openExternal } from "$lib/external";
  import {
    Ban,
    Check,
    ExternalLink,
    Loader2,
    PackageCheck,
    Save,
    ShoppingBag,
    X,
  } from "@lucide/svelte";

  interface CommerceSettings {
    enabled: boolean;
    currency: string;
    per_request_limit_minor: string;
    daily_limit_minor: string;
    allowlist: string;
    valid: boolean;
    validation_error: string | null;
  }

  interface SpendRequest {
    id: string;
    bot_id: string;
    thread_id: string | null;
    run_id: string | null;
    title: string;
    merchant: string;
    merchant_url: string;
    amount_minor: number;
    currency: string;
    reason: string;
    status: string;
    decided_by: string | null;
    decided_at: string | null;
    decision_note: string | null;
    purchased_at: string | null;
    created_at: string;
    updated_at: string;
  }

  let settings = $state<CommerceSettings | null>(null);
  let enabled = $state(false);
  let currency = $state("USD");
  let perRequest = $state("");
  let daily = $state("");
  let allowlist = $state("");

  let requests = $state<SpendRequest[]>([]);
  let loading = $state(true);
  let saving = $state(false);
  let busyId = $state<string | null>(null);
  let error = $state("");
  let notice = $state("");

  function errorText(cause: unknown): string {
    return String(cause).replace(/^Error:\s*/, "");
  }

  function applySettings(next: CommerceSettings | null | undefined) {
    settings = next ?? null;
    enabled = next?.enabled ?? false;
    currency = next?.currency ?? "USD";
    perRequest = next?.per_request_limit_minor ?? "";
    daily = next?.daily_limit_minor ?? "";
    allowlist = next?.allowlist ?? "";
  }

  function replaceRequest(next: SpendRequest) {
    requests = requests.map((row) => (row.id === next.id ? next : row));
  }

  function statusLabel(status: string): string {
    switch (status) {
      case "approved":
        return t("commerce.statusApproved");
      case "denied":
        return t("commerce.statusDenied");
      case "purchased":
        return t("commerce.statusPurchased");
      case "cancelled":
        return t("commerce.statusCancelled");
      default:
        return t("commerce.statusPending");
    }
  }

  function statusClass(status: string): string {
    switch (status) {
      case "approved":
        return "text-info bg-info/10";
      case "purchased":
        return "text-success bg-success/10";
      case "denied":
        return "text-danger bg-danger/10";
      case "cancelled":
        return "text-[var(--text-muted)] bg-[var(--surface-2)]";
      default:
        return "text-warning bg-warning/10";
    }
  }

  function formatTime(value: string | null): string {
    if (!value) return "";
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
  }

  function amountText(row: SpendRequest): string {
    return `${row.currency} ${row.amount_minor.toLocaleString()} ${t("commerce.minorUnits")}`;
  }

  async function load() {
    loading = true;
    error = "";
    try {
      const [nextSettings, nextRequests] = await Promise.all([
        invoke<CommerceSettings>("get_commerce_settings"),
        invoke<SpendRequest[]>("list_spend_requests"),
      ]);
      applySettings(nextSettings);
      requests = Array.isArray(nextRequests) ? nextRequests : [];
    } catch (cause) {
      error = `${t("commerce.loadError")} ${errorText(cause)}`;
    } finally {
      loading = false;
    }
  }

  async function savePolicy() {
    if (saving || loading) return;
    saving = true;
    error = "";
    notice = "";
    try {
      const next = await invoke<CommerceSettings>("set_commerce_settings", {
        input: {
          enabled,
          currency: currency.trim(),
          per_request_limit_minor: perRequest.trim(),
          daily_limit_minor: daily.trim(),
          allowlist: allowlist.trim(),
        },
      });
      applySettings(next);
      notice = t("commerce.saved");
    } catch (cause) {
      error = `${t("commerce.policyError")} ${errorText(cause)}`;
    } finally {
      saving = false;
    }
  }

  async function approve(row: SpendRequest) {
    if (busyId || row.status !== "pending") return;
    if (
      !confirm(
        t("commerce.approveConfirm", {
          amount: amountText(row),
          merchant: row.merchant,
        }),
      )
    ) {
      return;
    }
    busyId = row.id;
    error = "";
    notice = "";
    try {
      replaceRequest(await invoke<SpendRequest>("approve_spend_request", { id: row.id }));
    } catch (cause) {
      error = `${t("commerce.decideError")} ${errorText(cause)}`;
    } finally {
      busyId = null;
    }
  }

  async function deny(row: SpendRequest) {
    if (busyId || row.status !== "pending") return;
    busyId = row.id;
    error = "";
    notice = "";
    try {
      replaceRequest(await invoke<SpendRequest>("deny_spend_request", { id: row.id }));
    } catch (cause) {
      error = `${t("commerce.decideError")} ${errorText(cause)}`;
    } finally {
      busyId = null;
    }
  }

  async function markPurchased(row: SpendRequest) {
    if (busyId || row.status !== "approved" || !confirm(t("commerce.purchaseConfirm"))) {
      return;
    }
    busyId = row.id;
    error = "";
    notice = "";
    try {
      replaceRequest(await invoke<SpendRequest>("mark_spend_purchased", { id: row.id }));
    } catch (cause) {
      error = `${t("commerce.decideError")} ${errorText(cause)}`;
    } finally {
      busyId = null;
    }
  }

  async function cancel(row: SpendRequest) {
    if (busyId || !["pending", "approved"].includes(row.status)) return;
    busyId = row.id;
    error = "";
    notice = "";
    try {
      replaceRequest(await invoke<SpendRequest>("cancel_spend_request", { id: row.id }));
    } catch (cause) {
      error = `${t("commerce.decideError")} ${errorText(cause)}`;
    } finally {
      busyId = null;
    }
  }

  async function openCheckout(row: SpendRequest) {
    if (row.status !== "approved") return;
    await openExternal(row.merchant_url);
  }

  onMount(load);
</script>

<div class="space-y-5">
  <section class="rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-5">
    <div class="flex items-start gap-3">
      <div class="flex size-10 shrink-0 items-center justify-center rounded-xl border border-[var(--brand)]/25 bg-[var(--brand-soft)]">
        <ShoppingBag class="size-5 text-[var(--brand-text)]" />
      </div>
      <div class="min-w-0">
        <h2 class="text-base font-bold text-[var(--text-primary)]">{t("commerce.title")}</h2>
        <p class="mt-1 text-[12px] leading-5 text-[var(--text-tertiary)]">
          {t("commerce.description")}
        </p>
      </div>
    </div>

    {#if error}
      <div class="mt-4 rounded-xl border border-[var(--danger-border)] bg-[var(--danger-soft)] px-3 py-2 text-[12px] text-[var(--danger-text)]" role="alert">
        {error}
      </div>
    {/if}
    {#if notice}
      <div class="mt-4 rounded-xl border border-[var(--success-border)] bg-[var(--success-soft)] px-3 py-2 text-[12px] text-[var(--success-text)]" role="status">
        {notice}
      </div>
    {/if}
    {#if settings && !settings.valid}
      <div class="mt-4 rounded-xl border border-[var(--warning-border)] bg-[var(--warning-soft)] px-3 py-2 text-[12px] leading-5 text-[var(--warning-text)]" role="status">
        <span class="font-semibold">{t("commerce.invalidPolicy")}</span>
        <span class="break-words"> {settings.validation_error}</span>
      </div>
    {/if}

    {#if loading}
      <div class="mt-5 flex items-center gap-2 text-[12px] text-[var(--text-tertiary)]">
        <Loader2 class="size-4 animate-spin" /> {t("commerce.loading")}
      </div>
    {:else}
      <div class="mt-5 space-y-4">
        <label class="flex items-start gap-3 rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] px-3 py-3">
          <input type="checkbox" bind:checked={enabled} class="mt-0.5 accent-[var(--brand)]" />
          <span class="min-w-0">
            <span class="block text-[12px] font-semibold text-[var(--text-primary)]">
              {t("commerce.enabled")}
            </span>
            <span class="mt-0.5 block text-[11px] leading-5 text-[var(--text-tertiary)]">
              {t("commerce.enabledHint")}
            </span>
          </span>
        </label>

        <div class="grid gap-4 sm:grid-cols-3">
          <div class="space-y-1.5">
            <label for="commerce-currency" class="text-[11px] font-semibold text-[var(--text-secondary)]">
              {t("commerce.currency")}
            </label>
            <input
              id="commerce-currency"
              bind:value={currency}
              maxlength="3"
              autocomplete="off"
              spellcheck="false"
              class="h-9 w-full rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-3 font-mono text-[12px] text-[var(--text-primary)] outline-none focus:border-[var(--brand)]"
            />
            <p class="text-[11px] leading-4 text-[var(--text-muted)]">{t("commerce.currencyHint")}</p>
          </div>
          <div class="space-y-1.5">
            <label for="commerce-per-request" class="text-[11px] font-semibold text-[var(--text-secondary)]">
              {t("commerce.perRequest")}
            </label>
            <input
              id="commerce-per-request"
              bind:value={perRequest}
              inputmode="numeric"
              autocomplete="off"
              class="h-9 w-full rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-3 font-mono text-[12px] text-[var(--text-primary)] outline-none focus:border-[var(--brand)]"
            />
            <p class="text-[11px] leading-4 text-[var(--text-muted)]">{t("commerce.perRequestHint")}</p>
          </div>
          <div class="space-y-1.5">
            <label for="commerce-daily" class="text-[11px] font-semibold text-[var(--text-secondary)]">
              {t("commerce.daily")}
            </label>
            <input
              id="commerce-daily"
              bind:value={daily}
              inputmode="numeric"
              autocomplete="off"
              class="h-9 w-full rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-3 font-mono text-[12px] text-[var(--text-primary)] outline-none focus:border-[var(--brand)]"
            />
            <p class="text-[11px] leading-4 text-[var(--text-muted)]">{t("commerce.dailyHint")}</p>
          </div>
        </div>

        <div class="space-y-1.5">
          <label for="commerce-allowlist" class="text-[11px] font-semibold text-[var(--text-secondary)]">
            {t("commerce.allowlist")}
          </label>
          <textarea
            id="commerce-allowlist"
            bind:value={allowlist}
            rows="3"
            spellcheck="false"
            class="w-full rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-3 py-2 font-mono text-[12px] text-[var(--text-primary)] outline-none focus:border-[var(--brand)]"
          ></textarea>
          <p class="text-[11px] leading-4 text-[var(--text-muted)]">{t("commerce.allowlistHint")}</p>
        </div>

        <button
          type="button"
          class="inline-flex h-9 cursor-pointer items-center gap-2 rounded-lg border border-[var(--brand)]/40 bg-[var(--brand-soft)] px-3 text-[12px] font-semibold text-[var(--brand-text)] hover:bg-[var(--brand)]/15 disabled:cursor-not-allowed disabled:opacity-50"
          onclick={() => void savePolicy()}
          disabled={saving}
        >
          {#if saving}
            <Loader2 class="size-3.5 animate-spin" />
          {:else}
            <Save class="size-3.5" />
          {/if}
          {saving ? t("commerce.saving") : t("commerce.save")}
        </button>
      </div>
    {/if}
  </section>

  <section class="rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-5">
    <div class="flex items-center justify-between gap-3">
      <h3 class="text-sm font-bold text-[var(--text-primary)]">{t("commerce.requests")}</h3>
      <span class="text-[11px] text-[var(--text-muted)]">{requests.length}</span>
    </div>
    <p class="mt-1 text-[11px] leading-5 text-[var(--text-tertiary)]">{t("commerce.reviewHint")}</p>

    {#if !loading && requests.length === 0}
      <p class="mt-4 rounded-xl border border-dashed border-[var(--hairline)] px-3 py-4 text-center text-[12px] text-[var(--text-muted)]">
        {t("commerce.noRequests")}
      </p>
    {/if}

    <div class="mt-4 space-y-3">
      {#each requests as row (row.id)}
        <article class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] p-4">
          <div class="flex flex-wrap items-start justify-between gap-2">
            <div class="min-w-0">
              <h4 class="text-[13px] font-semibold text-[var(--text-primary)]">{row.title}</h4>
              <p class="mt-0.5 text-[11px] text-[var(--text-tertiary)]">
                {row.merchant} · {amountText(row)}
              </p>
            </div>
            <span class="rounded-full px-2 py-0.5 text-[11px] font-semibold {statusClass(row.status)}">
              {statusLabel(row.status)}
            </span>
          </div>

          <p class="mt-2 whitespace-pre-wrap break-words text-[12px] leading-5 text-[var(--text-secondary)]">{row.reason}</p>
          <p class="mt-2 break-all font-mono text-[11px] text-[var(--text-muted)]">{row.merchant_url}</p>
          {#if row.decision_note}
            <p class="mt-2 rounded-lg border border-[var(--hairline)] bg-[var(--surface-1)] px-2.5 py-2 text-[11px] leading-5 text-[var(--text-tertiary)]">
              {row.decision_note}
            </p>
          {/if}
          <p class="mt-2 text-[11px] text-[var(--text-muted)]">
            {formatTime(row.created_at)}
            {#if row.decided_by} · {row.decided_by}{/if}
            {#if row.purchased_at} · {formatTime(row.purchased_at)}{/if}
          </p>

          {#if ["pending", "approved"].includes(row.status)}
            <div class="mt-3 flex flex-wrap items-center gap-2">
              {#if row.status === "pending"}
                <button
                  type="button"
                  class="inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-lg border border-[var(--success-border)] bg-[var(--success-soft)] px-2.5 text-[11px] font-semibold text-[var(--success-text)] disabled:opacity-50"
                  onclick={() => void approve(row)}
                  aria-label={`${t("commerce.approve")} — ${row.title}`}
                  disabled={busyId === row.id}
                >
                  <Check class="size-3.5" /> {t("commerce.approve")}
                </button>
                <button
                  type="button"
                  class="inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-lg border border-[var(--danger-border)] bg-[var(--danger-soft)] px-2.5 text-[11px] font-semibold text-[var(--danger-text)] disabled:opacity-50"
                  onclick={() => void deny(row)}
                  aria-label={`${t("commerce.deny")} — ${row.title}`}
                  disabled={busyId === row.id}
                >
                  <Ban class="size-3.5" /> {t("commerce.deny")}
                </button>
              {:else}
                <button
                  type="button"
                  class="inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-lg border border-[var(--brand)]/40 bg-[var(--brand-soft)] px-2.5 text-[11px] font-semibold text-[var(--brand-text)]"
                  onclick={() => void openCheckout(row)}
                  aria-label={`${t("commerce.openCheckout")} — ${row.title}`}
                >
                  <ExternalLink class="size-3.5" /> {t("commerce.openCheckout")}
                </button>
                <button
                  type="button"
                  class="inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-lg border border-[var(--success-border)] bg-[var(--success-soft)] px-2.5 text-[11px] font-semibold text-[var(--success-text)] disabled:opacity-50"
                  onclick={() => void markPurchased(row)}
                  aria-label={`${t("commerce.markPurchased")} — ${row.title}`}
                  disabled={busyId === row.id}
                >
                  <PackageCheck class="size-3.5" /> {t("commerce.markPurchased")}
                </button>
              {/if}
              <button
                type="button"
                class="inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-lg border border-[var(--hairline-strong)] px-2.5 text-[11px] font-semibold text-[var(--text-tertiary)] hover:text-[var(--text-primary)] disabled:opacity-50"
                onclick={() => void cancel(row)}
                aria-label={`${t("commerce.cancel")} — ${row.title}`}
                disabled={busyId === row.id}
              >
                <X class="size-3.5" /> {t("commerce.cancel")}
              </button>
              {#if busyId === row.id}
                <Loader2 class="size-3.5 animate-spin text-[var(--text-tertiary)]" />
              {/if}
            </div>
          {/if}
        </article>
      {/each}
    </div>
  </section>
</div>
