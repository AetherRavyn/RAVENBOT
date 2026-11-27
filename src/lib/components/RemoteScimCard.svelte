<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import SimpleSelect from "$lib/components/SimpleSelect.svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import {
    Copy,
    KeyRound,
    Loader2,
    RefreshCw,
    Save,
    ShieldCheck,
    Trash2,
  } from "@lucide/svelte";

  interface RemoteScimSummary {
    enabled: boolean;
    active: boolean;
    token_configured: boolean;
    default_role: string;
    last_used_at: string | null;
  }

  interface Props {
    scim?: RemoteScimSummary | null;
    onSaved: (status: any) => void;
  }

  let { scim = null, onSaved }: Props = $props();

  let formEnabled = $state(untrack(() => Boolean(scim?.enabled)));
  let active = $state(untrack(() => Boolean(scim?.active)));
  let tokenConfigured = $state(untrack(() => Boolean(scim?.token_configured)));
  let defaultRole = $state(
    untrack(() => (scim?.default_role === "operator" ? "operator" : "viewer")),
  );
  let lastUsedAt = $state<string | null>(untrack(() => scim?.last_used_at ?? null));
  // Plaintext lives only here: never in a prop, a store or the parent status.
  let token = $state<string | null>(null);
  let busy = $state(false);
  let tokenBusy = $state(false);
  let saved = $state(false);
  let copied = $state(false);
  let error = $state("");
  let savedTimer: ReturnType<typeof setTimeout> | undefined;
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;

  function applySummary(summary: RemoteScimSummary | null | undefined) {
    formEnabled = Boolean(summary?.enabled);
    active = Boolean(summary?.active);
    tokenConfigured = Boolean(summary?.token_configured);
    defaultRole = summary?.default_role === "operator" ? "operator" : "viewer";
    lastUsedAt = summary?.last_used_at ?? null;
  }

  function applyStatus(status: any) {
    applySummary(status?.scim as RemoteScimSummary | null | undefined);
  }

  function errorText(cause: unknown): string {
    return String(cause).replace(/^Error:\s*/, "");
  }

  // The parent owns the status. A refresh or a save updates the form without
  // discarding a plaintext token that was just generated.
  $effect(() => {
    applySummary(scim);
  });

  function lastUsedLabel(): string {
    if (!lastUsedAt) return t("settings.remoteScimNeverUsed");
    const date = new Date(lastUsedAt);
    const readable = Number.isNaN(date.getTime())
      ? lastUsedAt
      : date.toLocaleString();
    return t("settings.remoteScimLastUsed", { time: readable });
  }

  async function save() {
    if (busy || tokenBusy) return;
    busy = true;
    saved = false;
    error = "";
    if (savedTimer) clearTimeout(savedTimer);
    try {
      const status = await invoke<any>("configure_remote_scim", {
        enabled: formEnabled,
        defaultRole,
      });
      applyStatus(status);
      onSaved(status);
      saved = true;
      savedTimer = setTimeout(() => {
        saved = false;
      }, 2400);
    } catch (cause) {
      error = errorText(cause);
    } finally {
      busy = false;
    }
  }

  async function generateToken() {
    if (tokenBusy || busy) return;
    tokenBusy = true;
    copied = false;
    error = "";
    try {
      const response = await invoke<any>("create_remote_scim_token");
      token =
        typeof response?.token === "string" && response.token
          ? response.token
          : null;
      if (response?.status) applyStatus(response.status);
      else tokenConfigured = true;
      onSaved(response?.status);
    } catch (cause) {
      error = errorText(cause);
    } finally {
      tokenBusy = false;
    }
  }

  async function replaceToken() {
    if (tokenBusy || busy) return;
    if (!confirm(t("settings.remoteScimConfirmReplace"))) return;
    // The old token dies on the next successful generate; never keep its
    // plaintext around while the replacement is requested.
    token = null;
    copied = false;
    await generateToken();
  }

  async function revokeToken() {
    if (tokenBusy || busy) return;
    if (!confirm(t("settings.remoteScimConfirmRevoke"))) return;
    tokenBusy = true;
    token = null;
    copied = false;
    saved = false;
    error = "";
    try {
      const status = await invoke<any>("revoke_remote_scim_token");
      applyStatus(status);
      onSaved(status);
    } catch (cause) {
      error = errorText(cause);
    } finally {
      tokenBusy = false;
    }
  }

  async function copyToken() {
    if (!token) return;
    try {
      await navigator.clipboard.writeText(token);
      copied = true;
      if (copiedTimer) clearTimeout(copiedTimer);
      copiedTimer = setTimeout(() => {
        copied = false;
      }, 2400);
    } catch (cause) {
      error = errorText(cause);
    }
  }

  onDestroy(() => {
    if (savedTimer) clearTimeout(savedTimer);
    if (copiedTimer) clearTimeout(copiedTimer);
    token = null;
  });
