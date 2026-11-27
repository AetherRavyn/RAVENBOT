<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { KeyRound, Loader2, Save, ShieldCheck } from "@lucide/svelte";

  interface RemoteSsoSummary {
    issuer: string;
    client_id: string;
    redirect_url: string;
    scopes: string;
    username_claim: string;
  }

  interface Props {
    sso?: RemoteSsoSummary | null;
    enabled?: boolean;
    onSaved: (status: any) => void;
  }

  let { sso = null, enabled = false, onSaved }: Props = $props();

  const DEFAULT_SCOPES = "openid profile email";
  const DEFAULT_CLAIM = "preferred_username";

  // Initial values are a deliberate snapshot; `$effect` below keeps the form
  // in step with later prop changes from a refresh or save.
  let formEnabled = $state(untrack(() => enabled));
  let issuer = $state(untrack(() => sso?.issuer ?? ""));
  let clientId = $state(untrack(() => sso?.client_id ?? ""));
  let redirectUrl = $state(untrack(() => sso?.redirect_url ?? ""));
  let scopes = $state(untrack(() => sso?.scopes ?? DEFAULT_SCOPES));
  let usernameClaim = $state(untrack(() => sso?.username_claim ?? DEFAULT_CLAIM));
  let busy = $state(false);
  let saved = $state(false);
  let error = $state("");
  let savedTimer: ReturnType<typeof setTimeout> | undefined;

  // The parent owns the status. Apply prop changes from a refresh or another
  // save without overwriting what the person is typing on every keystroke.
  $effect(() => {
    formEnabled = enabled;
    if (sso) {
      issuer = sso.issuer;
      clientId = sso.client_id;
      redirectUrl = sso.redirect_url;
      scopes = sso.scopes || DEFAULT_SCOPES;
      usernameClaim = sso.username_claim || DEFAULT_CLAIM;
    }
  });

  function applyStatus(status: any) {
    formEnabled = Boolean(status?.sso_enabled);
    const summary = status?.sso as RemoteSsoSummary | null | undefined;
    if (summary) {
      issuer = summary.issuer;
      clientId = summary.client_id;
      redirectUrl = summary.redirect_url;
      scopes = summary.scopes || DEFAULT_SCOPES;
      usernameClaim = summary.username_claim || DEFAULT_CLAIM;
    }
  }

  async function save() {
    if (busy) return;
    busy = true;
    saved = false;
    error = "";
    if (savedTimer) clearTimeout(savedTimer);
    try {
      const status = await invoke<any>("configure_remote_sso", {
        enabled: formEnabled,
        issuer: issuer.trim(),
        clientId: clientId.trim(),
        redirectUrl: redirectUrl.trim(),
        scopes: scopes.trim(),
        usernameClaim: usernameClaim.trim(),
      });
      applyStatus(status);
      onSaved(status);
      saved = true;
      savedTimer = setTimeout(() => {
        saved = false;
      }, 2400);
    } catch (cause) {
      error = String(cause);
    } finally {
      busy = false;
    }
  }

  onDestroy(() => {
    if (savedTimer) clearTimeout(savedTimer);
  });
</script>

<section
  class="p-5 rounded-2xl border border-[var(--hairline)] bg-card/60 space-y-4"
  aria-label={t("settings.remoteSsoTitle")}
