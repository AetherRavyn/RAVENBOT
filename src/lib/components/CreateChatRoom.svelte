<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import * as Dialog from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import { Textarea } from "$lib/components/ui/textarea";
  import AvatarPicker from "$lib/components/AvatarPicker.svelte";
  import { getDiceBearUrl, OFFICE_TEMPLATES, type OfficeTemplateKey } from "$lib/utils";
  import { cn } from "$lib/utils.js";
  import {
    Building2,
    Sparkles,
    Users,
    Laptop,
    TrendingUp,
    Briefcase,
    Palette,
    Plus,
    CheckCircle2,
    Wand2,
  } from "@lucide/svelte";

  interface Props {
    open: boolean;
    onClose: () => void;
    onCreated: (room: any) => void;
    bots: any[];
  }

  let { open, onClose, onCreated, bots = [] }: Props = $props();

  let name = $state("");
  let description = $state("");
  let officeTemplate: OfficeTemplateKey = $state("it-office");
  let roomAvatarUrl = $state("");
  let roomAvatarStyle = $state("bottts");
  let showAvatarPicker = $state(false);
  let selectedMembers: { botId: string; rank: string; specialty: string }[] = $state([]);
  let isCreating = $state(false);

  const templateIcons: Record<string, any> = {
    "it-office": Laptop,
    "marketing": TrendingUp,
    "sales": Briefcase,
    "design": Palette,
    "custom": Building2,
  };

  let templates = Object.entries(OFFICE_TEMPLATES);
  let currentTemplate = $derived(OFFICE_TEMPLATES[officeTemplate]);

  function toggleMember(bot: any, rank: string, specialty: string) {
    const idx = selectedMembers.findIndex((m) => m.botId === bot.id);
    if (idx >= 0) selectedMembers.splice(idx, 1);
    else selectedMembers.push({ botId: bot.id, rank, specialty });
    selectedMembers = [...selectedMembers];
  }

  function autoAssign() {
    selectedMembers = [];
    const ranks = currentTemplate.ranks;
    if (!ranks.length) return;
    bots.slice(0, ranks.length).forEach((bot, i) => {
      const r = ranks[i % ranks.length];
      selectedMembers.push({ botId: bot.id, rank: r.rank, specialty: r.specialty });
    });
    selectedMembers = [...selectedMembers];
  }

  async function create() {
    if (!name.trim()) return;
    isCreating = true;
    try {
      const url = roomAvatarUrl || getDiceBearUrl(name, roomAvatarStyle);
      const room = await invoke("create_chatroom", {
        name,
        description,
        officeTemplate,
        avatarUrl: url,
        avatarStyle: roomAvatarStyle,
      });

      for (const m of selectedMembers) {
        await invoke("add_member_to_chatroom", {
          chatroomId: (room as any).id,
          botId: m.botId,
          rank: m.rank,
          specialty: m.specialty,
        });
      }

      onCreated(room);
      onClose();
      name = "";
      description = "";
      selectedMembers = [];
    } catch (e) {
      console.error("Failed to create office:", e);
    } finally {
      isCreating = false;
    }
  }

  let roomPreviewUrl = $derived(roomAvatarUrl || getDiceBearUrl(name || "office", roomAvatarStyle));
</script>

