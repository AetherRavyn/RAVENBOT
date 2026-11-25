<script lang="ts">
  /**
   * A single- or multi-select list. The selection is local until Submit, so a
   * mis-click on a multi-select does not answer the parked run.
   */
  import { untrack } from "svelte";
  import { Check } from "@lucide/svelte";
  import { t } from "$lib/i18n";
  import BlockHeader from "./BlockHeader.svelte";
  import {
    serializeBlockResponse,
    submitLabel,
    type InteractionBlock,
  } from "$lib/chat/blocks";

  interface Props {
    botName?: string;
    header: string;
    question: string;
    payload: InteractionBlock;
    answering?: boolean;
    onAnswer: (answer: string) => void;
  }

  let { botName = "", header, question, payload, answering = false, onAnswer }: Props = $props();

  const options = $derived(payload.options ?? []);
  const initialSelected = untrack(() =>
    options.filter((option) => option.selected).map((option) => option.id),
  );
  let selected = $state<string[]>(initialSelected);
  let text = $state("");
  let hint = $state("");

  function toggle(id: string) {
    if (answering) return;
    hint = "";
    if (payload.multiple) {
      selected = selected.includes(id)
        ? selected.filter((value) => value !== id)
        : [...selected, id];
      return;
    }
    selected = [id];
  }

  function submit() {
    if (answering) return;
    if (selected.length === 0 && !text.trim()) {
      hint = payload.multiple
        ? t("thread.blockSelectMany")
        : t("thread.blockSelectOne");
      return;
    }
    const values: Record<string, string | string[]> = { selected };
    if (text.trim()) values.text = text.trim();
    onAnswer(serializeBlockResponse({ action_id: "submit", values }));
  }
</script>

<article
  class="chat-interaction chat-interaction--choice"
  aria-busy={answering ? "true" : undefined}
  aria-label={payload.title || header || t("thread.chooseOption")}
>
  <BlockHeader
    {botName}
    kind="choice"
    title={payload.title || header}
    busy={answering}
  />

  {#if payload.description || question}
    <p class="chat-interaction__question">{payload.description || question}</p>
  {/if}

  <div
    class="block-choice-list"
    role="group"
    aria-label={payload.multiple ? t("thread.blockSelectMany") : t("thread.blockSelectOne")}
  >
    {#each options as option (option.id)}
      {@const isSelected = selected.includes(option.id)}
      <button
        type="button"
        class="block-choice"
        data-selected={isSelected ? "" : undefined}
        aria-pressed={isSelected}
        disabled={answering}
        onclick={() => toggle(option.id)}
      >
        <span class="block-choice__mark" aria-hidden="true">
          {#if isSelected}<Check class="size-3" strokeWidth={2.6} />{/if}
        </span>
        <span class="block-choice__label">{option.label}</span>
        {#if option.meta}<span class="block-choice__meta">{option.meta}</span>{/if}
      </button>
    {/each}
  </div>

  {#if payload.allow_text}
    <input
      class="chat-question__input"
      aria-label={t("thread.answerPlaceholder")}
      placeholder={t("thread.answerPlaceholder")}
      bind:value={text}
      disabled={answering}
    />
  {/if}

  <footer class="chat-interaction__actions">
    <button
      type="button"
      class="chat-interaction__button chat-interaction__button--primary"
      disabled={answering || (selected.length === 0 && !text.trim())}
      onclick={submit}
    >
      {answering ? t("thread.sendingAnswer") : submitLabel(payload, t("thread.blockSubmit"))}
    </button>
    {#if hint}<span class="block-error" role="status">{hint}</span>{/if}
    <span class="chat-interaction__hint">{t("thread.blockResumeHint")}</span>
  </footer>
</article>
