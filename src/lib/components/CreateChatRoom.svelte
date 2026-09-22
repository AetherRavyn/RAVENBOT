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
  // Provision the template's full org (CEO + specialists) automatically. This
  // is what turns a new office into a real team without hand-creating bots.
  let autoStaff = $state(true);

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
      const roomId = (room as any).id;

      if (autoStaff) {
        // Build the full org (CEO + role specialists) from the blueprint.
        // Existing fleet agents that match a role are reused, not duplicated.
        const roles = await invoke<any[]>("default_office_org", {
          officeTemplate,
        });
        if (roles.length > 0) {
          await invoke("provision_office_org", { chatroomId: roomId, roles });
          window.dispatchEvent(new CustomEvent("bots-changed"));
        }
      } else {
        for (const m of selectedMembers) {
          await invoke("add_member_to_chatroom", {
            chatroomId: roomId,
            botId: m.botId,
            rank: m.rank,
            specialty: m.specialty,
          });
        }
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
  <Dialog.Content class="sm:max-w-3xl max-h-[88vh] overflow-y-auto bg-[var(--surface-1)] border border-[var(--brand)]/30 ]  rounded-xl">
    <Dialog.Header class="pb-3 border-[var(--hairline)] border-[var(--hairline)]">
      <Dialog.Title class="text-base font-bold flex items-center gap-2.5 text-white">
        <div class="size-8 rounded-xl bg-[var(--brand-soft)] border border-[var(--brand)]/50 flex items-center justify-center text-[var(--brand-text)]">
          <Building2 class="size-4.5" />
        </div>
        Create Office Workspace
      </Dialog.Title>
      <Dialog.Description class="text-xs text-[var(--text-tertiary)]">
        Form a specialized multi-bot collaborative office with ranked roles, automated task distribution, and shared thread lanes.
      </Dialog.Description>
    </Dialog.Header>

    <div class="grid gap-5 py-3">
      <!-- Room Identity Card -->
      <div class="p-4 rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)]/80 flex flex-col md:flex-row gap-4 items-start">
        <button
          type="button"
          onclick={() => (showAvatarPicker = !showAvatarPicker)}
          class="flex flex-col items-center gap-1.5 shrink-0 group focus:outline-none cursor-pointer"
        >
          <div class="size-16 rounded-full overflow-hidden bg-[var(--surface-3)] border-[var(--hairline)] border-[var(--brand)]/40 p-0.5 ] group-hover:scale-105 transition-transform">
            <img src={roomPreviewUrl} alt={name || "Office"} class="size-full rounded-full object-cover" />
          </div>
          <span class="text-[11px] text-[var(--brand-text)] flex items-center gap-1 group-hover:underline font-medium">
            <Sparkles class="size-3" />
            Change Icon
          </span>
        </button>

        <div class="flex-1 space-y-3 w-full">
          <div class="space-y-1">
            <Label for="room-name" class="text-xs font-bold uppercase tracking-wider text-[var(--text-tertiary)]">
              Office Name
            </Label>
            <Input
              id="room-name"
              bind:value={name}
              placeholder="e.g. Core Engineering, Growth Pod, Product Studio..."
              class="h-9 text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)]"
            />
          </div>
          <div class="space-y-1">
            <Label for="room-desc" class="text-xs font-bold uppercase tracking-wider text-[var(--text-tertiary)]">
              Office Mission
            </Label>
            <Textarea
              id="room-desc"
              bind:value={description}
              placeholder="What does this office collaborate on?"
              rows={2}
              class="text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)] resize-none"
            />
          </div>
        </div>
      </div>

      {#if showAvatarPicker}
        <div class="p-4 rounded-2xl border border-[var(--brand)]/40 bg-[var(--surface-1)] shadow-2xl">
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
        <Label class="text-xs font-bold uppercase tracking-wider text-[var(--text-tertiary)]">
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
                  ? "border-[var(--brand)] bg-[var(--brand-soft)] ] ring-1 ring-[var(--brand)]/60"
                  : "border-[var(--hairline)] bg-[var(--surface-1)] hover:border-[var(--brand)]/40 hover:bg-[#131320]"
              )}
            >
              <div>
                <div class="flex items-center justify-between">
                  <div class="size-8 rounded-xl bg-[var(--surface-3)] border border-[var(--hairline)] flex items-center justify-center text-[var(--brand-text)]">
                    <IconComponent class="size-4" />
                  </div>
                  {#if isSelected}
                    <CheckCircle2 class="size-4 text-[var(--brand-text)]" />
                  {/if}
                </div>
                <div class="font-bold text-xs text-white mt-2">{tmpl.name}</div>
                <p class="text-[11px] text-[var(--text-tertiary)] line-clamp-2 mt-0.5">{tmpl.description}</p>
              </div>

              {#if tmpl.ranks.length}
                <div class="flex flex-wrap gap-1 mt-2.5 pt-2 border-t border-[var(--hairline)]">
                  {#each tmpl.ranks.slice(0, 3) as r}
                    <span class="text-[9px] px-1.5 py-0.5 rounded bg-[var(--surface-2)] border border-[var(--hairline)] font-mono text-[var(--text-tertiary)]">
                      {r.rank}
                    </span>
                  {/each}
                  {#if tmpl.ranks.length > 3}
                    <span class="text-[9px] px-1.5 py-0.5 rounded bg-[var(--surface-3)] text-[var(--text-muted)] font-mono">
                      +{tmpl.ranks.length - 3}
                    </span>
                  {/if}
                </div>
              {/if}
            </button>
          {/each}
        </div>

        {#if currentTemplate.ranks.length}
          <div class="rounded-2xl bg-[var(--surface-1)]/80 border border-[var(--hairline)] p-3 space-y-1.5 mt-2">
            <div class="text-[11px] font-bold text-white flex items-center gap-1.5">
              <Sparkles class="size-3.5 text-[var(--brand-text)]" />
              Pre-configured specialized roles for {currentTemplate.name}:
            </div>
            <div class="grid grid-cols-1 sm:grid-cols-2 gap-1.5 pt-1">
              {#each currentTemplate.ranks as r}
                <div class="flex items-center gap-2 text-xs bg-[var(--surface-2)] px-2.5 py-1.5 rounded-xl border border-[var(--hairline)]">
                  <span class="size-2 rounded-full shrink-0" style="background-color: {r.color}"></span>
                  <span class="font-bold text-white text-[11px]">{r.rank}</span>
                  <span class="text-[var(--text-tertiary)] text-[11px] truncate">— {r.specialty}</span>
                </div>
              {/each}
            </div>
          </div>
        {/if}
      </div>

      <!-- Staffing mode -->
      <div class="space-y-2.5">
        <label class="flex items-start gap-3 p-3 rounded-2xl border border-[var(--brand)]/30 bg-[var(--brand-soft)] cursor-pointer">
          <input type="checkbox" bind:checked={autoStaff} class="mt-0.5 size-4 accent-[var(--brand)] cursor-pointer" />
          <span class="space-y-0.5">
            <span class="text-xs font-bold text-white block">Auto-staff the full team</span>
            <span class="text-[11px] text-[var(--text-tertiary)] block">
              Creates the {currentTemplate.name} org automatically — a CEO plus each specialist role,
              with role-specific prompts and skills. No pre-made bots required. You can add or remove
              people later from the office.
            </span>
          </span>
        </label>
      </div>

      {#if !autoStaff}
      <!-- Team Members -->
      <div class="space-y-2.5">
        <div class="flex items-center justify-between">
          <Label class="text-xs font-bold uppercase tracking-wider text-[var(--text-tertiary)]">
            Staffing: Assign Agents ({selectedMembers.length} selected)
          </Label>
          {#if bots.length > 0}
            <Button variant="outline" size="xs" class="h-7 text-xs gap-1 bg-[var(--surface-3)] border-[var(--hairline)] text-[var(--text-secondary)] hover:bg-[#202033]" onclick={autoAssign}>
              <Wand2 class="size-3 text-[var(--brand-text)]" />
              Auto-fill from template
            </Button>
          {/if}
        </div>

        {#if bots.length === 0}
          <div class="p-8 text-center border border-dashed border-[var(--hairline)] rounded-2xl">
            <Users class="size-8 text-[var(--text-muted)] mx-auto mb-2" />
            <p class="text-xs text-[var(--text-muted)]">No bots created yet. Create bots from the sidebar first.</p>
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
                    ? "border-[var(--brand)]/70 bg-[var(--brand-soft)] shadow-sm"
                    : "border-[var(--hairline)] bg-[var(--surface-1)]"
                )}
              >
                <div class="flex items-center gap-3 min-w-0">
                  <div class="size-9 rounded-full overflow-hidden bg-[var(--surface-3)] border border-[#2b2b3d] shrink-0">
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
                          style="background-color: {rankInfo?.color || '#007cf7'}"
                        >
                          {mem.rank}
                        </span>
                        <span class="text-[11px] text-[var(--text-tertiary)] truncate">{mem.specialty}</span>
                      </div>
                    {:else}
                      <span class="text-[11px] text-[var(--text-muted)] truncate block">
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
                    class="h-7 text-xs gap-1 shrink-0 bg-[var(--surface-3)] border-[var(--hairline)] text-[var(--text-secondary)] hover:bg-[#202033]"
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
      {/if}
    </div>

    <div class="flex items-center justify-end gap-2 pt-3 border-t border-[var(--hairline)]">
      <Button variant="outline" size="sm" class="bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)] hover:bg-[var(--surface-3)]" onclick={onClose} disabled={isCreating}>
        Cancel
      </Button>
      <Button
        size="sm"
        class="gap-1.5 bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-white font-medium shadow-md "
        onclick={create}
        disabled={!name.trim() || isCreating}
      >
        <Building2 class="size-3.5" />
        {isCreating ? "Establishing Office..." : `Create "${name || "Office"}"`}
      </Button>
    </div>
  </Dialog.Content>
</Dialog.Root>
