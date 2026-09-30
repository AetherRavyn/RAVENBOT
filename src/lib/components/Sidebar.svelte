<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getDiceBearUrl } from "$lib/utils";
  import { cn } from "$lib/utils.js";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Badge } from "$lib/components/ui/badge";
  import * as Dialog from "$lib/components/ui/dialog";
  import * as Avatar from "$lib/components/ui/avatar";
  import * as Tabs from "$lib/components/ui/tabs";
  import { Label } from "$lib/components/ui/label";
  import { Textarea } from "$lib/components/ui/textarea";
  import AvatarPicker from "$lib/components/AvatarPicker.svelte";
  import {
    Bot,
    Plus,
    Settings as SettingsIcon,
    Search,
    Clock,
    AlertCircle,
    Crown,
    Server,
    Pause,
    MoreVertical,
    Wrench,
    Trash2,
    Sliders,
    Sparkles,
    Check,
    Circle,
    Palette,
    UserCheck,
    Layers,
  } from "@lucide/svelte";

  interface Props {
    bots: any[];
    selectedBotId: string | null;
    onSelectBot: (id: string) => void;
    onBotCreated: (bot: any) => void;
    onBotUpdated: (bot: any) => void;
    onBotDeleted: (botId: string) => void;
    openSettings: () => void;
    onNewChat?: () => void;
  }

  let {
    bots = [],
    selectedBotId,
    onSelectBot,
    onBotCreated,
    onBotUpdated,
    onBotDeleted,
    openSettings,
    onNewChat,
  }: Props = $props();

  let showCreateModal = $state(false);
  let createModalTab = $state("profile");
  let newBotName = $state("");
  let newBotDescription = $state("");
  let newBotAvatarUrl = $state<string | null>(null);
  let newBotAvatarStyle = $state("bottts");

  let searchQuery = $state("");
  let showOnlyWaiting = $state(false);
  let selectedBotForSettings = $state<any>(null);
  let showBotSettings = $state(false);
  let selectedBotForSkills = $state<any>(null);
  let showSkillManager = $state(false);
  let selectedBotForMcp = $state<any>(null);
  let showMcpManager = $state(false);
  let isCreating = $state(false);
  let activeActionMenuBotId = $state<string | null>(null);

  let filteredBots = $derived(
    bots.filter((bot) => {
      const matchesSearch =
        bot.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
        (bot.description && bot.description.toLowerCase().includes(searchQuery.toLowerCase()));
      const matchesFilter = !showOnlyWaiting || bot.status === "waiting_on_user";
      return matchesSearch && matchesFilter;
    })
  );

  let effectiveAvatarUrl = $derived(
    newBotAvatarUrl || getDiceBearUrl(newBotName || "Agent", newBotAvatarStyle)
  );

  async function createBot() {
    if (!newBotName.trim()) return;
    isCreating = true;
    try {
      const bot = await invoke("create_bot", {
        name: newBotName,
        description: newBotDescription,
        avatarUrl: newBotAvatarUrl || effectiveAvatarUrl,
        avatarStyle: newBotAvatarStyle,
      });
      onBotCreated(bot);
      showCreateModal = false;
      newBotName = "";
      newBotDescription = "";
      newBotAvatarUrl = null;
      newBotAvatarStyle = "bottts";
      createModalTab = "profile";
    } catch (e) {
      console.error("Failed to create bot:", e);
    } finally {
      isCreating = false;
    }
  }

  function getStatusTheme(status: string) {
    switch (status) {
      case "idle":
        return { bg: "bg-emerald-500", text: "text-zinc-400", label: "Idle" };
      case "thinking":
        return { bg: "bg-amber-400", text: "text-amber-400", label: "Thinking..." };
      case "running_tool":
        return { bg: "bg-blue-400", text: "text-blue-400", label: "Running Tool" };
      case "waiting_on_user":
        return { bg: "bg-red-500", text: "text-red-400", label: "Waiting on you" };
      case "paused":
        return { bg: "bg-purple-400", text: "text-purple-400", label: "Paused" };
      default:
        return { bg: "bg-emerald-500", text: "text-zinc-400", label: status };
    }
  }
</script>

<svelte:window onclick={() => (activeActionMenuBotId = null)} />

