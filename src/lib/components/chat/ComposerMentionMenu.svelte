<script lang="ts">
  /**
   * The picker that follows `@`, `#`, or `$` in the composer.
   *
   * Keyboard movement lives in the composing textarea; this component only
   * renders the current list and the active row. A pointer pick prevents the
   * textarea from losing focus, so the caret stays where the token was.
   */
  import { AtSign, FileText, Sparkles } from "@lucide/svelte";
  import { t } from "$lib/i18n";
  import type { MentionItem, MentionSigil } from "$lib/chat/mentions";

  interface Props {
    sigil: MentionSigil;
    items: MentionItem[];
    activeIndex: number;
    onPick: (item: MentionItem) => void;
  }

  let { sigil, items, activeIndex, onPick }: Props = $props();

  function iconFor(item: MentionItem) {
    if (item.kind === "agent") return AtSign;
    if (item.kind === "skill") return Sparkles;
    return FileText;
  }
</script>

<div class="composer-mentions" aria-label={t("thread.mentionPicker")}>
  {#each items as item, index (item.kind + ":" + item.id)}
    {@const Icon = iconFor(item)}
    <button
      type="button"
      class="composer-mention"
      data-active={index === activeIndex ? "" : undefined}
      onmousedown={(event) => event.preventDefault()}
      onclick={() => onPick(item)}
    >
      <span class="composer-mention__icon" aria-hidden="true">
        <Icon class="size-3.5" strokeWidth={1.8} />
      </span>
      <span class="composer-mention__copy">
        <span class="composer-mention__label">
          {#if sigil === "#"}<span class="composer-mention__sigil">#</span>{/if}
          {item.label}
        </span>
        {#if item.detail}<span class="composer-mention__detail">{item.detail}</span>{/if}
      </span>
      <span class="composer-mention__kind">
        {#if item.kind === "agent"}@
        {:else if item.kind === "skill"}$
        {:else}#
        {/if}
      </span>
    </button>
  {/each}
</div>
