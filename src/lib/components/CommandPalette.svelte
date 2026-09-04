<script lang="ts">
  import { cn } from "$lib/utils.js";
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
      description: bot.description || `Switch to ${bot.name} workspace`,
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
      label: "Connectors Command Center",
      description: "Manage 135+ MCP tools, API plugins & multi-agent connector assignments",
      icon: Layers,
      action: () => {
        window.dispatchEvent(new CustomEvent("open-connectors"));
        onClose();
      },
    },
    {
      id: "create-bot",
      category: "Actions" as const,
      label: "Create New Bot",
      description: "Provision a new sovereign AI agent",
      icon: Plus,
      action: () => {
        onCreateBot();
        onClose();
      },
    },
    {
      id: "settings",
      category: "System" as const,
      label: "System Settings",
      description: "Configure model providers, API keys & preferences",
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
    class="fixed inset-0 z-50 bg-black/70 backdrop-blur-md flex items-start justify-center pt-[15vh] p-4 transition-all"
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
      class="w-full max-w-xl bg-[#0c0c10]/98 border border-white/10 rounded-2xl shadow-2xl overflow-hidden flex flex-col transition-all duration-200 animate-in fade-in zoom-in-95 backdrop-blur-2xl"
      onclick={(e) => e.stopPropagation()}
    >
      <!-- Search Bar -->
      <div class="flex items-center px-4 py-3 border-b border-white/10 bg-white/[0.02] gap-3">
        <Search class="size-4 text-zinc-400 shrink-0" />
        <input
          bind:this={inputRef}
          type="text"
          placeholder="Search bots, commands, actions..."
          bind:value={query}
          onkeydown={handleKeydown}
          class="flex-1 bg-transparent text-sm text-zinc-100 placeholder:text-zinc-500 outline-none border-none ring-0 font-sans"
        />
        {#if query}
          <button
            type="button"
            class="text-xs text-zinc-400 hover:text-white px-2 py-0.5 rounded-lg bg-white/5 border border-white/10 transition-colors"
            onclick={() => (query = "")}
          >
            Clear
          </button>
        {/if}
        <span class="px-2 py-0.5 rounded-lg border border-white/10 bg-white/5 text-zinc-400 font-mono text-[10px]">
          ESC
        </span>
      </div>

      <!-- Command List -->
      <div class="max-h-84 overflow-y-auto p-2 space-y-1">
        {#each filteredCommands as cmd, i (cmd.id)}
          {@const isSelected = selectedIndex === i}
          {@const IconComponent = cmd.icon}
          <button
            type="button"
            class={cn(
              "w-full flex items-center justify-between gap-3 px-3 py-2.5 rounded-xl text-left text-xs transition-all group cursor-pointer focus:outline-none",
              isSelected
                ? "bg-white/10 border border-white/20 text-white shadow-sm font-medium"
                : "text-zinc-300 hover:bg-white/5 border border-transparent"
            )}
            onclick={cmd.action}
            onmouseenter={() => (selectedIndex = i)}
          >
            <div class="flex items-center gap-3 min-w-0">
              <div
                class={cn(
                  "size-8 rounded-lg flex items-center justify-center shrink-0 transition-colors",
                  isSelected
                    ? "bg-white/20 border border-white/30 text-white"
                    : "bg-white/5 border border-white/10 text-zinc-400 group-hover:text-zinc-200"
                )}
              >
                <IconComponent class="size-4" />
              </div>
              <div class="flex flex-col min-w-0">
                <div class="flex items-center gap-2">
                  <span class="font-bold text-xs truncate text-white">{cmd.label}</span>
                  {#if cmd.badge}
                    <span
                      class={cn(
                        "text-[9px] px-1.5 py-0.2 rounded font-mono uppercase border",
                        isSelected
                          ? "bg-white/20 border-white/30 text-white"
                          : "bg-white/5 border-white/10 text-zinc-400"
                      )}
                    >
                      {cmd.badge}
                    </span>
                  {/if}
                </div>
                <span
                  class={cn(
                    "text-[11px] truncate mt-0.5",
                    isSelected ? "text-zinc-300" : "text-zinc-500"
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
                  isSelected ? "opacity-100 text-purple-300 drop-shadow-[0_0_6px_#a855f7]" : "opacity-0 text-zinc-500"
                )}
              />
            </div>
          </button>
        {:else}
          <div class="py-12 px-4 text-center">
            <Bot class="size-8 text-purple-400/40 mx-auto mb-2" />
            <p class="text-sm font-semibold text-zinc-200">No matching commands</p>
            <p class="text-xs text-zinc-500 mt-0.5">Try searching with another keyword</p>
          </div>
        {/each}
      </div>

      <!-- Footer Info Bar -->
      <div class="px-4 py-2.5 bg-[#08080d]/90 border-t border-purple-500/15 flex items-center justify-between text-[11px] text-zinc-400">
        <div class="flex items-center gap-4">
          <span class="flex items-center gap-1">
            <kbd class="px-1.5 py-0.5 rounded bg-[#12121c] border border-purple-500/20 text-purple-300 text-[10px] font-mono">↑</kbd>
            <kbd class="px-1.5 py-0.5 rounded bg-[#12121c] border border-purple-500/20 text-purple-300 text-[10px] font-mono">↓</kbd>
            Navigate
          </span>
          <span class="flex items-center gap-1">
            <kbd class="px-1.5 py-0.5 rounded bg-[#12121c] border border-purple-500/20 text-purple-300 text-[10px] font-mono">↵</kbd>
            Select
          </span>
          <span class="flex items-center gap-1">
            <kbd class="px-1.5 py-0.5 rounded bg-[#12121c] border border-purple-500/20 text-purple-300 text-[10px] font-mono">esc</kbd>
            Dismiss
          </span>
        </div>
        <div class="flex items-center gap-1.5 font-mono text-[10px] text-purple-400/80">
          <Zap class="size-3 text-purple-400" />
          <span>RAVENBOT Core</span>
        </div>
      </div>
    </div>
  </div>
{/if}
