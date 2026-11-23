<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import {
    Check,
    Loader2,
    Send,
    Trash2,
    Webhook,
  } from "@lucide/svelte";

  interface WebhookStatus {
    configured: boolean;
    host: string | null;
  }

  let configured = $state(false);
  let host = $state<string | null>(null);
  let url = $state("");
  let busy = $state(false);
  let notice = $state("");
  let error = $state("");

  async function load() {
    try {
      const status = await invoke<WebhookStatus>("get_attention_webhook");
      configured = status.configured === true;
      host = status.host ?? null;
    } catch (cause) {
      error = String(cause);
    }
  }

  async function save() {
    if (busy || !url.trim()) return;
    busy = true;
    notice = "";
    error = "";
    try {
      const status = await invoke<WebhookStatus>("set_attention_webhook", {
        url: url.trim(),
      });
      configured = status.configured === true;
      host = status.host ?? null;
      url = "";
      notice = t("notifications.webhookSaved");
    } catch (cause) {
      error = String(cause);
    } finally {
      busy = false;
    }
  }

  async function clear() {
    if (busy || !configured) return;
    busy = true;
    notice = "";
    error = "";
    try {
      await invoke("clear_attention_webhook");
      configured = false;
      host = null;
      url = "";
      notice = t("notifications.webhookCleared");
    } catch (cause) {
      error = String(cause);
    } finally {
      busy = false;
    }
  }

  async function test() {
    if (busy || !configured) return;
    busy = true;
    notice = "";
    error = "";
    try {
      await invoke("test_attention_webhook");
      notice = t("notifications.webhookTested");
    } catch (cause) {
      error = `${t("notifications.webhookFailed")} ${String(cause)}`;
    } finally {
      busy = false;
    }
  }

  onMount(() => void load());
</script>

<section class="space-y-3" aria-label={t("notifications.webhookTitle")}>
  <div>
    <p class="flex items-center gap-1.5 text-xs font-bold text-[var(--text-primary)]">
      <Webhook class="size-4 text-[var(--brand-text)]" />
      {t("notifications.webhookTitle")}
    </p>
    <p class="mt-1 text-[11px] leading-4 text-[var(--text-tertiary)]">
      {t("notifications.webhookDesc")}
    </p>
  </div>

  <p class="flex items-center gap-1.5 text-[11px] {configured
    ? 'text-[var(--success-text)]'
    : 'text-[var(--text-muted)]'}">
    {#if configured}
      <Check class="size-3.5" />
      {t("notifications.webhookConfigured", { host: host || "webhook" })}
    {:else}
      {t("notifications.webhookNotConfigured")}
    {/if}
  </p>

  <div class="flex flex-wrap items-end gap-2">
    <label class="min-w-48 flex-1 text-[11px] text-[var(--text-secondary)]">
      <span class="mb-1 block">{t("notifications.webhookUrl")}</span>
      <input
        class="chat-question__input font-mono"
        type="password"
        bind:value={url}
        placeholder={t("notifications.webhookPlaceholder")}
        aria-label={t("notifications.webhookUrl")}
        autocomplete="off"
        spellcheck="false"
        disabled={busy}
      />
    </label>
    <button
      type="button"
      class="inline-flex h-8 items-center gap-1.5 rounded-lg border border-[var(--brand)]/40 bg-[var(--brand-soft)] px-3 text-[11px] font-semibold text-[var(--brand-text)] cursor-pointer disabled:opacity-50"
      onclick={() => void save()}
      disabled={busy || !url.trim()}
    >
      {#if busy}
        <Loader2 class="size-3.5 animate-spin" />
      {/if}
      {t("notifications.webhookSave")}
    </button>
    <button
      type="button"
      class="inline-flex h-8 items-center gap-1.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-3 text-[11px] font-semibold text-[var(--text-secondary)] cursor-pointer disabled:opacity-50"
      onclick={() => void test()}
      disabled={busy || !configured}
    >
      <Send class="size-3.5" />
      {t("notifications.webhookTest")}
    </button>
    <button
      type="button"
      class="inline-flex h-8 items-center gap-1.5 rounded-lg border border-[var(--danger-border)] bg-[var(--danger-soft)] px-3 text-[11px] font-semibold text-[var(--danger-text)] cursor-pointer disabled:opacity-50"
      onclick={() => void clear()}
      disabled={busy || !configured}
    >
      <Trash2 class="size-3.5" />
      {t("notifications.webhookClear")}
    </button>
  </div>

  <p class="text-[11px] leading-4 text-[var(--text-muted)]">
    {t("notifications.webhookHttps")}
  </p>
  {#if notice}
    <p class="text-[11px] text-[var(--success-text)]" role="status">{notice}</p>
  {/if}
  {#if error}
    <p class="text-[11px] text-[var(--danger-text)] break-words" role="alert">{error}</p>
  {/if}
</section>
