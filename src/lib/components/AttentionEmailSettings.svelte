<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import SimpleSelect from "$lib/components/SimpleSelect.svelte";
  import { t } from "$lib/i18n";
  import {
    Check,
    Loader2,
    Mail,
    Send,
    Trash2,
  } from "@lucide/svelte";

  interface EmailStatus {
    configured: boolean;
    host: string;
    port: number;
    security: string;
    username: string;
    from: string;
    to: string;
    has_password: boolean;
  }

  let status = $state<EmailStatus | null>(null);
  let host = $state("");
  let port = $state(587);
  let security = $state("starttls");
  let username = $state("");
  let password = $state("");
  let from = $state("");
  let to = $state("");
  let busy = $state(false);
  let loaded = $state(false);
  let notice = $state("");
  let error = $state("");

  const securityOptions = $derived([
    { value: "starttls", label: t("notifications.emailStarttls") },
    { value: "none", label: t("notifications.emailNone") },
  ]);

  function apply(next: EmailStatus) {
    status = next;
    host = next.host;
    port = next.port;
    security = next.security;
    username = next.username;
    from = next.from;
    to = next.to;
    password = "";
  }

  async function load() {
    try {
      apply(await invoke<EmailStatus>("get_attention_email"));
      error = "";
    } catch (cause) {
      error = String(cause);
    } finally {
      loaded = true;
    }
  }

  async function save() {
    if (busy || !host.trim() || !from.trim() || !to.trim()) return;
    busy = true;
    notice = "";
    error = "";
    try {
      apply(
        await invoke<EmailStatus>("set_attention_email", {
          input: {
            host: host.trim(),
            port: Number(port),
            security,
            username: username.trim(),
            password: password || null,
            from: from.trim(),
            to: to.trim(),
          },
        }),
      );
      notice = t("notifications.emailSaved");
    } catch (cause) {
      error = String(cause);
    } finally {
      busy = false;
    }
  }

  async function clear() {
    if (busy) return;
    busy = true;
    notice = "";
    error = "";
    try {
      await invoke("clear_attention_email");
      status = null;
      host = "";
      port = 587;
      security = "starttls";
      username = "";
      password = "";
      from = "";
      to = "";
      notice = t("notifications.emailCleared");
    } catch (cause) {
      error = String(cause);
    } finally {
      busy = false;
    }
  }

  async function test() {
    if (busy || !status?.configured) return;
    busy = true;
    notice = "";
    error = "";
    try {
      await invoke("test_attention_email");
      notice = t("notifications.emailTested");
    } catch (cause) {
      error = `${t("notifications.emailFailed")} ${String(cause)}`;
    } finally {
      busy = false;
    }
  }

  onMount(() => void load());
</script>

