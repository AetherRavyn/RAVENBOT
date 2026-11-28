<script lang="ts">
  /**
   * An agent asks the local vault for a login, or asks the owner to enter an
   * API key. The login card lists metadata only and fills the current page; the
   * key card is a password field whose value goes to the host command, is
   * stored in the Vault and is written to an owner-only file. The model
   * receives only the item name, origin, name and path — never a secret.
   */
  import { onMount, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import { sidePanel } from "$lib/workspace/sidePanel.svelte";
  import BlockHeader from "./BlockHeader.svelte";
  import {
    serializeBlockResponse,
    type InteractionBlock,
  } from "$lib/chat/blocks";
  import type { VaultItem } from "$lib/vault";
  import { Eye, EyeOff, KeyRound, Loader2, ShieldCheck } from "@lucide/svelte";

  interface Props {
    botName?: string;
    header: string;
    question: string;
    payload: InteractionBlock;
    questionId?: string;
    answering?: boolean;
    onAnswer: (answer: string) => void;
    onSubmitted?: () => void;
  }

  let {
    botName = "",
    header,
    question,
    payload,
    questionId = "",
    answering = false,
    onAnswer,
    onSubmitted,
  }: Props = $props();

  let items = $state<VaultItem[]>([]);
  let loading = $state(true);
  let filling = $state<string | null>(null);
  let error = $state("");
  let keyValue = $state("");
  let showKey = $state(false);
  let savingKey = $state(false);
  let keyInput: HTMLInputElement | null = $state(null);

  const isApiKey = $derived(payload.item_kind === "api_key");

  function hostOf(value: string | null | undefined): string {
    try {
      return new URL(value || "").hostname.replace(/^www\./u, "").toLowerCase();
    } catch {
      return "";
    }
  }

  function hostMatches(itemHost: string, wanted: string): boolean {
    if (!wanted) return false;
    return itemHost === wanted || itemHost.endsWith(`.${wanted}`) || wanted.endsWith(`.${itemHost}`);
  }

  const wantedHost = $derived(hostOf(payload.site));
  const matching = $derived(
    items.filter(
      (item) =>
        item.kind === "login" &&
        (!wantedHost || hostMatches(hostOf(item.website), wantedHost)),
    ),
  );
  const otherLogins = $derived(
    items.filter(
      (item) =>
        item.kind === "login" &&
        !matching.some((candidate) => candidate.id === item.id),
    ),
  );
  const shown = $derived([...matching, ...otherLogins]);
  const keyTitle = $derived(payload.title?.trim() || payload.name?.trim() || header);

  onMount(async () => {
    if (isApiKey) {
      await tick();
      keyInput?.focus();
      return;
    }
    try {
      items = await invoke<VaultItem[]>("list_vault_items");
    } catch (cause) {
      error = String(cause);
    } finally {
      loading = false;
    }
  });

  async function fill(item: VaultItem) {
    if (answering || filling) return;
    filling = item.id;
    error = "";
    try {
      const result = await invoke<any>("fill_browser_from_vault", {
        vaultItemId: item.id,
      });
      if (result?.ok) {
        onAnswer(
          serializeBlockResponse({
            action_id: "fill",
            values: {
              item_id: item.id,
              item_name: item.name,
              origin: String(result.origin || ""),
            },
          }),
        );
      } else {
        error = String(result?.error || t("vault.revealFailed"));
      }
    } catch (cause) {
      error = String(cause);
    } finally {
      filling = null;
    }
  }

  async function saveKey() {
    if (answering || savingKey || !isApiKey) return;
    const value = keyValue.trim();
    if (!value) {
      error = t("vault.apiKeyRequired");
      return;
    }
    if (value.length > 8192) {
      error = t("vault.apiKeyTooLong");
      return;
    }
    if (/\s/u.test(value)) {
      error = t("vault.apiKeyWhitespace");
      return;
    }
    if (!questionId) {
      error = t("vault.apiKeyUnavailable");
      return;
    }
    savingKey = true;
    error = "";
    try {
      const result = await invoke<any>("submit_api_key_answer", {
        questionId,
        secret: value,
      });
      keyValue = "";
      showKey = false;
      if (result?.answered === false) {
        error = t("vault.apiKeyClosed");
        return;
      }
      onSubmitted?.();
    } catch (cause) {
      error = String(cause);
    } finally {
      savingKey = false;
    }
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key !== "Enter" || event.isComposing) return;
    event.preventDefault();
    void saveKey();
  }

  function cancel() {
    if (answering || savingKey) return;
    keyValue = "";
    showKey = false;
    onAnswer(serializeBlockResponse({ action_id: "cancel" }));
  }
</script>

