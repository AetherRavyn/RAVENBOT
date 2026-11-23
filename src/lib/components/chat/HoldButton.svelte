<script lang="ts">
  /**
   * A destructive action that cannot be completed with one click.
   *
   * Holding is checked by a timer, not only by CSS: an animation can be
   * interrupted, reduced, or skipped by automation, and the action must still
   * require the full duration. Pointer leave, cancel, blur, and key release all
   * reset it.
   */
  import { onDestroy } from "svelte";
  import { t } from "$lib/i18n";

  interface Props {
    label: string;
    holdMs?: number;
    disabled?: boolean;
    onConfirm: () => void;
  }

  let { label, holdMs = 800, disabled = false, onConfirm }: Props = $props();

  let holding = $state(false);
  let timer: ReturnType<typeof setTimeout> | null = null;

  function stop() {
    holding = false;
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
  }

  function start() {
    if (disabled || timer !== null) return;
    holding = true;
    timer = setTimeout(() => {
      timer = null;
      holding = false;
      if (!disabled) onConfirm();
    }, Math.max(250, holdMs));
  }

  onDestroy(stop);
</script>

<button
  type="button"
  class="chat-interaction__button chat-interaction__button--danger hold-button"
  data-holding={holding ? "" : undefined}
  style:--hold-ms={`${Math.max(250, holdMs)}ms`}
  disabled={disabled}
  aria-label={t("thread.blockHoldTo", { action: label })}
  title={t("thread.blockHoldHint")}
  onpointerdown={(event) => {
    event.preventDefault();
    start();
  }}
  onpointerup={stop}
  onpointercancel={stop}
  onpointerleave={stop}
  onkeydown={(event) => {
    if (event.key !== " " && event.key !== "Enter") return;
    event.preventDefault();
    start();
  }}
  onkeyup={stop}
  onblur={stop}
>
  <span class="hold-button__fill" aria-hidden="true"></span>
  <span class="hold-button__label">{label}</span>
</button>
