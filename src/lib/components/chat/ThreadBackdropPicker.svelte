<script lang="ts">
  /**
   * Per-conversation backdrop picker.
   *
   * The four bundled images are static files. Uploads are copied into the
   * RAVENBOT data directory by the backend, which also checks the extension,
   * magic bytes and the 100 MB cap; this dialog never receives a URL it could
   * load from the network. Clear and Undo are separate because clearing a
   * backdrop is a normal action and undoing it should not require re-picking
   * the file.
   */
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { Button } from "$lib/components/ui/button";
  import { t } from "$lib/i18n";
  import {
    BACKDROP_IMAGE_FILTER,
    BACKDROP_VIDEO_FILTER,
    BUILTIN_BACKDROPS,
    backdropLabel,
    chooseBuiltinBackdrop,
    clearThreadBackdrop,
    importThreadBackdrop,
    undoThreadBackdrop,
    type ThreadBackdropState,
  } from "$lib/chat/backdrop";
  import { Image, Undo2, X } from "@lucide/svelte";

  interface Props {
    threadId: string;
    initial: ThreadBackdropState;
    onStateChange: (state: ThreadBackdropState) => void;
    onClose: () => void;
  }

  let { threadId, initial, onStateChange, onClose }: Props = $props();

  // svelte-ignore state_referenced_locally
  let selection = $state<ThreadBackdropState>(initial);
  let busy = $state(false);
  let error = $state("");

  let current = $derived(selection.current);

  function update(next: ThreadBackdropState) {
    selection = next;
    onStateChange(next);
    error = "";
  }

  async function run(action: () => Promise<ThreadBackdropState>) {
    if (busy) return;
    busy = true;
    error = "";
    try {
      update(await action());
    } catch (cause) {
      error = String(cause);
    } finally {
      busy = false;
    }
  }

  async function pick(kind: "image" | "video") {
    if (busy) return;
    const selected = await openDialog({
      multiple: false,
      directory: false,
      filters: [kind === "image" ? BACKDROP_IMAGE_FILTER : BACKDROP_VIDEO_FILTER],
    });
    if (typeof selected !== "string" || !selected) return;
    await run(() => importThreadBackdrop(threadId, selected));
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-[60] flex items-center justify-center bg-[var(--scrim)] p-4"
  role="dialog"
  aria-modal="true"
  aria-label={t("backdrop.title")}
  tabindex="-1"
  onclick={(event) => {
    if (event.target === event.currentTarget) onClose();
  }}
  onkeydown={(event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    }
  }}
>
  <div class="max-h-[90vh] w-full max-w-xl overflow-y-auto rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-5 shadow-2xl">
    <header class="flex items-start justify-between gap-3">
      <div>
        <h3 class="flex items-center gap-2 text-sm font-bold text-[var(--text-primary)]">
          <Image class="size-4 text-[var(--brand-text)]" />
          {t("backdrop.title")}
        </h3>
        <p class="mt-1 text-xs text-[var(--text-tertiary)]">{t("backdrop.description")}</p>
      </div>
      <Button
        size="sm"
        variant="ghost"
        class="h-7 px-2"
        aria-label={t("ui.close")}
        onclick={onClose}
      >
        <X class="size-3.5" />
      </Button>
    </header>

    <section class="mt-4">
      <span class="text-[11px] font-bold uppercase tracking-wide text-[var(--text-tertiary)]">
        {t("backdrop.builtin")}
      </span>
      <div class="mt-2 grid grid-cols-2 gap-2">
        {#each BUILTIN_BACKDROPS as item (item.id)}
          {@const selected = current?.kind === "builtin" && current.id === item.id}
          <button
            type="button"
            class="group relative overflow-hidden rounded-xl border text-left transition-all focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--brand)] cursor-pointer {selected
              ? 'border-[var(--brand)] ring-2 ring-[var(--brand)]'
              : 'border-[var(--hairline)] hover:border-[var(--hairline-strong)]'}"
            aria-pressed={selected}
            disabled={busy}
            onclick={() => void run(() => chooseBuiltinBackdrop(threadId, item.id))}
          >
            <img src={item.path} alt="" class="h-20 w-full object-cover opacity-90" />
            <span class="absolute inset-x-0 bottom-0 bg-gradient-to-t from-black/70 to-transparent px-2 pb-1.5 pt-5 text-[11px] font-semibold text-white">
              {item.name}
            </span>
          </button>
        {/each}
      </div>
    </section>

    <section class="mt-4">
      <span class="text-[11px] font-bold uppercase tracking-wide text-[var(--text-tertiary)]">
        {t("backdrop.upload")}
      </span>
      <div class="mt-2 flex flex-wrap gap-2">
        <Button size="sm" variant="outline" disabled={busy} onclick={() => void pick("image")}>
          {t("backdrop.uploadImage")}
        </Button>
        <Button size="sm" variant="outline" disabled={busy} onclick={() => void pick("video")}>
          {t("backdrop.uploadVideo")}
        </Button>
      </div>
      <p class="mt-1.5 text-[11px] text-[var(--text-muted)]">{t("backdrop.uploadHint")}</p>
    </section>

    {#if current}
      <div class="mt-4 flex flex-wrap items-center gap-2 rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] px-3 py-2">
        <span class="min-w-0 flex-1 truncate text-xs text-[var(--text-secondary)]" title={backdropLabel(current)}>
          {backdropLabel(current)}
        </span>
        {#if current.kind !== "builtin"}
          <span class="rounded-full border border-[var(--hairline)] px-1.5 py-0.5 font-mono text-[11px] uppercase text-[var(--text-tertiary)]">
            {current.kind}
          </span>
        {/if}
        <Button
          size="sm"
          variant="ghost"
          class="h-7 px-2 text-[var(--danger-text)]"
          disabled={busy}
          onclick={() => void run(() => clearThreadBackdrop(threadId))}
        >
          {t("backdrop.clear")}
        </Button>
        {#if selection.previous}
          <Button
            size="sm"
            variant="ghost"
            class="h-7 gap-1 px-2"
            disabled={busy}
            onclick={() => void run(() => undoThreadBackdrop(threadId))}
          >
            <Undo2 class="size-3.5" />
            {t("backdrop.undo")}
          </Button>
        {/if}
      </div>
    {:else if selection.previous}
      <div class="mt-4 flex justify-end">
        <Button
          size="sm"
          variant="outline"
          class="gap-1"
          disabled={busy}
          onclick={() => void run(() => undoThreadBackdrop(threadId))}
        >
          <Undo2 class="size-3.5" />
          {t("backdrop.undo")}
        </Button>
      </div>
    {/if}

    {#if error}
      <p class="mt-3 rounded-lg border border-[var(--danger-border)] bg-[var(--danger-soft)] px-2.5 py-2 text-xs text-[var(--danger-text)]" role="alert">
        {t("backdrop.error", { error })}
      </p>
    {/if}

    <footer class="mt-5 flex items-center justify-between gap-2">
      <span class="truncate text-[11px] text-[var(--text-muted)]">
        {current ? "" : t("backdrop.none")}
      </span>
      <Button size="sm" variant="outline" onclick={onClose}>{t("ui.close")}</Button>
    </footer>
  </div>
</div>
