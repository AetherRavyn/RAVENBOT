<script lang="ts">
  /**
   * A question an agent needs answered before its run can continue.
   *
   * Reply chips are the primary path: one click is the whole interaction when
   * the agent already named the useful choices. The free-text field stays
   * available unless the agent explicitly turned it off, because a question
   * with four options still sometimes has a fifth answer.
   */
  import { CircleHelp, Loader2, Send } from "@lucide/svelte";
  import { t } from "$lib/i18n";

  interface Props {
    botName?: string;
    header?: string;
    question: string;
    options?: string[];
    allowCustom?: boolean;
    answering?: boolean;
    onAnswer: (answer: string) => void;
    class?: string;
  }

  let {
    botName = "",
    header = "",
    question,
    options = [],
    allowCustom = true,
    answering = false,
    onAnswer,
    class: cls = "",
  }: Props = $props();

  let draft = $state("");
  const choices = $derived(options.map((option) => option.trim()).filter(Boolean));

  function submit() {
    const value = draft.trim();
    if (!value || answering) return;
    onAnswer(value);
  }
</script>

<article
  class={"chat-interaction chat-interaction--question " + cls}
  aria-busy={answering ? "true" : undefined}
  aria-label={header || "Question from an agent"}
>
  <header class="chat-interaction__header">
    <span class="chat-interaction__icon" data-tone="question" aria-hidden="true">
      <CircleHelp class="size-4" strokeWidth={1.8} />
    </span>
    <div class="chat-interaction__heading">
      <span class="chat-interaction__eyebrow">
        {#if botName}<span class="chat-interaction__agent">{botName}</span> asks{:else}Agent question{/if}
      </span>
      <h3 class="chat-interaction__title">{header || "Question"}</h3>
    </div>
    <span class="chat-interaction__chip" data-tone="question">Needs you</span>
  </header>

  <p class="chat-interaction__question">{question}</p>

  {#if choices.length}
    <div class="chat-question__options" role="group" aria-label="Suggested answers">
      {#each choices as option (option)}
        <button
          type="button"
          class="chat-question__chip"
          disabled={answering}
          onclick={() => onAnswer(option)}
        >
          {option}
        </button>
      {/each}
    </div>
  {/if}

  {#if allowCustom}
    <form
      class="chat-question__form"
      novalidate
      onsubmit={(event) => {
        event.preventDefault();
        submit();
      }}
    >
      <input
        class="chat-question__input"
        aria-label="Answer"
        placeholder={t("thread.answerPlaceholder")}
        bind:value={draft}
        disabled={answering}
      />
      <button
        type="submit"
        class="chat-interaction__button chat-interaction__button--primary chat-question__send"
        disabled={answering || !draft.trim()}
      >
        {#if answering}
          <Loader2 class="size-3.5 animate-spin" />
          Sending…
        {:else}
          <Send class="size-3.5" strokeWidth={2} />
          Answer
        {/if}
      </button>
    </form>
  {/if}
</article>
