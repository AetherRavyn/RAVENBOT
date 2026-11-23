<script lang="ts">
  /**
   * Save a password typed into the browser's secure card.
   *
   * The password is already in this card's memory; saving sends it directly to
   * the backend, which verifies the live page origin and writes through the
   * vault's keychain path. It is never added to the transcript or sent to a
   * model.
   */
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import { KeyRound, Loader2 } from "@lucide/svelte";

  interface Props {
    origin: string;
    initialName: string;
    initialUsername: string;
    secret: string;
    onCancel: () => void;
    onSaved: (updated: boolean) => void;
  }

  let {
    origin,
    initialName,
    initialUsername,
    secret,
    onCancel,
    onSaved,
  }: Props = $props();

  // Seed the editable fields once; later prop updates come from the parent
  // secret card, not from a changing name/username.
  // svelte-ignore state_referenced_locally
  let name = $state(initialName);
  // svelte-ignore state_referenced_locally
  let username = $state(initialUsername);
  let busy = $state(false);
  let error = $state("");

  async function save() {
    if (busy) return;
    if (!name.trim()) {
      error = t("browser.vaultNameRequired");
      return;
    }
    busy = true;
    error = "";
    try {
      const result = await invoke<{ updated: boolean }>(
        "save_browser_secret_to_vault",
        {
          name: name.trim(),
          username: username.trim(),
          website: origin,
          secret,
        },
      );
      onSaved(result.updated === true);
    } catch (cause) {
      error = String(cause);
    } finally {
      busy = false;
    }
  }
</script>

<div class="browser-secret__vault">
  <p class="browser-secret__vault-hint">
    <KeyRound class="size-3.5" aria-hidden="true" />
    {t("browser.vaultHint")}
  </p>
  <label class="browser-secret__vault-field">
    <span>{t("vault.name")}</span>
    <input
      class="browser-secret__input"
      bind:value={name}
      maxlength={120}
      aria-label={t("vault.name")}
      disabled={busy}
    />
  </label>
  <label class="browser-secret__vault-field">
    <span>{t("browser.vaultUsername")}</span>
    <input
      class="browser-secret__input"
      bind:value={username}
      maxlength={300}
      aria-label={t("browser.vaultUsername")}
      disabled={busy}
    />
  </label>
  <p class="browser-secret__vault-site">
    <span>{t("browser.vaultSite")}</span>
    <code>{origin}</code>
  </p>
  {#if error}
    <p class="browser-secret__error" role="alert">{error}</p>
  {/if}
  <div class="browser-secret__actions">
    <button
      type="button"
      class="browser-secret__cancel"
      onclick={onCancel}
      disabled={busy}
    >{t("ui.cancel")}</button>
    <button
      type="button"
      class="browser-secret__submit"
      onclick={() => void save()}
      disabled={busy || !name.trim()}
    >
      {#if busy}
        <Loader2 class="size-3.5 animate-spin" />
      {/if}
      {busy ? t("ui.saving") : t("browser.vaultSave")}
    </button>
  </div>
</div>
