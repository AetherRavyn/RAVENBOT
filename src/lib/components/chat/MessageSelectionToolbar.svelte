<script lang="ts">
  /**
   * The small actions that appear over a text selection in the transcript.
   *
   * The transcript owns the selection, so this component only reads it. Reply
   * and Quote call back with the row id; the chat surface decides which message
   * that id names and how a quote enters its composer. A selection that crosses
   * controls or messages is ignored by `selectionQuote`, not by guesswork here.
   */
  import { onMount, tick } from "svelte";
  import { Copy, Reply, TextQuote } from "@lucide/svelte";
  import { t } from "$lib/i18n";
  import {
    selectionQuote,
    selectionToolbarPosition,
    type SelectionQuote,
    type ToolbarPlacement,
  } from "$lib/chat/selection";

  interface Props {
    /** The scroll container that holds the message rows. */
    container: HTMLElement | null;
    /** Disable the actions while the surface cannot accept a quote. */
    disabled?: boolean;
    onReply?: (messageId: string) => void;
    onQuote?: (selection: SelectionQuote) => void;
  }

  let { container, disabled = false, onReply, onQuote }: Props = $props();

  let active = $state<SelectionQuote | null>(null);
  let positioned = $state(false);
  let toolbar = $state<HTMLDivElement | null>(null);
  let seq = 0;
  let top = $state(0);
  let left = $state(0);
  let placement = $state<ToolbarPlacement["placement"]>("bottom");

  function dismiss() {
    seq += 1;
    active = null;
    positioned = false;
  }

  /** Read the selection, then measure the toolbar once it is in the DOM. */
  async function inspect() {
    if (disabled) {
      dismiss();
      return;
    }
    const next = selectionQuote(window.getSelection(), container);
    if (!next) {
      dismiss();
      return;
    }
    const ticket = ++seq;
    active = next;
    positioned = false;
    await tick();
    // A second selection can land while this one waits for the DOM. Only the
    // newest read may place the toolbar.
    if (ticket !== seq || !toolbar || !active) return;
    // The toolbar has no size until it is in the DOM. Measure it, then place it
    // against the live viewport rather than the stale selection rectangle.
    const position = selectionToolbarPosition(
      active.rect,
      {
        width: toolbar.offsetWidth || 160,
        height: toolbar.offsetHeight || 32,
      },
      { width: window.innerWidth, height: window.innerHeight },
    );
    top = position.top;
    left = position.left;
    placement = position.placement;
    positioned = true;
  }

  function clearSelection() {
    window.getSelection()?.removeAllRanges();
    dismiss();
  }

  function reply() {
    if (!active) return;
    const messageId = active.messageId;
    clearSelection();
    onReply?.(messageId);
  }

  function quote() {
    if (!active) return;
    const selection = active;
    clearSelection();
    onQuote?.(selection);
  }

  async function copy() {
    if (!active) return;
    const text = active.text;
    clearSelection();
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      // A denied clipboard is not a reason to lose the selection silently;
      // the transcript still has it, and the user can use the browser menu.
    }
  }

  onMount(() => {
    // `pointerup` fires before the selection is final in some browsers, so the
    // read waits for Svelte's flush rather than the next animation frame.
    // `selectionchange` alone is too eager: it fires on every caret move in a
    // text field.
    const onPointerUp = (event: PointerEvent) => {
      if (event.target instanceof Element && event.target.closest(".message-selection-toolbar")) return;
      void inspect();
    };
    const onPointerDown = (event: PointerEvent) => {
      if (event.target instanceof Element && event.target.closest(".message-selection-toolbar")) return;
      dismiss();
    };
    const onKeyUp = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        dismiss();
        return;
      }
      if (event.shiftKey || event.key.startsWith("Arrow")) void inspect();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      // Cmd/Ctrl+Shift+. quotes the passage, matching the shortcut readers of
      // OpenBot already know. The selection must be captured first; a bare
      // chord with no selection does nothing.
      if ((event.metaKey || event.ctrlKey) && event.shiftKey && event.key === "." && active) {
        event.preventDefault();
        quote();
      }
    };
    const onScroll = () => dismiss();

    document.addEventListener("pointerup", onPointerUp);
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keyup", onKeyUp);
    document.addEventListener("keydown", onKeyDown);
    window.addEventListener("scroll", onScroll, true);
    return () => {
      document.removeEventListener("pointerup", onPointerUp);
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keyup", onKeyUp);
      document.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("scroll", onScroll, true);
    };
  });
</script>

{#if active}
  <div
    bind:this={toolbar}
    class="message-selection-toolbar"
    class:message-selection-toolbar--measuring={!positioned}
    data-placement={placement}
    style:top={`${top}px`}
    style:left={`${left}px`}
    role="toolbar"
    aria-label={t("thread.selectionActions")}
  >
    <button
      type="button"
      class="message-selection-toolbar__action"
      onclick={reply}
      title={t("thread.reply")}
    >
      <Reply class="size-3.5" strokeWidth={1.9} />
      <span>{t("thread.reply")}</span>
    </button>
    <button
      type="button"
      class="message-selection-toolbar__action"
      onclick={quote}
      title={t("thread.quoteSelection")}
    >
      <TextQuote class="size-3.5" strokeWidth={1.9} />
      <span>{t("thread.quoteSelection")}</span>
    </button>
    <button
      type="button"
      class="message-selection-toolbar__action message-selection-toolbar__action--icon"
      onclick={copy}
      title={t("thread.copySelection")}
      aria-label={t("thread.copySelection")}
    >
      <Copy class="size-3.5" strokeWidth={1.9} />
    </button>
  </div>
{/if}
