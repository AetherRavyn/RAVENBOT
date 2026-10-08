<script lang="ts">
  import { cn } from "$lib/utils.js";
  import { t } from "$lib/i18n";
  import {
    Search,
    Bot,
    Plus,
    Settings as SettingsIcon,
    Command,
    CornerDownLeft,
    Zap,
    Layers,
  } from "@lucide/svelte";

  interface Props {
    open: boolean;
    onClose: () => void;
    bots: any[];
    onSelectBot: (id: string) => void;
    onCreateBot: () => void;
    onOpenSettings: () => void;
  }

  let { open, onClose, bots = [], onSelectBot, onCreateBot, onOpenSettings }: Props = $props();

  let query = $state("");
  let selectedIndex = $state(0);
  let inputRef = $state<HTMLInputElement | null>(null);

  interface CommandItem {
    id: string;
    category: "Bots" | "Actions" | "System" | "Tools & Integrations";
    label: string;
    description: string;
    icon: any;
    badge?: string;
    action: () => void;
  }

  let commands = $derived<CommandItem[]>([
    ...bots.map((bot) => ({
      id: `bot-${bot.id}`,
      category: "Bots" as const,
      label: bot.name,
      description: t("palette.switchTo", { name: bot.name }),
      icon: Bot,
      badge: bot.config?.model_provider || "OPENROUTER",
      action: () => {
        onSelectBot(bot.id);
        onClose();
      },
    })),
    {
      id: "connectors-hub",
      category: "Tools & Integrations" as const,
      label: t("palette.connectors"),
      description: t("palette.connectorsDesc"),
      icon: Layers,
      action: () => {
        window.dispatchEvent(new CustomEvent("open-connectors"));
        onClose();
      },
    },
    {
      id: "create-bot",
      category: "Actions" as const,
      label: t("palette.createBot"),
      description: t("palette.createBotDesc"),
      icon: Plus,
      action: () => {
        onCreateBot();
        onClose();
      },
    },
    {
      id: "settings",
      category: "System" as const,
      label: t("palette.settings"),
      description: t("palette.settingsDesc"),
      icon: SettingsIcon,
      action: () => {
        onOpenSettings();
        onClose();
      },
    },
  ]);

  let filteredCommands = $derived(
    commands.filter(
      (cmd) =>
        cmd.label.toLowerCase().includes(query.toLowerCase()) ||
        cmd.description.toLowerCase().includes(query.toLowerCase()) ||
        cmd.category.toLowerCase().includes(query.toLowerCase())
    )
  );

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      selectedIndex = Math.min(selectedIndex + 1, Math.max(filteredCommands.length - 1, 0));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      selectedIndex = Math.max(selectedIndex - 1, 0);
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (filteredCommands[selectedIndex]) {
        filteredCommands[selectedIndex].action();
      }
    } else if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  }

  $effect(() => {
    query;
    selectedIndex = 0;
  });

  $effect(() => {
    if (open) {
      setTimeout(() => {
        inputRef?.focus();
      }, 50);
    }
  });
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-50 bg-black/60  flex items-start justify-center pt-[15vh] p-4 transition-all"
    onclick={onClose}
    onkeydown={handleKeydown}
    role="dialog"
    aria-modal="true"
    tabindex="-1"
  >
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      role="document"
      class="w-full max-w-xl bg-[var(--surface-1)] border border-[var(--hairline)] rounded-2xl shadow-2xl overflow-hidden flex flex-col transition-all duration-200 animate-in fade-in zoom-in-95 "
      onclick={(e) => e.stopPropagation()}
    >
      <!-- Search Bar -->
      <div class="flex items-center px-4 py-3 border-b border-[var(--hairline)] bg-[var(--surface-1)] gap-3">
        <Search class="size-4 text-[var(--text-tertiary)] shrink-0" />
        <input
          bind:this={inputRef}
          type="text"
          placeholder={t("palette.search")}
          aria-label={t("palette.search")}
          aria-controls="palette-results"
          aria-activedescendant={filteredCommands[selectedIndex] ? `palette-result-${filteredCommands[selectedIndex].id}` : undefined}
          bind:value={query}
          onkeydown={handleKeydown}
          class="flex-1 bg-transparent text-sm text-[var(--text-primary)] placeholder:text-[var(--text-muted)] outline-none border-none ring-0 font-sans"
        />
        {#if query}
          <button
            type="button"
            class="text-xs text-[var(--text-tertiary)] hover:text-[var(--text-primary)] px-2 py-0.5 rounded-lg bg-[var(--surface-2)] border border-[var(--hairline)] transition-colors"
            onclick={() => (query = "")}
          >
            {t("palette.clear")}
          </button>
        {/if}
        <span class="px-2 py-0.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] text-[var(--text-tertiary)] font-mono text-[11px]">
          ESC
        </span>
      </div>

      <!-- Command List -->
      <div id="palette-results" role="listbox" class="max-h-84 overflow-y-auto p-2 space-y-1">
        {#each filteredCommands as cmd, i (cmd.id)}
          {@const isSelected = selectedIndex === i}
          {@const IconComponent = cmd.icon}
          <button
            type="button"
            id="palette-result-{cmd.id}"
            role="option"
            aria-selected={isSelected}
            class={cn(
              "w-full flex items-center justify-between gap-3 px-3 py-2.5 rounded-xl text-left text-xs transition-all group cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--brand)]/60",
              isSelected
                ? "bg-[var(--surface-3)] border border-[var(--hairline-strong)] text-[var(--text-primary)] shadow-sm font-medium"
                : "text-[var(--text-secondary)] hover:bg-[var(--surface-3)] border border-transparent"
            )}
            onclick={cmd.action}
            onmouseenter={() => (selectedIndex = i)}
          >
            <div class="flex items-center gap-3 min-w-0">
              <div
                class={cn(
                  "size-8 rounded-lg flex items-center justify-center shrink-0 transition-colors",
                  isSelected
                    ? "bg-[var(--surface-3)] border border-[var(--hairline-strong)] text-[var(--text-primary)]"
                    : "bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-tertiary)] group-hover:text-[var(--text-primary)]"
                )}
              >
                <IconComponent class="size-4" />
              </div>
              <div class="flex flex-col min-w-0">
                <div class="flex items-center gap-2">
                  <span class="font-bold text-xs truncate text-[var(--text-primary)]">{cmd.label}</span>
                  {#if cmd.badge}
                    <span
                      class={cn(
                        "text-[11px] px-1.5 py-[2px] rounded font-mono uppercase border",
                        isSelected
                          ? "bg-[var(--surface-3)] border-[var(--hairline-strong)] text-[var(--text-primary)]"
                          : "bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-tertiary)]"
                      )}
                    >
                      {cmd.badge}
                    </span>
                  {/if}
                </div>
                <span
                  class={cn(
                    "text-[11px] truncate mt-0.5",
                    isSelected ? "text-[var(--text-secondary)]" : "text-[var(--text-muted)]"
                  )}
                >
                  {cmd.description}
                </span>
              </div>
            </div>

            <div class="flex items-center shrink-0">
              <CornerDownLeft
                class={cn(
                  "size-4 transition-all",
                  isSelected ? "opacity-100 text-[var(--brand-text)] " : "opacity-0 text-[var(--text-muted)]"
                )}
              />
            </div>
          </button>
        {:else}
          <div class="py-12 px-4 text-center">
            <Bot class="size-8 text-[var(--brand-text)]/40 mx-auto mb-2" />
            <p class="text-sm font-semibold text-[var(--text-secondary)]">{t("palette.noMatches")}</p>
            <p class="text-xs text-[var(--text-muted)] mt-0.5">{t("palette.noMatchesHint")}</p>
          </div>
        {/each}
      </div>

      <!-- Footer Info Bar -->
      <div class="px-4 py-2.5 bg-[var(--surface-0)] border-t border-[var(--hairline)] flex items-center justify-between text-[11px] text-[var(--text-tertiary)]">
        <div class="flex items-center gap-4">
          <span class="flex items-center gap-1">
            <kbd class="px-1.5 py-0.5 rounded bg-[var(--surface-2)] border border-[var(--brand)]/25 text-[var(--brand-text)] text-[11px] font-mono">↑</kbd>
            <kbd class="px-1.5 py-0.5 rounded bg-[var(--surface-2)] border border-[var(--brand)]/25 text-[var(--brand-text)] text-[11px] font-mono">↓</kbd>
            {t("palette.navigate")}
          </span>
          <span class="flex items-center gap-1">
            <kbd class="px-1.5 py-0.5 rounded bg-[var(--surface-2)] border border-[var(--brand)]/25 text-[var(--brand-text)] text-[11px] font-mono">↵</kbd>
            {t("palette.select")}
          </span>
          <span class="flex items-center gap-1">
            <kbd class="px-1.5 py-0.5 rounded bg-[var(--surface-2)] border border-[var(--brand)]/25 text-[var(--brand-text)] text-[11px] font-mono">esc</kbd>
            {t("palette.dismiss")}
          </span>
        </div>
        <div class="flex items-center gap-1.5 font-mono text-[11px] text-[var(--brand-text)]/80">
          <Zap class="size-3 text-[var(--brand-text)]" />
          <span>RAVENBOT Core</span>
        </div>
      </div>
    </div>
  </div>
{/if}