>
  <div class="flex items-start gap-3">
    <div class="flex size-9 shrink-0 items-center justify-center rounded-xl border border-[var(--brand)]/25 bg-[var(--brand-soft)]">
      <KeyRound class="size-4 text-[var(--brand-text)]" />
    </div>
    <div class="min-w-0">
      <h4 class="text-sm font-bold text-[var(--text-primary)]">
        {t("settings.remoteSsoTitle")}
      </h4>
      <p class="mt-1 text-[11px] leading-5 text-[var(--text-tertiary)]">
        {t("settings.remoteSsoDescription")}
      </p>
    </div>
  </div>

  <label class="flex items-center gap-2 text-xs text-[var(--text-secondary)]">
    <input
      type="checkbox"
      bind:checked={formEnabled}
      class="size-4 accent-[var(--brand)]"
      disabled={busy}
    />
    <span>{t("settings.remoteSsoEnabled")}</span>
  </label>

  <div class="grid grid-cols-1 gap-3 sm:grid-cols-2">
    <label class="space-y-1.5 text-[11px] text-[var(--text-secondary)]">
      <span class="block font-semibold">{t("settings.remoteSsoIssuer")}</span>
      <Input
        bind:value={issuer}
        aria-label={t("settings.remoteSsoIssuer")}
        placeholder="https://idp.example.com"
        class="h-8 border-[var(--hairline)] bg-[var(--surface-2)] font-mono text-xs"
        autocomplete="off"
        spellcheck="false"
        disabled={busy}
      />
      <span class="block leading-4 text-[var(--text-muted)]">
        {t("settings.remoteSsoIssuerHint")}
      </span>
    </label>

    <label class="space-y-1.5 text-[11px] text-[var(--text-secondary)]">
      <span class="block font-semibold">{t("settings.remoteSsoClientId")}</span>
      <Input
        bind:value={clientId}
        aria-label={t("settings.remoteSsoClientId")}
        placeholder="ravenbot-remote"
        class="h-8 border-[var(--hairline)] bg-[var(--surface-2)] font-mono text-xs"
        autocomplete="off"
        spellcheck="false"
        disabled={busy}
      />
      <span class="block leading-4 text-[var(--text-muted)]">
        {t("settings.remoteSsoClientIdHint")}
      </span>
    </label>

    <label class="space-y-1.5 text-[11px] text-[var(--text-secondary)] sm:col-span-2">
      <span class="block font-semibold">{t("settings.remoteSsoRedirectUrl")}</span>
      <Input
        bind:value={redirectUrl}
        aria-label={t("settings.remoteSsoRedirectUrl")}
        placeholder="https://bot.example.com/api/sso/callback"
        class="h-8 border-[var(--hairline)] bg-[var(--surface-2)] font-mono text-xs"
        autocomplete="off"
        spellcheck="false"
        disabled={busy}
      />
      <span class="block leading-4 text-[var(--text-muted)]">
        {t("settings.remoteSsoRedirectUrlHint")}
      </span>
    </label>

    <label class="space-y-1.5 text-[11px] text-[var(--text-secondary)]">
      <span class="block font-semibold">{t("settings.remoteSsoScopes")}</span>
      <Input
        bind:value={scopes}
        aria-label={t("settings.remoteSsoScopes")}
        placeholder={DEFAULT_SCOPES}
        class="h-8 border-[var(--hairline)] bg-[var(--surface-2)] font-mono text-xs"
        autocomplete="off"
        spellcheck="false"
        disabled={busy}
      />
    </label>

    <label class="space-y-1.5 text-[11px] text-[var(--text-secondary)]">
      <span class="block font-semibold">{t("settings.remoteSsoClaim")}</span>
      <Input
        bind:value={usernameClaim}
        aria-label={t("settings.remoteSsoClaim")}
        placeholder={DEFAULT_CLAIM}
        class="h-8 border-[var(--hairline)] bg-[var(--surface-2)] font-mono text-xs"
        autocomplete="off"
        spellcheck="false"
        disabled={busy}
      />
      <span class="block leading-4 text-[var(--text-muted)]">
        {t("settings.remoteSsoClaimHint")}
      </span>
    </label>
  </div>

  <div class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] p-3 space-y-2">
    <p class="flex items-start gap-2 text-[11px] leading-5 text-[var(--text-tertiary)]">
      <ShieldCheck class="mt-0.5 size-3.5 shrink-0 text-[var(--brand-text)]" />
      <span>{t("settings.remoteSsoSecurity")}</span>
    </p>
    <ul class="list-disc space-y-1 pl-5 text-[11px] leading-5 text-[var(--text-muted)]">
      <li>{t("settings.remoteSsoAccountsOnly")}</li>
      <li>{t("settings.remoteSsoNoScim")}</li>
      <li>{t("settings.remoteSsoRestartHint")}</li>
      <li>{t("settings.remoteSsoNoTest")}</li>
    </ul>
  </div>

  <div class="flex flex-wrap items-center gap-2">
    <Button
      size="sm"
      class="gap-1.5"
      onclick={save}
      disabled={busy}
    >
      {#if busy}
        <Loader2 class="size-3.5 animate-spin" />
      {:else}
        <Save class="size-3.5" />
      {/if}
      {t("settings.remoteSsoSave")}
    </Button>
    {#if saved}
      <span class="text-[11px] text-[var(--success-text)] font-bold" role="status">
        {t("settings.remoteSsoSaved")}
      </span>
    {/if}
  </div>

  {#if error}
    <p class="text-[11px] text-[var(--danger-text)] break-words" role="alert">
      {error}
    </p>
  {/if}
</section>
