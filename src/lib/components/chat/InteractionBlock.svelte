<script lang="ts">
  /**
   * The single entry point both chat surfaces render for a parked question.
   *
   * A row written before structured blocks existed has no `kind` and falls
   * through to the original question card. A row with an unknown or incomplete
   * block also falls back, so a bad payload never blanks the conversation.
   */
  import QuestionCard from "./QuestionCard.svelte";
  import ConfirmBlock from "./ConfirmBlock.svelte";
  import ChoiceBlock from "./ChoiceBlock.svelte";
  import FormBlock from "./FormBlock.svelte";
  import VaultRequestCard from "./VaultRequestCard.svelte";
  import type {
    InteractionBlock as BlockPayload,
    QuestionKind,
  } from "$lib/chat/blocks";

  interface Props {
    botName?: string;
    header?: string;
    question: string;
    options?: string[];
    allowCustom?: boolean;
    kind?: QuestionKind | null;
    payload?: BlockPayload | null;
    questionId?: string;
    answering?: boolean;
    onAnswer: (answer: string) => void;
    onSubmitted?: () => void;
  }

  let {
    botName = "",
    header = "",
    question,
    options = [],
    allowCustom = true,
    kind = "question",
    payload = null,
    questionId = "",
    answering = false,
    onAnswer,
    onSubmitted,
  }: Props = $props();
</script>

{#if kind === "confirm" && payload}
  <ConfirmBlock {botName} {header} {question} {payload} {answering} {onAnswer} />
{:else if kind === "choice" && payload}
  <ChoiceBlock {botName} {header} {question} {payload} {answering} {onAnswer} />
{:else if kind === "form" && payload}
  <FormBlock {botName} {header} {question} {payload} {answering} {onAnswer} />
{:else if kind === "vault" && payload}
  <VaultRequestCard
    {botName}
    {header}
    {question}
    {payload}
    {questionId}
    {answering}
    {onAnswer}
    {onSubmitted}
  />
{:else}
  <QuestionCard
    {botName}
    {header}
    {question}
    {options}
    {allowCustom}
    {answering}
    {onAnswer}
  />
{/if}
