<script lang="ts">
  /**
   * An agent asks the local vault for a login. The card lists metadata only;
   * the user explicitly chooses an item, and the secret goes from the keychain
   * into the current browser page. The model receives only the item name,
   * origin and result.
   */
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import { sidePanel } from "$lib/workspace/sidePanel.svelte";
  import BlockHeader from "./BlockHeader.svelte";
  import {
    serializeBlockResponse,
    type InteractionBlock,
  } from "$lib/chat/blocks";
  import type { VaultItem } from "$lib/vault";
  import { KeyRound, Loader2, ShieldCheck } from "@lucide/svelte";

  interface Props {
    botName?: string;
    header: string;
    question: string;
    payload: InteractionBlock;
    answering?: boolean;
    onAnswer: (answer: string) => void;
  }

  let { botName = "", header, question, payload, answering = false, onAnswer }: Props = $props();

  let items = $state<VaultItem[]>([]);
  let loading = $state(true);
  let filling = $state<string | null>(null);
  let error = $state("");

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

  onMount(async () => {
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

  function cancel() {
    if (answering) return;
    onAnswer(serializeBlockResponse({ action_id: "cancel" }));
  }
</script>

<article
  class="chat-interaction chat-interaction--vault"
  aria-busy={answering ? "true" : undefined}
  aria-label={payload.title || header || "Vault request"}
>
  <BlockHeader
    {botName}
    kind="vault"
    title={payload.title || header}
    busy={answering}
  />

  <p class="chat-interaction__question">
    {payload.purpose || question}
  </p>
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
</article>