<Dialog.Root {open} onOpenChange={(o) => !o && onClose()}>
  <Dialog.Content class="sm:max-w-3xl max-h-[88vh] overflow-y-auto bg-[#0c0c14]/95 border border-purple-500/30 shadow-[0_0_50px_rgba(147,51,234,0.25)] backdrop-blur-2xl rounded-3xl">
    <Dialog.Header class="pb-3 border-b border-purple-500/15">
      <Dialog.Title class="text-base font-bold flex items-center gap-2.5 text-white">
        <div class="size-8 rounded-xl bg-purple-950/60 border border-purple-800/50 flex items-center justify-center text-purple-400">
          <Building2 class="size-4.5" />
        </div>
        Create Office Workspace
      </Dialog.Title>
      <Dialog.Description class="text-xs text-zinc-400">
        Form a specialized multi-bot collaborative office with ranked roles, automated task distribution, and shared thread lanes.
      </Dialog.Description>
    </Dialog.Header>

    <div class="grid gap-5 py-3">
      <!-- Room Identity Card -->
      <div class="p-4 rounded-2xl border border-[#1e1e2d] bg-[#0f0f18]/80 flex flex-col md:flex-row gap-4 items-start">
        <button
          type="button"
          onclick={() => (showAvatarPicker = !showAvatarPicker)}
          class="flex flex-col items-center gap-1.5 shrink-0 group focus:outline-none cursor-pointer"
        >
          <div class="size-16 rounded-full overflow-hidden bg-[#181826] border-2 border-purple-500/40 p-0.5 shadow-[0_0_20px_rgba(147,51,234,0.2)] group-hover:scale-105 transition-transform">
            <img src={roomPreviewUrl} alt={name || "Office"} class="size-full rounded-full object-cover" />
          </div>
          <span class="text-[11px] text-purple-400 flex items-center gap-1 group-hover:underline font-medium">
            <Sparkles class="size-3" />
            Change Icon
          </span>
        </button>

        <div class="flex-1 space-y-3 w-full">
          <div class="space-y-1">
            <Label for="room-name" class="text-xs font-bold uppercase tracking-wider text-zinc-400">
              Office Name
            </Label>
            <Input
              id="room-name"
              bind:value={name}
              placeholder="e.g. Core Engineering, Growth Pod, Product Studio..."
              class="h-9 text-xs bg-[#141420] border-[#252538] text-zinc-200"
            />
          </div>
          <div class="space-y-1">
            <Label for="room-desc" class="text-xs font-bold uppercase tracking-wider text-zinc-400">
              Office Mission
            </Label>
            <Textarea
              id="room-desc"
              bind:value={description}
              placeholder="What does this office collaborate on?"
              rows={2}
              class="text-xs bg-[#141420] border-[#252538] text-zinc-200 resize-none"
            />
          </div>
        </div>
      </div>

      {#if showAvatarPicker}
        <div class="p-4 rounded-2xl border border-purple-500/40 bg-[#0e0e18] shadow-2xl">
          <div class="flex items-center justify-between mb-2">
            <span class="text-xs font-bold text-white">Customize Office Avatar</span>
            <Button variant="ghost" size="xs" onclick={() => (showAvatarPicker = false)}>Done</Button>
          </div>
          <AvatarPicker
            seed={name || "office"}
            style={roomAvatarStyle}
            customUrl={roomAvatarUrl}
            onSelect={(url, style) => {
              roomAvatarUrl = url;
              roomAvatarStyle = style;
              showAvatarPicker = false;
            }}
          />
        </div>
      {/if}

      <!-- Template Selection -->
      <div class="space-y-2">
        <Label class="text-xs font-bold uppercase tracking-wider text-zinc-400">
          Office Template & Organization
        </Label>
        <div class="grid grid-cols-2 md:grid-cols-3 gap-2.5">
          {#each templates as [key, tmpl]}
            {@const IconComponent = templateIcons[key] || Building2}
            {@const isSelected = officeTemplate === key}
            <button
              type="button"
              onclick={() => (officeTemplate = key as OfficeTemplateKey)}
              class={cn(
                "text-left rounded-2xl border p-3 transition-all flex flex-col justify-between focus:outline-none cursor-pointer",
                isSelected
                  ? "border-purple-500 bg-purple-950/40 shadow-[0_0_20px_rgba(147,51,234,0.2)] ring-1 ring-purple-500/60"
                  : "border-[#1e1e2d] bg-[#0d0d16] hover:border-purple-500/40 hover:bg-[#131320]"
              )}
            >
              <div>
                <div class="flex items-center justify-between">
                  <div class="size-8 rounded-xl bg-[#181826] border border-[#27273a] flex items-center justify-center text-purple-400">
                    <IconComponent class="size-4" />
                  </div>
                  {#if isSelected}
                    <CheckCircle2 class="size-4 text-purple-400" />
                  {/if}
                </div>
                <div class="font-bold text-xs text-white mt-2">{tmpl.name}</div>
                <p class="text-[11px] text-zinc-400 line-clamp-2 mt-0.5">{tmpl.description}</p>
              </div>

              {#if tmpl.ranks.length}
                <div class="flex flex-wrap gap-1 mt-2.5 pt-2 border-t border-[#1e1e2d]">
                  {#each tmpl.ranks.slice(0, 3) as r}
                    <span class="text-[9px] px-1.5 py-0.5 rounded bg-[#141420] border border-[#232334] font-mono text-zinc-400">
                      {r.rank}
                    </span>
                  {/each}
                  {#if tmpl.ranks.length > 3}
                    <span class="text-[9px] px-1.5 py-0.5 rounded bg-[#181826] text-zinc-500 font-mono">
                      +{tmpl.ranks.length - 3}
                    </span>
                  {/if}
                </div>
              {/if}
            </button>
          {/each}
        </div>

        {#if currentTemplate.ranks.length}
          <div class="rounded-2xl bg-[#0f0f18]/80 border border-[#1e1e2d] p-3 space-y-1.5 mt-2">
            <div class="text-[11px] font-bold text-white flex items-center gap-1.5">
              <Sparkles class="size-3.5 text-purple-400" />
              Pre-configured specialized roles for {currentTemplate.name}:
            </div>
            <div class="grid grid-cols-1 sm:grid-cols-2 gap-1.5 pt-1">
              {#each currentTemplate.ranks as r}
                <div class="flex items-center gap-2 text-xs bg-[#141420] px-2.5 py-1.5 rounded-xl border border-[#232334]">
                  <span class="size-2 rounded-full shrink-0" style="background-color: {r.color}"></span>
                  <span class="font-bold text-white text-[11px]">{r.rank}</span>
                  <span class="text-zinc-400 text-[11px] truncate">— {r.specialty}</span>
                </div>
              {/each}
            </div>
          </div>
        {/if}
      </div>

      <!-- Team Members -->
      <div class="space-y-2.5">
        <div class="flex items-center justify-between">
          <Label class="text-xs font-bold uppercase tracking-wider text-zinc-400">
            Staffing: Assign Agents ({selectedMembers.length} selected)
          </Label>
          {#if bots.length > 0}
            <Button variant="outline" size="xs" class="h-7 text-xs gap-1 bg-[#181826] border-[#2b2b3e] text-zinc-300 hover:bg-[#202033]" onclick={autoAssign}>
              <Wand2 class="size-3 text-purple-400" />
              Auto-fill from template
            </Button>
          {/if}
        </div>

        {#if bots.length === 0}
          <div class="p-8 text-center border border-dashed border-[#232334] rounded-2xl">
            <Users class="size-8 text-zinc-600 mx-auto mb-2" />
            <p class="text-xs text-zinc-500">No bots created yet. Create bots from the sidebar first.</p>
          </div>
        {:else}
          <div class="grid gap-2 max-h-56 overflow-y-auto pr-1">
            {#each bots as bot}
              {@const mem = selectedMembers.find((m) => m.botId === bot.id)}
              {@const isSelected = Boolean(mem)}
              {@const rankInfo = currentTemplate.ranks.find((r) => r.rank === mem?.rank)}
              <div
                class={cn(
                  "flex items-center justify-between gap-3 rounded-2xl border p-2.5 transition-all",
                  isSelected
                    ? "border-purple-500/70 bg-purple-950/25 shadow-sm"
                    : "border-[#1e1e2d] bg-[#0d0d16]"
                )}
              >
                <div class="flex items-center gap-3 min-w-0">
                  <div class="size-9 rounded-full overflow-hidden bg-[#161624] border border-[#2b2b3d] shrink-0">
                    <img
                      src={bot.avatar_url || getDiceBearUrl(bot.name, bot.avatar_style || "avataaars")}
                      alt={bot.name}
                      class="size-full object-cover"
                    />
                  </div>
                  <div class="min-w-0">
                    <div class="font-bold text-xs text-white truncate">{bot.name}</div>
                    {#if isSelected && mem}
                      <div class="flex items-center gap-1.5 mt-0.5">
                        <span
                          class="text-[10px] py-0 px-1.5 rounded text-white font-mono font-bold"
                          style="background-color: {rankInfo?.color || '#8b5cf6'}"
                        >
                          {mem.rank}
                        </span>
                        <span class="text-[11px] text-zinc-400 truncate">{mem.specialty}</span>
                      </div>
                    {:else}
                      <span class="text-[11px] text-zinc-500 truncate block">
                        {bot.description || "General sovereign agent"}
                      </span>
                    {/if}
                  </div>
                </div>

                {#if isSelected && mem}
                  <Button
                    variant="ghost"
                    size="xs"
                    class="h-7 text-xs text-red-400 hover:bg-red-500/10"
                    onclick={() => toggleMember(bot, mem.rank, mem.specialty)}
                  >
                    Remove
                  </Button>
                {:else}
                  {@const autoR = currentTemplate.ranks[selectedMembers.length % (currentTemplate.ranks.length || 1)]}
                  <Button
                    variant="outline"
                    size="xs"
                    class="h-7 text-xs gap-1 shrink-0 bg-[#181826] border-[#2b2b3e] text-zinc-300 hover:bg-[#202033]"
                    onclick={() => toggleMember(bot, autoR?.rank || "Member", autoR?.specialty || "Generalist")}
                  >
                    <Plus class="size-3" />
                    Assign {autoR?.rank || "Member"}
                  </Button>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </div>

    <div class="flex items-center justify-end gap-2 pt-3 border-t border-purple-500/15">
      <Button variant="outline" size="sm" class="bg-[#141420] border-[#252538] text-zinc-300 hover:bg-[#1a1a2a]" onclick={onClose} disabled={isCreating}>
        Cancel
      </Button>
      <Button
        size="sm"
        class="gap-1.5 bg-purple-600 hover:bg-purple-500 text-white font-medium shadow-md shadow-purple-950/50"
        onclick={create}
        disabled={!name.trim() || isCreating}
      >
        <Building2 class="size-3.5" />
        {isCreating ? "Establishing Office..." : `Create "${name || "Office"}"`}
      </Button>
    </div>
  </Dialog.Content>
</Dialog.Root>
