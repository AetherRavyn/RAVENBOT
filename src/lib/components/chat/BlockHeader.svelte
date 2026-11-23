<script lang="ts">
  /**
   * The shared header of a structured interaction card.
   *
   * The tone is carried by a chip and an icon border, never by a full-card
   * wash: a dangerous confirmation should read as a decision, not an error.
   */
  import { CircleHelp, ListChecks, Lock, ShieldAlert, TextCursorInput } from "@lucide/svelte";
  import { t } from "$lib/i18n";
  import type { QuestionKind } from "$lib/chat/blocks";

  interface Props {
    botName?: string;
    kind: Exclude<QuestionKind, "question">;
    title: string;
    danger?: boolean;
    busy?: boolean;
    chip?: string;
  }

  let { botName = "", kind, title, danger = false, busy = false, chip }: Props = $props();

  const tone = $derived(danger ? "warning" : "question");
  const chipText = $derived(chip || t("thread.blockNeedsYou"));
  const eyebrow = $derived(
    botName
      ? t("thread.blockConfirmEyebrow", { name: botName })
      : t("thread.blockRequest"),
  );
</script>

<header class="chat-interaction__header">
  <span class="chat-interaction__icon" data-tone={tone} aria-hidden="true">
    {#if kind === "confirm"}
      <ShieldAlert class="size-4" strokeWidth={1.8} />
    {:else if kind === "choice"}
      <ListChecks class="size-4" strokeWidth={1.8} />
    {:else if kind === "vault"}
      <Lock class="size-4" strokeWidth={1.8} />
    {:else}
      <TextCursorInput class="size-4" strokeWidth={1.8} />
    {/if}
  </span>
  <div class="chat-interaction__heading">
    <span class="chat-interaction__eyebrow">{eyebrow}</span>
    <h3 class="chat-interaction__title">{title}</h3>
  </div>
  <span class="chat-interaction__chip" data-tone={tone} aria-busy={busy ? "true" : undefined}>
    {chipText}
  </span>
</header>
