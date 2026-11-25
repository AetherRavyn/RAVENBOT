<script lang="ts">
  /**
   * A permission request from an agent that has parked mid-run.
   *
   * This is one of the few places the interface deliberately interrupts the
   * conversation. It reads as a card rather than a chat bubble because it is
   * not speech: it is a gate, and the run does not continue until a person
   * chooses. The risk tone is carried by a chip and a border, not by a loud
   * full-card colour, so a high-stakes request still reads as a decision rather
   * than an error.
   */
  import { Check, Loader2, ShieldAlert, X } from "@lucide/svelte";
  import { t } from "$lib/i18n";
  import { approvalSummary } from "$lib/chat/interactionSummary";

  interface Props {
    botName?: string;
    toolName: string;
    toolLabel?: string;
    args?: unknown;
    risk?: string;
    deciding?: boolean;
    onDecide: (allowed: boolean) => void;
    class?: string;
  }

  let {
    botName = "",
    toolName,
    toolLabel = "",
    args = null,
    risk = "low",
    deciding = false,
    onDecide,
    class: cls = "",
  }: Props = $props();

  const summary = $derived(approvalSummary(args));
  const high = $derived(risk === "high");
  const action = $derived(toolLabel.trim() || toolName);
  const title = $derived(
    botName
      ? t("thread.namedWantsTo", { name: botName, action })
      : t("thread.agentWantsTo", { action }),
  );
</script>

<article
  class={"chat-interaction chat-interaction--approval " + cls}
  data-risk={risk}
  aria-busy={deciding ? "true" : undefined}
  aria-label={title}
>
  <header class="chat-interaction__header">
    <span class="chat-interaction__icon" data-tone={high ? "warning" : "brand"} aria-hidden="true">
      <ShieldAlert class="size-4" strokeWidth={1.8} />
    </span>
    <div class="chat-interaction__heading">
      <span class="chat-interaction__eyebrow">{t("thread.permissionRequest")}</span>
      <h3 class="chat-interaction__title">
        {#if botName}
          <span class="chat-interaction__agent">{botName}</span> {t("thread.wantsTo", { action })}
        {:else}
          {t("thread.agentWantsTo", { action })}
        {/if}
      </h3>
    </div>
    <span class="chat-interaction__chip" data-tone={high ? "warning" : "neutral"}>
      {high ? t("thread.highStakesChip") : t("thread.review")}
    </span>
  </header>

  {#if summary}
    <pre class="chat-interaction__summary">{summary}</pre>
  {/if}

  {#if high}
    <p class="chat-interaction__note">{t("thread.highStakes")}</p>
  {/if}

  <footer class="chat-interaction__actions">
    <button
      type="button"
      class="chat-interaction__button chat-interaction__button--primary"
      disabled={deciding}
      onclick={() => onDecide(true)}
    >
      {#if deciding}
        <Loader2 class="size-3.5 animate-spin" />
        {t("thread.allowing")}
      {:else}
        <Check class="size-3.5" strokeWidth={2.2} />
        {t("thread.allow")}
      {/if}
    </button>
    <button
      type="button"
      class="chat-interaction__button chat-interaction__button--danger"
      disabled={deciding}
      onclick={() => onDecide(false)}
    >
      <X class="size-3.5" strokeWidth={2.2} />
      {t("thread.deny")}
    </button>
    <span class="chat-interaction__hint">{t("thread.resumeAfterDecision", { tool: toolName })}</span>
  </footer>
</article>