<div class="flex flex-col h-full overflow-hidden select-none">
  <!-- New Chat Action Button (Grok Signature) -->
  <div class="p-3 pb-1.5">
    <button
      type="button"
      class="w-full flex items-center justify-between px-3.5 py-2.5 rounded-xl bg-white/10 hover:bg-white/15 text-white font-semibold text-xs border border-white/15 shadow-sm transition-all cursor-pointer group"
      onclick={() => {
        if (onNewChat) onNewChat();
        else onSelectBot("");
      }}
    >
      <div class="flex items-center gap-2">
        <Plus class="size-4 text-sky-400 group-hover:rotate-90 transition-transform" />
        <span>New Chat</span>
      </div>
      <span class="text-[10px] font-mono text-zinc-400 bg-white/10 px-1.5 py-0.5 rounded">⌘N</span>
    </button>
  </div>

  <!-- Section Header: FLEET AGENTS + Actions -->
  <div class="px-3 pt-2 pb-1.5 flex items-center justify-between">
    <div class="flex items-center gap-2">
      <span class="font-bold text-[10px] tracking-wider uppercase text-zinc-400 font-mono">Fleet Agents</span>
      <span class="bg-white/5 text-zinc-400 text-[10px] font-mono font-medium px-2 py-0.5 rounded-full border border-white/10">
        {bots.length}
      </span>
    </div>

    <div class="flex items-center gap-1">
      <button
        type="button"
        class="size-6.5 rounded-lg border border-white/10 bg-white/5 flex items-center justify-center text-zinc-400 hover:text-white hover:border-zinc-500 transition-colors cursor-pointer"
        onclick={() => {
          showCreateModal = true;
          createModalTab = "profile";
        }}
        title="Create new AI agent"
      >
        <Plus class="size-3.5" />
      </button>
      <button
        type="button"
        class="size-6.5 rounded-lg border border-white/10 bg-white/5 flex items-center justify-center text-zinc-400 hover:text-white hover:border-zinc-500 transition-colors cursor-pointer"
        onclick={openSettings}
        title="Settings"
      >
        <SettingsIcon class="size-3.5" />
      </button>
    </div>
  </div>

  <!-- Search & Filter Controls -->
  <div class="px-3 py-1 space-y-1.5">
    <div class="relative">
      <Search class="size-3.5 absolute left-3 top-1/2 -translate-y-1/2 text-zinc-500 pointer-events-none" />
      <input
        type="text"
        placeholder="Filter fleet..."
        bind:value={searchQuery}
        class="w-full h-8 pl-8 pr-3 bg-[#0d0d12] border border-white/10 rounded-xl text-xs text-zinc-200 placeholder:text-zinc-500 focus:outline-none focus:border-zinc-500 transition-colors font-sans"
      />
    </div>

    <!-- Waiting on me filter toggle -->
    {#if bots.some((b) => b.status === "waiting_on_user")}
      <button
        type="button"
        class="flex items-center gap-2 text-xs text-zinc-400 hover:text-zinc-200 px-1 py-1 transition-colors cursor-pointer"
        onclick={() => (showOnlyWaiting = !showOnlyWaiting)}
      >
        <div class="size-3.5 rounded-full border border-zinc-600 flex items-center justify-center {showOnlyWaiting ? 'border-rose-500 bg-rose-500/20' : ''}">
          {#if showOnlyWaiting}
            <div class="size-1.5 rounded-full bg-rose-500"></div>
          {/if}
        </div>
        <span class="text-[11px] font-medium {showOnlyWaiting ? 'text-rose-400' : 'text-zinc-400'}">
          Waiting on me
        </span>
      </button>
    {/if}
  </div>

  <!-- Agent Card List -->
  <div class="flex-1 overflow-y-auto px-3 py-1.5 space-y-1.5">
    {#each filteredBots as bot (bot.id)}
      {@const isSelected = selectedBotId === bot.id}
      {@const statusTheme = getStatusTheme(bot.status)}
      <div class="relative group/item">
        <button
          type="button"
          class={cn(
            "w-full text-left p-2.5 rounded-2xl border transition-all flex items-center gap-3 cursor-pointer focus:outline-none",
            isSelected
              ? "border-white/30 bg-white/10 shadow-[0_0_20px_rgba(255,255,255,0.06)] text-white"
              : "border-white/5 bg-[#0e0e13]/60 hover:border-white/15 hover:bg-[#14141c] text-zinc-300"
          )}
          onclick={() => onSelectBot(bot.id)}
        >
          <!-- Avatar Container -->
          <div class="relative size-10 shrink-0">
            <div class="size-10 rounded-xl overflow-hidden bg-[#14141c] border border-white/10">
              <img
                src={bot.avatar_url || getDiceBearUrl(bot.name, bot.avatar_style || "bottts")}
                alt={bot.name}
                class="size-full object-cover"
                loading="lazy"
              />
            </div>

            <!-- Status Dot Badge -->
            <div
              class={cn(
                "absolute -bottom-0.5 -right-0.5 size-2.5 rounded-full ring-2 ring-black",
                statusTheme.bg
              )}
            ></div>
          </div>

          <!-- Name & Status -->
          <div class="flex-1 min-w-0">
            <div class="flex items-center justify-between">
              <span class="font-bold text-xs text-zinc-100 truncate">{bot.name}</span>
              {#if bot.is_orchestrator}
                <Crown class="size-3 text-amber-400" />
              {/if}
            </div>
            <span class="text-[11px] text-zinc-500 truncate block mt-0.5">
              {statusTheme.label}
            </span>
          </div>
        </button>

        <!-- Quick Action Menu Trigger -->
        <button
          type="button"
          class="absolute right-2 top-2 size-6 rounded-lg bg-white/5 text-zinc-400 hover:text-white opacity-0 group-hover/item:opacity-100 transition-opacity flex items-center justify-center border border-white/10 cursor-pointer"
          onclick={(e) => {
            e.stopPropagation();
            activeActionMenuBotId = activeActionMenuBotId === bot.id ? null : bot.id;
          }}
          title="Agent options"
        >
          <MoreVertical class="size-3.5" />
        </button>

        <!-- Popover Action Menu -->
        {#if activeActionMenuBotId === bot.id}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="absolute right-2 top-8 z-40 w-40 bg-[#0e0e14] border border-white/10 rounded-2xl shadow-2xl p-1.5 space-y-0.5 animate-in fade-in zoom-in-95 text-xs backdrop-blur-xl"
            onclick={(e) => e.stopPropagation()}
          >
            <button
              type="button"
              class="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-xl text-zinc-200 hover:bg-white/10 text-left transition-colors cursor-pointer"
              onclick={() => {
                selectedBotForSettings = bot;
                showBotSettings = true;
                activeActionMenuBotId = null;
              }}
            >
              <Sliders class="size-3.5 text-zinc-400" />
              Settings
            </button>
            <button
              type="button"
              class="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-xl text-zinc-200 hover:bg-white/10 text-left transition-colors cursor-pointer"
              onclick={() => {
                selectedBotForSkills = bot;
                showSkillManager = true;
                activeActionMenuBotId = null;
              }}
            >
              <Wrench class="size-3.5 text-zinc-400" />
              Skills
            </button>
            <button
              type="button"
              class="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-xl text-sky-300 hover:bg-white/10 text-left transition-colors cursor-pointer"
              onclick={() => {
                onSelectBot(bot.id);
                window.dispatchEvent(new CustomEvent("open-connectors"));
                activeActionMenuBotId = null;
              }}
            >
              <Layers class="size-3.5 text-sky-400" />
              Connectors Hub
            </button>
            <button
              type="button"
              class="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-xl text-sky-300 hover:bg-white/10 text-left transition-colors cursor-pointer"
              onclick={() => {
                selectedBotForMcp = bot;
                showMcpManager = true;
                activeActionMenuBotId = null;
              }}
            >
              <Server class="size-3.5 text-sky-400" />
              MCP Tools
            </button>
            <button
              type="button"
              class="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-xl text-rose-400 hover:bg-rose-500/10 text-left transition-colors cursor-pointer"
              onclick={() => {
                activeActionMenuBotId = null;
                selectedBotForSettings = bot;
                showBotSettings = true;
              }}
            >
              <Trash2 class="size-3.5" />
              Delete
            </button>
          </div>
        {/if}
      </div>
    {:else}
      <div class="py-12 px-3 text-center text-zinc-500">
        <Bot class="size-8 mx-auto mb-2 opacity-30 text-zinc-400" />
        <p class="text-xs">No agents found</p>
      </div>
    {/each}
  </div>

  <!-- Bottom Pause Fleet Bar -->
  <div class="p-3 border-t border-white/10 bg-[#09090d]">
    <button
      type="button"
      class="w-full bg-white/5 border border-white/10 hover:border-zinc-500 hover:bg-white/10 rounded-xl py-2 px-3.5 flex items-center justify-between text-xs text-zinc-300 transition-all font-medium cursor-pointer"
      onclick={() => invoke("pause_all")}
    >
      <div class="flex items-center gap-2">
        <Pause class="size-3.5 fill-current text-zinc-400" />
        <span>Pause Fleet</span>
      </div>
      <span class="font-mono text-[10px] text-zinc-500 bg-white/5 px-1.5 py-0.5 rounded">⌘P</span>
    </button>
  </div>
</div>

<!-- Create Bot Dialog (Clean, Responsive 2-Tab Design with Fixed Footer) -->
<Dialog.Root open={showCreateModal} onOpenChange={(o) => (!o && (showCreateModal = false))}>
  <Dialog.Content class="sm:max-w-xl max-h-[85vh] flex flex-col bg-[#0c0c14]/98 border border-purple-500/30 shadow-[0_0_50px_rgba(147,51,234,0.25)] backdrop-blur-2xl rounded-3xl p-0 overflow-hidden">
    <!-- Fixed Dialog Header -->
    <div class="px-6 pt-5 pb-3 border-b border-white/10 shrink-0">
      <Dialog.Header class="gap-1">
        <Dialog.Title class="text-base font-bold flex items-center gap-2 text-white">
          <Bot class="size-5 text-purple-400" />
          Provision New Fleet Agent
        </Dialog.Title>
        <Dialog.Description class="text-xs text-zinc-400">
          Configure agent identity, mission, and customizable DiceBear look.
        </Dialog.Description>
      </Dialog.Header>

      <!-- Sub-tabs: Profile vs Avatar Picker -->
      <div class="grid grid-cols-2 bg-[#12121e] border border-[#232336] p-1 rounded-xl mt-3">
        <button
          type="button"
          class="flex items-center justify-center gap-1.5 py-1 px-3 rounded-lg text-xs font-medium transition-all cursor-pointer {createModalTab === 'profile'
            ? 'bg-purple-600 text-white shadow-sm'
            : 'text-zinc-400 hover:text-zinc-200'}"
          onclick={() => (createModalTab = "profile")}
        >
          <UserCheck class="size-3.5" />
          <span>Agent Details</span>
        </button>

        <button
          type="button"
          class="flex items-center justify-center gap-1.5 py-1 px-3 rounded-lg text-xs font-medium transition-all cursor-pointer {createModalTab === 'avatar'
            ? 'bg-purple-600 text-white shadow-sm'
            : 'text-zinc-400 hover:text-zinc-200'}"
          onclick={() => (createModalTab = "avatar")}
        >
          <Palette class="size-3.5" />
          <span>Choose Avatar ({newBotAvatarStyle})</span>
        </button>
      </div>
    </div>

    <!-- Scrollable Dialog Body (Guarantees no modal overflow) -->
    <form onsubmit={(e) => { e.preventDefault(); createBot(); }} class="flex-1 flex flex-col overflow-hidden">
      <div class="flex-1 overflow-y-auto px-6 py-4 space-y-4">
        {#if createModalTab === "profile"}
          <!-- Live Avatar Preview Card -->
          <div class="p-3.5 rounded-2xl bg-[#11111d] border border-purple-500/25 flex items-center justify-between shadow-inner">
            <div class="flex items-center gap-3.5">
              <div class="size-14 rounded-2xl overflow-hidden bg-[#181826] border-2 border-purple-500/50 p-0.5 shadow-md shrink-0">
                <img
                  src={effectiveAvatarUrl}
                  alt="Agent Avatar"
                  class="size-full rounded-xl object-cover"
                />
              </div>
              <div class="flex flex-col">
                <span class="text-sm font-bold text-white">
                  {newBotName || "New Agent"}
                </span>
                <span class="text-xs text-purple-300 capitalize font-mono mt-0.5">
                  Style: {newBotAvatarStyle}
                </span>
              </div>
            </div>

            <Button
              type="button"
              variant="outline"
              size="sm"
              class="h-8 gap-1.5 text-xs bg-[#171726] border-purple-500/30 text-purple-300 hover:bg-purple-950/40 hover:text-white"
              onclick={() => (createModalTab = "avatar")}
            >
              <Palette class="size-3.5 text-purple-400" />
              Customize
            </Button>
          </div>

          <div class="space-y-1.5">
            <Label for="new-bot-name" class="text-xs font-semibold uppercase tracking-wider text-zinc-400">
              Agent Name
            </Label>
            <Input
              id="new-bot-name"
              type="text"
              bind:value={newBotName}
              placeholder="e.g. Bro, Chief of Staff, Architect, Sentinel..."
              class="h-9 text-xs bg-[#141420] border-[#252538] text-white"
              required
            />
          </div>

          <div class="space-y-1.5">
            <Label for="new-bot-desc" class="text-xs font-semibold uppercase tracking-wider text-zinc-400">
              Specialization & Mission
            </Label>
            <Textarea
              id="new-bot-desc"
              bind:value={newBotDescription}
              placeholder="What tasks, workflows, or roles does this agent handle?"
              rows={3}
              class="text-xs bg-[#141420] border-[#252538] text-zinc-200 resize-none"
            />
          </div>
        {:else}
          <!-- Avatar Picker View inside tab -->
          <div class="space-y-2">
            <AvatarPicker
              seed={newBotName || "Agent"}
              style={newBotAvatarStyle}
              customUrl={newBotAvatarUrl}
              onSelect={(url, style) => {
                newBotAvatarUrl = url;
                newBotAvatarStyle = style;
                createModalTab = "profile";
              }}
            />
          </div>
        {/if}
      </div>

      <!-- Always Fixed Pinned Footer (Never gets pushed out of view) -->
      <div class="px-6 py-3.5 border-t border-white/10 bg-[#0a0a12] flex items-center justify-end gap-2 shrink-0">
        <Button variant="outline" size="sm" type="button" onclick={() => (showCreateModal = false)}>
          Cancel
        </Button>
        <Button
          size="sm"
          type="submit"
          class="gap-1.5 bg-purple-600 hover:bg-purple-500 text-white font-medium shadow-md shadow-purple-950/50"
          disabled={!newBotName.trim() || isCreating}
        >
          <Plus class="size-3.5" />
          {isCreating ? "Provisioning..." : "Create Agent"}
        </Button>
      </div>
    </form>
  </Dialog.Content>
</Dialog.Root>

<!-- Bot Settings Modal -->
{#if showBotSettings && selectedBotForSettings}
  {#await import("$lib/components/BotSettings.svelte") then BotSettings}
    <BotSettings.default
      bot={selectedBotForSettings}
      open={showBotSettings}
      onClose={() => { showBotSettings = false; selectedBotForSettings = null; }}
      onUpdated={(updatedBot) => {
        if (updatedBot === null) {
          onBotDeleted(selectedBotForSettings.id);
        } else {
          onBotUpdated(updatedBot);
        }
        showBotSettings = false;
        selectedBotForSettings = null;
      }}
    />
  {/await}
{/if}

<!-- Skill Manager Modal -->
{#if showSkillManager && selectedBotForSkills}
  {#await import("$lib/components/SkillManager.svelte") then SkillManager}
    <SkillManager.default
      bot={selectedBotForSkills}
      open={showSkillManager}
      onClose={() => { showSkillManager = false; selectedBotForSkills = null; }}
      onUpdated={(updatedBot) => {
        if (updatedBot) {
          onBotUpdated(updatedBot);
        }
        showSkillManager = false;
        selectedBotForSkills = null;
      }}
    />
  {/await}
{/if}

<!-- MCP Manager Modal -->
{#if showMcpManager && selectedBotForMcp}
  {#await import("$lib/components/McpManager.svelte") then McpManager}
    <McpManager.default
      bot={selectedBotForMcp}
      open={showMcpManager}
      onClose={() => {
        showMcpManager = false;
        selectedBotForMcp = null;
      }}
    />
  {/await}
{/if}
