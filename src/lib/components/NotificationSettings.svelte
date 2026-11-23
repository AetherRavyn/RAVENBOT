<script lang="ts">
  import { t } from "$lib/i18n";
  import {
    notificationPreference,
    sendTestNotification,
    setNotificationPreference,
  } from "$lib/notifications";
  import { Bell, Loader2, Send } from "@lucide/svelte";
  import AttentionWebhookSettings from "$lib/components/AttentionWebhookSettings.svelte";
  import AttentionEmailSettings from "$lib/components/AttentionEmailSettings.svelte";

  let enabled = $state(notificationPreference());
  let busy = $state(false);
  let notice = $state("");
  let error = $state("");

  function toggle(event: Event) {
    enabled = setNotificationPreference(
      (event.currentTarget as HTMLInputElement).checked,
    );
    notice = "";
    error = "";
  }

  async function sendTest() {
    if (busy) return;
    busy = true;
    notice = "";
    error = "";
    try {
      notice = await sendTestNotification();
    } catch (cause) {
      error = `${t("notifications.failed")} ${String(cause)}`;
    } finally {
      busy = false;
    }
  }
</script>

<section class="space-y-4" aria-label={t("notifications.title")}>
  <div>
    <h3 class="flex items-center gap-2 text-lg font-bold text-[var(--text-primary)]">
      <Bell class="size-5 text-[var(--brand-text)]" />
      {t("notifications.title")}
    </h3>
    <p class="mt-1 text-sm text-[var(--text-tertiary)]">
      {t("notifications.desc")}
    </p>
  </div>

  <div class="space-y-3 rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-4">
    <label class="flex items-center gap-2 text-xs font-semibold text-[var(--text-secondary)]">
      <input type="checkbox" checked={enabled} onchange={toggle} />
      {t("notifications.enable")}
    </label>
    <p class="text-[11px] leading-4 text-[var(--text-muted)]">
      {t("notifications.privacy")}
    </p>
    <button
      type="button"
      class="inline-flex h-8 items-center gap-1.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-3 text-[11px] font-semibold text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] cursor-pointer disabled:opacity-50"
      onclick={() => void sendTest()}
      disabled={busy || !enabled}
    >
      {#if busy}
        <Loader2 class="size-3.5 animate-spin" />
      {:else}
        <Send class="size-3.5" />
      {/if}
      {t("notifications.test")}
    </button>
    {#if notice}
      <p class="text-[11px] text-[var(--success-text)]" role="status">{notice}</p>
    {/if}
    {#if error}
      <p class="text-[11px] text-[var(--danger-text)] break-words" role="alert">{error}</p>
    {/if}
  </div>

  <div class="rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-4">
    <AttentionWebhookSettings />
  </div>

  <div class="rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-4">
    <AttentionEmailSettings />
  </div>
</section>
