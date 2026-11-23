<script lang="ts">
  /**
   * A confirmation card with details and up to four actions.
   *
   * `preview` is the only prose field. It goes through the shared safe Markdown
   * renderer; every other value is plain text. A dangerous or `confirm` action
   * uses the hold control so a stray click cannot send it.
   */
  import { untrack } from "svelte";
  import MarkdownRenderer from "$lib/components/MarkdownRenderer.svelte";
  import { t } from "$lib/i18n";
  import BlockHeader from "./BlockHeader.svelte";
  import HoldButton from "./HoldButton.svelte";
  import {
    serializeBlockResponse,
    type BlockAction,
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
  // The card is mounted for one request. Capture the initial field defaults
  // without making the form reactive to a payload that cannot change.
  const initialFields = untrack(() => payload.fields ?? []);
  let values = $state<Record<string, string>>(
    Object.fromEntries(
      initialFields
        .filter((field) => field.kind === "select" && (field.options?.length ?? 0) > 0)
        .map((field) => [field.id, field.value || field.options?.[0] || ""]),
    ),
  );

  function respond(action: BlockAction) {
    if (answering) return;
    onAnswer(serializeBlockResponse({ action_id: action.id, values }));
  }

  function needsHold(action: BlockAction): boolean {
    return Boolean(
      action.danger ||
        action.confirm ||
        (payload.danger && action.style === "primary"),
    );
  }

  function actionClass(action: BlockAction): string {
    const style =
      action.style === "danger"
        ? "danger"
        : action.style === "primary"
          ? "primary"
          : "secondary";
    return `chat-interaction__button chat-interaction__button--${style}`;
  }
</script>

<article
  class="chat-interaction chat-interaction--confirm"
  data-danger={payload.danger ? "" : undefined}
  aria-busy={answering ? "true" : undefined}
  aria-label={payload.title || header || "Confirmation"}
>
  <BlockHeader
    {botName}
    kind="confirm"
    title={payload.title || header}
    danger={payload.danger}
    busy={answering}
  />

  {#if payload.description || question}
    <p class="chat-interaction__question">{payload.description || question}</p>
  {/if}

  {#if fields.length}
    <div class="block-fields">
      {#each fields as field (field.id)}
        {#if field.kind === "select" && (field.options?.length ?? 0) > 0}
          <label class="block-field">
            <span class="block-field__label">{field.label}</span>
            <select class="block-select" bind:value={values[field.id]} disabled={answering}>
              {#each field.options ?? [] as option (option)}
                <option value={option}>{option}</option>
              {/each}
            </select>
          </label>
        {:else}
          <div class="block-detail">
            <span class="block-detail__label">{field.label}</span>
            <span class="block-detail__value">{field.value || "—"}</span>
          </div>
        {/if}
      {/each}
    </div>
  {/if}

  {#if payload.preview}
    <div class="block-preview">
      <MarkdownRenderer content={payload.preview} />
    </div>
  {/if}

  <footer class="chat-interaction__actions">
    {#each payload.actions ?? [] as action (action.id)}
      {#if needsHold(action)}
        <HoldButton
          label={action.label}
          holdMs={action.hold_ms ?? 800}
          disabled={answering}
          onConfirm={() => respond(action)}
        />
      {:else}
        <button
          type="button"
          class={actionClass(action)}
          disabled={answering}
          onclick={() => respond(action)}
        >
          {action.label}
        </button>
      {/if}
    {/each}
    <span class="chat-interaction__hint">{t("thread.blockResumeHint")}</span>
  </footer>
</article>