<section class="space-y-3" aria-label={t("notifications.emailTitle")}>
  <div>
    <p class="flex items-center gap-1.5 text-xs font-bold text-[var(--text-primary)]">
      <Mail class="size-4 text-[var(--brand-text)]" />
      {t("notifications.emailTitle")}
    </p>
    <p class="mt-1 text-[11px] leading-4 text-[var(--text-tertiary)]">
      {t("notifications.emailDesc")}
    </p>
  </div>

  {#if !loaded}
    <p class="flex items-center gap-1.5 text-[11px] text-[var(--text-tertiary)]" role="status">
      <Loader2 class="size-3.5 animate-spin" />
      {t("ui.loading")}
    </p>
  {:else}
  <p class="flex items-center gap-1.5 text-[11px] {status?.configured
    ? 'text-[var(--success-text)]'
    : 'text-[var(--text-muted)]'}">
    {#if status?.configured}
      <Check class="size-3.5" />
      {t("notifications.emailConfigured", {
        host: status.host,
        port: status.port,
      })}
    {:else}
      {t("notifications.emailNotConfigured")}
    {/if}
  </p>

  <div class="grid gap-3 sm:grid-cols-2">
    <label class="space-y-1 text-[11px] text-[var(--text-secondary)]">
      <span class="block">{t("notifications.emailHost")}</span>
      <input
        class="chat-question__input font-mono"
        bind:value={host}
        placeholder="smtp.example.com"
        aria-label={t("notifications.emailHost")}
        autocomplete="off"
        spellcheck="false"
        disabled={busy}
      />
    </label>
    <label class="space-y-1 text-[11px] text-[var(--text-secondary)]">
      <span class="block">{t("notifications.emailPort")}</span>
      <input
        class="chat-question__input font-mono"
        type="number"
        min="1"
        max="65535"
        bind:value={port}
        aria-label={t("notifications.emailPort")}
        disabled={busy}
      />
    </label>
    <label class="space-y-1 text-[11px] text-[var(--text-secondary)]">
      <span class="block">{t("notifications.emailSecurity")}</span>
      <SimpleSelect
        value={security}
        options={securityOptions}
        onValueChange={(value) => (security = value)}
        class="h-8.5 rounded-xl"
      />
    </label>
    <label class="space-y-1 text-[11px] text-[var(--text-secondary)]">
      <span class="block">{t("notifications.emailUsername")}</span>
      <input
        class="chat-question__input font-mono"
        bind:value={username}
        placeholder={t("notifications.emailUsernameHint")}
        aria-label={t("notifications.emailUsername")}
        autocomplete="off"
        spellcheck="false"
        disabled={busy}
      />
    </label>
    <label class="space-y-1 text-[11px] text-[var(--text-secondary)]">
      <span class="block">{t("notifications.emailPassword")}</span>
      <input
        class="chat-question__input font-mono"
        type="password"
        bind:value={password}
        placeholder={status?.has_password
          ? t("notifications.emailPasswordHint")
          : t("notifications.emailPasswordPlaceholder")}
        aria-label={t("notifications.emailPassword")}
        autocomplete="off"
        spellcheck="false"
        disabled={busy}
      />
      {#if status?.has_password}
        <span class="flex items-center gap-1 text-[11px] text-[var(--success-text)]">
          <Check class="size-3" />
          {t("notifications.emailPasswordSaved")}
        </span>
      {/if}
    </label>
    <label class="space-y-1 text-[11px] text-[var(--text-secondary)]">
      <span class="block">{t("notifications.emailFrom")}</span>
      <input
        class="chat-question__input font-mono"
        type="email"
        bind:value={from}
        placeholder="alerts@example.com"
        aria-label={t("notifications.emailFrom")}
        autocomplete="off"
        spellcheck="false"
        disabled={busy}
      />
    </label>
    <label class="space-y-1 text-[11px] text-[var(--text-secondary)] sm:col-span-2">
      <span class="block">{t("notifications.emailTo")}</span>
      <input
        class="chat-question__input font-mono"
        type="email"
        bind:value={to}
        placeholder="ops@example.com"
        aria-label={t("notifications.emailTo")}
        autocomplete="off"
        spellcheck="false"
        disabled={busy}
      />
    </label>
  </div>

  <p class="text-[11px] leading-4 text-[var(--text-muted)]">
    {t("notifications.emailTls")}
  </p>

  <div class="flex flex-wrap items-center gap-2">
    <button
      type="button"
      class="inline-flex h-8 items-center gap-1.5 rounded-lg border border-[var(--brand)]/40 bg-[var(--brand-soft)] px-3 text-[11px] font-semibold text-[var(--brand-text)] cursor-pointer disabled:opacity-50"
      onclick={() => void save()}
      disabled={busy || !host.trim() || !from.trim() || !to.trim()}
    >
      {#if busy}
        <Loader2 class="size-3.5 animate-spin" />
      {:else}
        <Mail class="size-3.5" />
      {/if}
      {t("notifications.emailSave")}
    </button>
    <button
      type="button"
      class="inline-flex h-8 items-center gap-1.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-3 text-[11px] font-semibold text-[var(--text-secondary)] cursor-pointer disabled:opacity-50"
      onclick={() => void test()}
      disabled={busy || !status?.configured}
    >
      <Send class="size-3.5" />
      {t("notifications.emailTest")}
    </button>
    <button
      type="button"
      class="inline-flex h-8 items-center gap-1.5 rounded-lg border border-[var(--danger-border)] bg-[var(--danger-soft)] px-3 text-[11px] font-semibold text-[var(--danger-text)] cursor-pointer disabled:opacity-50"
      onclick={() => void clear()}
      disabled={busy || !status?.configured}
    >
      <Trash2 class="size-3.5" />
      {t("notifications.emailClear")}
    </button>
  </div>

  {#if notice}
    <p class="text-[11px] text-[var(--success-text)]" role="status">{notice}</p>
  {/if}
  {#if error}
    <p class="break-words text-[11px] text-[var(--danger-text)]" role="alert">{error}</p>
  {/if}
  {/if}
</section>
