<script lang="ts">
  /**
   * A small form. Values are strings; a select's value is one of its options.
   * Required fields are checked before the run receives anything, so the model
   * never gets a half-filled block.
   */
  import { untrack } from "svelte";
  import { t } from "$lib/i18n";
  import BlockHeader from "./BlockHeader.svelte";
  import {
    missingRequiredFields,
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

  const fields = $derived(payload.fields ?? []);
  const initialFields = untrack(() => payload.fields ?? []);
  let values = $state<Record<string, string>>(
    Object.fromEntries(
      initialFields.map((field) => [
        field.id,
        field.value || (field.kind === "select" ? field.options?.[0] || "" : ""),
      ]),
    ),
  );
  let error = $state("");

  function submit() {
    if (answering) return;
    const missing = missingRequiredFields(fields, values);
    if (missing.length) {
      error = `${t("thread.blockRequired")}: ${missing.map((field) => field.label).join(", ")}`;
      return;
    }
    error = "";
    onAnswer(serializeBlockResponse({ action_id: "submit", values }));
  }
</script>

<article
  class="chat-interaction chat-interaction--form"
  aria-busy={answering ? "true" : undefined}
  aria-label={payload.title || header || t("thread.form")}
>
  <BlockHeader
    {botName}
    kind="form"
    title={payload.title || header}
    busy={answering}
  />

  {#if payload.description || question}
    <p class="chat-interaction__question">{payload.description || question}</p>
  {/if}

  <div class="block-form">
    {#each fields as field (field.id)}
      <label class="block-field" for={`block-${field.id}`}>
        <span class="block-field__label">
          {field.label}
          {#if field.required}<span class="block-field__required">{t("thread.blockRequired")}</span>{/if}
        </span>
        {#if field.kind === "textarea"}
          <textarea
            id={`block-${field.id}`}
            class="chat-question__input block-textarea"
            placeholder={field.placeholder || ""}
            required={field.required}
            disabled={answering}
            bind:value={values[field.id]}
          ></textarea>
        {:else if field.kind === "select"}
          <select
            id={`block-${field.id}`}
            class="block-select"
            required={field.required}
            disabled={answering}
            bind:value={values[field.id]}
          >
            {#each field.options ?? [] as option (option)}
              <option value={option}>{option}</option>
            {/each}
          </select>
        {:else}
          <input
            id={`block-${field.id}`}
            class="chat-question__input"
            type="text"
            placeholder={field.placeholder || ""}
            required={field.required}
            disabled={answering}
            bind:value={values[field.id]}
          />
        {/if}
      </label>
    {/each}
  </div>

  <footer class="chat-interaction__actions">
    <button
      type="button"
      class="chat-interaction__button chat-interaction__button--primary"
      disabled={answering}
      onclick={submit}
    >
      {answering ? t("thread.sendingAnswer") : submitLabel(payload, t("thread.blockSubmit"))}
    </button>
    {#if error}<span class="block-error" role="alert">{error}</span>{/if}
    <span class="chat-interaction__hint">{t("thread.blockResumeHint")}</span>
  </footer>
</article>
