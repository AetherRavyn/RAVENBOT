<script lang="ts">
  /**
   * The keyboard cheat sheet.
   *
   * Only implemented shortcuts are listed. The modifier glyph follows the
   * platform, and the sheet is reachable from the command palette and from
   * `Shift+?` outside a text field.
   */
  import { onMount, tick } from "svelte";
  import { Keyboard, X } from "@lucide/svelte";
  import { t, type TranslationKey } from "$lib/i18n";

  interface Props {
    open: boolean;
    onClose: () => void;
  }

  let { open, onClose }: Props = $props();
  let sheetEl = $state<HTMLElement | null>(null);

  const mod = (() => {
    if (typeof navigator === "undefined") return "Ctrl";
    return /Mac|iPhone|iPad|iPod/i.test(navigator.platform) ? "⌘" : "Ctrl";
  })();

  interface Shortcut {
    keys: string[];
    label: TranslationKey;
  }
  interface ShortcutGroup {
    id: string;
    label: TranslationKey;
    items: Shortcut[];
  }

  const groups: ShortcutGroup[] = [
    {
      id: "navigation",
      label: "shortcuts.groupNavigation",
      items: [
        { keys: [mod, "K"], label: "shortcuts.commandPalette" },
        { keys: [mod, "N"], label: "shortcuts.newChat" },
        { keys: [mod, "B"], label: "shortcuts.toggleSidebar" },
        { keys: [mod, ","], label: "shortcuts.settings" },
      ],
    },
    {
      id: "chat",
      label: "shortcuts.groupChat",
      items: [
        { keys: ["Alt", "↑"], label: "shortcuts.prevMessage" },
        { keys: ["Alt", "↓"], label: "shortcuts.nextMessage" },
        { keys: ["Enter"], label: "shortcuts.send" },
        { keys: ["Shift", "Enter"], label: "shortcuts.newline" },
      ],
    },
    {
      id: "workspace",
      label: "shortcuts.groupWorkspace",
      items: [
        { keys: [mod, "Shift", "B"], label: "shortcuts.browser" },
        { keys: ["Shift", "?"], label: "shortcuts.help" },
      ],
    },
  ];

  onMount(() => {
    const handler = (event: KeyboardEvent) => {
      if (!open) return;
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  });

  $effect(() => {
    if (open) void tick().then(() => sheetEl?.focus());
  });
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-[60] flex items-center justify-center bg-[var(--scrim)] p-4"
    role="dialog"
    aria-modal="true"
    aria-labelledby="shortcut-sheet-title"
    tabindex="-1"
    bind:this={sheetEl}
    onclick={(event) => {
      if (event.target === event.currentTarget) onClose();
    }}
  >
    <div class="w-full max-w-lg rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-5 shadow-2xl">
      <header class="flex items-start justify-between gap-3">
        <div class="min-w-0">
          <h2 id="shortcut-sheet-title" class="flex items-center gap-2 text-sm font-bold text-[var(--text-primary)]">
            <Keyboard class="size-4 text-[var(--brand-text)]" />
            {t("shortcuts.title")}
          </h2>
          <p class="mt-1 text-xs text-[var(--text-tertiary)]">{t("shortcuts.subtitle")}</p>
        </div>
        <button
          type="button"
          class="flex size-7 shrink-0 items-center justify-center rounded-lg text-[var(--text-tertiary)] hover:bg-[var(--surface-3)] hover:text-[var(--text-primary)] cursor-pointer"
          aria-label={t("ui.close")}
          onclick={onClose}
        >
          <X class="size-4" />
        </button>
      </header>

      <div class="mt-4 grid gap-4 sm:grid-cols-2">
        {#each groups as group (group.id)}
          <section aria-labelledby={`shortcut-group-${group.id}`}>
            <h3 id={`shortcut-group-${group.id}`} class="text-[11px] font-bold uppercase tracking-wider text-[var(--text-muted)]">
              {t(group.label)}
            </h3>
            <dl class="mt-1.5 space-y-1">
              {#each group.items as item (item.label)}
                <div class="flex items-center justify-between gap-3 rounded-lg px-1 py-1">
                  <dt class="min-w-0 truncate text-xs text-[var(--text-secondary)]">{t(item.label)}</dt>
                  <dd class="flex shrink-0 items-center gap-1">
                    {#each item.keys as key (key)}
                      <kbd class="rounded-md border border-[var(--hairline)] bg-[var(--surface-2)] px-1.5 py-0.5 font-mono text-[11px] text-[var(--text-secondary)]">{key}</kbd>
                    {/each}
                  </dd>
                </div>
              {/each}
            </dl>
          </section>
        {/each}
      </div>

      <footer class="mt-5 flex justify-end">
        <button
          type="button"
          class="inline-flex h-8 items-center rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-3 text-xs font-semibold text-[var(--text-secondary)] hover:text-[var(--text-primary)] cursor-pointer"
          onclick={onClose}
        >
          {t("ui.close")}
        </button>
      </footer>
    </div>
  </div>
{/if}