<article
  class="chat-interaction chat-interaction--vault"
  aria-busy={answering || savingKey ? "true" : undefined}
  aria-label={payload.title || header || (isApiKey ? t("vault.apiKeyRequest") : t("vault.title"))}
>
  <BlockHeader
    {botName}
    kind="vault"
    title={keyTitle}
    busy={answering || savingKey}
  />

  <p class="chat-interaction__question">
    {payload.description || payload.purpose || question}
  </p>

  {#if isApiKey}
    <p class="chat-interaction__hint">
      <ShieldCheck class="size-3 inline" />
      {t("vault.apiKeyHint")}
    </p>
    {#if payload.site}
      <p class="chat-interaction__hint">{payload.site}</p>
    {/if}

    <div class="block-form">
      <label class="block-field" for="vault-api-key-input">
        <span class="block-field__label">{keyTitle}</span>
        <div class="flex items-center gap-1">
          <input
            id="vault-api-key-input"
            bind:this={keyInput}
            bind:value={keyValue}
            class="chat-question__input"
            type={showKey ? "text" : "password"}
            placeholder={payload.placeholder || t("vault.apiKeyPlaceholder")}
            autocomplete="off"
            autocapitalize="off"
            spellcheck="false"
            disabled={answering || savingKey}
            aria-label={t("vault.apiKeyValue")}
            onkeydown={onKeydown}
          />
          <button
            type="button"
            class="chat-interaction__button"
            disabled={answering || savingKey}
            aria-label={showKey ? t("vault.hide") : t("vault.reveal")}
            aria-pressed={showKey}
            onclick={() => (showKey = !showKey)}
          >
            {#if showKey}
              <EyeOff class="size-3.5" />
            {:else}
              <Eye class="size-3.5" />
            {/if}
          </button>
        </div>
      </label>
    </div>

    {#if error}<p class="block-error" role="alert">{error}</p>{/if}

    <footer class="chat-interaction__actions">
      <button
        type="button"
        class="chat-interaction__button"
        disabled={answering || savingKey}
        onclick={cancel}
      >{t("ui.cancel")}</button>
      <button
        type="button"
        class="chat-interaction__button chat-interaction__button--primary"
        disabled={answering || savingKey || !keyValue.trim()}
        onclick={() => void saveKey()}
      >
        {#if savingKey}
          <Loader2 class="size-3.5 animate-spin" />
        {/if}
        {savingKey
          ? t("ui.saving")
          : payload.submit_label || t("vault.apiKeySave")}
      </button>
      <span class="chat-interaction__hint">{t("vault.apiKeySavedHint")}</span>
    </footer>
  {:else}
    <p class="chat-interaction__hint">
      <ShieldCheck class="size-3 inline" />
      {t("vault.requestOrigin", { site: payload.site || "this page" })}
    </p>

    {#if loading}
      <div class="flex items-center gap-2 py-3 text-[11px] text-[var(--text-muted)]">
        <Loader2 class="size-3.5 animate-spin" />{t("ui.loading")}
      </div>
    {:else if shown.length === 0}
      <div class="rounded-lg border border-[var(--hairline)] bg-[var(--surface-0)] p-3 text-[11px] text-[var(--text-tertiary)]">
        <p>{t("vault.noLoginForSite")}</p>
        <button
          type="button"
          class="mt-2 text-[var(--brand-text)] hover:underline cursor-pointer"
          onclick={() => sidePanel.select("vault", sidePanel.scope())}
        >{t("vault.openVault")}</button>
      </div>
    {:else}
      <div class="block-choice-list">
        {#each shown as item (item.id)}
          <button
            type="button"
            class="block-choice"
            disabled={answering || Boolean(filling)}
            onclick={() => void fill(item)}
          >
            <span class="block-choice__mark" aria-hidden="true">
              {#if filling === item.id}
                <Loader2 class="size-3 animate-spin" />
              {:else}
                <KeyRound class="size-3" />
              {/if}
            </span>
            <span class="block-choice__label">
              {item.name}
              {#if item.username}<span class="block-choice__meta"> · {item.username}</span>{/if}
            </span>
            <span class="block-choice__meta">
              {hostMatches(hostOf(item.website), wantedHost) ? t("vault.matchesSite") : hostOf(item.website)}
            </span>
          </button>
        {/each}
      </div>
    {/if}

    {#if error}<p class="block-error" role="alert">{error}</p>{/if}

    <footer class="chat-interaction__actions">
      <button
        type="button"
        class="chat-interaction__button"
        disabled={answering || Boolean(filling)}
        onclick={cancel}
      >{t("ui.cancel")}</button>
      <span class="chat-interaction__hint">{t("vault.requestHint")}</span>
    </footer>
  {/if}
</article>