</script>

<section
  class="p-5 rounded-2xl border border-[var(--hairline)] bg-card/60 space-y-4"
  aria-label={t("settings.remoteScimTitle")}
>
  <div class="flex items-start gap-3">
    <div
      class="flex size-9 shrink-0 items-center justify-center rounded-xl border border-[var(--brand)]/25 bg-[var(--brand-soft)]"
    >
      <KeyRound class="size-4 text-[var(--brand-text)]" />
    </div>
    <div class="min-w-0">
      <h4 class="text-sm font-bold text-[var(--text-primary)]">
        {t("settings.remoteScimTitle")}
      </h4>
      <p class="mt-1 text-[11px] leading-5 text-[var(--text-tertiary)]">
        {t("settings.remoteScimDescription")}
      </p>
    </div>
  </div>

  <label class="flex items-center gap-2 text-xs text-[var(--text-secondary)]">
    <input
      type="checkbox"
      bind:checked={formEnabled}
      class="size-4 accent-[var(--brand)]"
      disabled={busy || tokenBusy}
    />
    <span>{t("settings.remoteScimEnabled")}</span>
  </label>

  <div class="space-y-1.5 text-[11px] text-[var(--text-secondary)]">
    <span class="block font-semibold">{t("settings.remoteScimDefaultRole")}</span>
    <SimpleSelect
      value={defaultRole}
      options={[
        { value: "viewer", label: t("settings.remoteScimRoleViewer") },
        { value: "operator", label: t("settings.remoteScimRoleOperator") },
      ]}
      onValueChange={(value) => {
        defaultRole = value === "operator" ? "operator" : "viewer";
      }}
      ariaLabel={t("settings.remoteScimDefaultRole")}
      disabled={busy || tokenBusy}
    />
    <span class="block leading-4 text-[var(--text-muted)]">
      {t("settings.remoteScimRoleHint")}
    </span>
  </div>

  {#if formEnabled && (!active || !tokenConfigured)}
    <p
      class="rounded-lg border border-[var(--warning-border)] bg-[var(--warning-soft)] px-2.5 py-2 text-[11px] leading-5 text-[var(--warning-text)]"
      role="status"
    >
      {t("settings.remoteScimWarning")}
    </p>
  {/if}

  <div
    class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] p-3 space-y-3"
  >
    <div class="flex flex-wrap items-center justify-between gap-2">
      <p class="text-xs font-semibold text-[var(--text-secondary)]">
        {t("settings.remoteScimToken")}
      </p>
      <span class="text-[11px] text-[var(--text-muted)]">
        {lastUsedLabel()}
      </span>
    </div>

    <p class="text-[11px] leading-5 text-[var(--text-tertiary)]">
      {tokenConfigured
        ? t("settings.remoteScimTokenConfigured")
        : t("settings.remoteScimNoToken")}
    </p>

    {#if token}
      <div class="space-y-2">
        <div class="flex flex-wrap items-center gap-2">
          <Input
            readonly
            value={token}
            aria-label={t("settings.remoteScimToken")}
            class="h-8 min-w-0 flex-1 border-[var(--hairline)] bg-[var(--surface-1)] font-mono text-xs"
          />
          <Button size="sm" variant="outline" class="gap-1.5" onclick={copyToken}>
            <Copy class="size-3.5" />
            {copied ? t("settings.remoteScimCopied") : t("settings.remoteScimCopy")}
          </Button>
        </div>
        <p class="text-[11px] leading-5 text-[var(--warning-text)]">
          {t("settings.remoteScimTokenOnce")}
        </p>
      </div>
    {/if}

    <div class="flex flex-wrap items-center gap-2">
      {#if tokenConfigured}
        <Button
          size="sm"
          variant="outline"
          class="gap-1.5"
          onclick={replaceToken}
          disabled={tokenBusy || busy}
        >
          {#if tokenBusy}
            <Loader2 class="size-3.5 animate-spin" />
          {:else}
            <RefreshCw class="size-3.5" />
          {/if}
          {t("settings.remoteScimReplace")}
        </Button>
        <Button
          size="sm"
          variant="destructive"
          class="gap-1.5"
          onclick={revokeToken}
          disabled={tokenBusy || busy}
        >
          {#if tokenBusy}
            <Loader2 class="size-3.5 animate-spin" />
          {:else}
            <Trash2 class="size-3.5" />
          {/if}
          {t("settings.remoteScimRevoke")}
        </Button>
      {:else}
        <Button
          size="sm"
          class="gap-1.5"
          onclick={generateToken}
          disabled={tokenBusy || busy}
        >
          {#if tokenBusy}
            <Loader2 class="size-3.5 animate-spin" />
          {:else}
            <KeyRound class="size-3.5" />
          {/if}
          {t("settings.remoteScimGenerate")}
        </Button>
      {/if}
    </div>

    <p class="text-[11px] leading-5 text-[var(--text-muted)]">
      {t("settings.remoteScimTokenHelp")}
    </p>
  </div>

  <div
    class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] p-3 space-y-2"
  >
    <p class="flex items-start gap-2 text-[11px] leading-5 text-[var(--text-tertiary)]">
      <ShieldCheck class="mt-0.5 size-3.5 shrink-0 text-[var(--brand-text)]" />
      <span>{t("settings.remoteScimSecurity")}</span>
    </p>
    <ul class="list-disc space-y-1 pl-5 text-[11px] leading-5 text-[var(--text-muted)]">
      <li>{t("settings.remoteScimUsersOnly")}</li>
      <li>{t("settings.remoteScimNoGroups")}</li>
      <li>{t("settings.remoteScimOwnerProtected")}</li>
      <li>{t("settings.remoteScimSharedAccounts")}</li>
      <li>{t("settings.remoteScimSoftDelete")}</li>
      <li>{t("settings.remoteScimSsoHint")}</li>
      <li>{t("settings.remoteScimRestartHint")}</li>
      <li>{t("settings.remoteScimNoTest")}</li>
    </ul>
  </div>

  <div class="flex flex-wrap items-center gap-2">
    <Button
      size="sm"
      class="gap-1.5"
      onclick={save}
      disabled={busy || tokenBusy}
    >
      {#if busy}
        <Loader2 class="size-3.5 animate-spin" />
      {:else}
        <Save class="size-3.5" />
      {/if}
      {t("settings.remoteScimSave")}
    </Button>
    {#if saved}
      <span class="text-[11px] text-[var(--success-text)] font-bold" role="status">
        {t("settings.remoteScimSaved")}
      </span>
    {/if}
  </div>

  {#if error}
    <p class="text-[11px] text-[var(--danger-text)] break-words" role="alert">
      {error}
    </p>
  {/if}
</section>
