<script lang="ts">
  /**
   * The in-thread search popover.
   *
   * The owning view keeps the query, match list, and current position because
   * a hit also drives row highlighting. This component owns only the input
   * element and forwards every action, including the debounced typing hook.
   */
  import { ChevronDown, ChevronUp, Search, X } from "@lucide/svelte";
  import { t } from "$lib/i18n";

  interface Props {
    query: string;
    matchCount: number;
    position: number;
    input?: HTMLInputElement | null;
    onQueryChange: (value: string) => void;
    onStep: (delta: number) => void;
    onClose: () => void;
  }

  let {
    query,
    matchCount,
    position,
    input = $bindable(null),
    onQueryChange,
    onStep,
    onClose,
  }: Props = $props();
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="thread-search"
  role="search"
  onkeydown={(event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    }
  }}
>
  <Search class="thread-search__icon size-3.5" aria-hidden="true" />
  <input
    bind:this={input}
    class="thread-search__input"
    type="search"
    value={query}
    placeholder={t("thread.searchMessages")}
    aria-label={t("thread.searchMessages")}
    oninput={(event) => onQueryChange(event.currentTarget.value)}
    onkeydown={(event) => {
      if (event.key === "Enter") {
        event.preventDefault();
        onStep(event.shiftKey ? -1 : 1);
      }
    }}
  />
  <span class="thread-search__count" aria-live="polite">
    {#if matchCount}
      {t("thread.searchCount", {
        current: position + 1,
        total: matchCount,
      })}
    {:else if query.trim()}
      {t("thread.searchNoResults")}
    {/if}
  </span>
  <button
    type="button"
    class="thread-search__button"
    disabled={!matchCount}
    onclick={() => onStep(-1)}
    aria-label={t("thread.searchPrevious")}
    title={t("thread.searchPrevious")}
  >
    <ChevronUp class="size-3.5" />
  </button>
  <button
    type="button"
    class="thread-search__button"
    disabled={!matchCount}
    onclick={() => onStep(1)}
    aria-label={t("thread.searchNext")}
    title={t("thread.searchNext")}
  >
    <ChevronDown class="size-3.5" />
  </button>
  <button
    type="button"
    class="thread-search__button"
    onclick={onClose}
    aria-label={t("thread.searchClose")}
    title={t("thread.searchClose")}
  >
    <X class="size-3.5" />
  </button>
</div>
