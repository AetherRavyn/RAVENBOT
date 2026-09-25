<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
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
    Layers,
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

  // Wizard: Office → Template → Staffing
  let step = $state(0);
  $effect(() => {
    if (!open) step = 0;
  });

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
      step = 0;
    } catch (e) {
      console.error("Failed to create office:", e);
    } finally {
      isCreating = false;
    }
  }

  let roomPreviewUrl = $derived(roomAvatarUrl || getDiceBearUrl(name || "office", roomAvatarStyle));
</script>

<Dialog.Root {open} onOpenChange={(o) => !o && onClose()}>
  <Dialog.Content class="sm:max-w-2xl max-h-[88vh] overflow-y-auto">
    <Dialog.Header class="pb-3 border-b border-[var(--hairline)]">
      <Dialog.Title class="text-base font-bold flex items-center gap-2.5 text-[var(--text-primary)]">
        <div class="size-8 rounded-xl bg-[var(--brand-soft)] border border-[var(--brand)]/50 flex items-center justify-center text-[var(--brand-text)]">
          <Building2 class="size-4.5" />
        </div>
        {t("office.createTitle")}
      </Dialog.Title>
      <Dialog.Description class="text-xs text-[var(--text-tertiary)]">
        {t("office.createHint")}
      </Dialog.Description>
    </Dialog.Header>

    <!-- Wizard stepper -->
    <div class="flex items-center gap-1.5 px-1 pt-3">
      {#each [{ label: t("office.stepOffice"), Icon: Building2 }, { label: t("office.stepTemplate"), Icon: Layers }, { label: t("office.stepStaffing"), Icon: Users }] as s, si}
        {@const Icon = s.Icon}
        <button
          type="button"
          disabled={si > 0 && !name.trim()}
          onclick={() => {
            if (si <= step || name.trim()) step = si;
          }}
          class={cn(
            "flex items-center gap-2 h-8 px-2.5 rounded-xl border text-[10px] font-bold uppercase tracking-wider font-mono transition-colors cursor-pointer disabled:cursor-not-allowed disabled:opacity-40",
            si === step
              ? "border-[var(--brand)] bg-[var(--brand-soft)] text-[var(--brand-text)]"
              : "border-[var(--hairline)] bg-[var(--surface-1)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)]"
          )}
        >
          <span
            class={cn(
              "size-5 rounded-lg flex items-center justify-center text-[10px] font-bold font-mono shrink-0",
              si < step
                ? "bg-success/15 text-success"
                : si === step
                  ? "bg-[var(--brand)] text-[var(--text-on-light)]"
                  : "bg-[var(--surface-3)] text-[var(--text-muted)]"
            )}
          >
            {#if si < step}<CheckCircle2 class="size-3" />{:else}{si + 1}{/if}
          </span>
          <Icon class="size-3 opacity-70" />
          {s.label}
        </button>
        {#if si < 2}<span class="flex-1 h-px bg-[var(--hairline)]"></span>{/if}
      {/each}
    </div>

    <div class="grid gap-5 py-3">
      <!-- STEP 1: Room Identity -->
      {#if step === 0}
      <div class="p-4 rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)]/80 flex flex-col md:flex-row gap-4 items-start">
        <button
          type="button"
          onclick={() => (showAvatarPicker = !showAvatarPicker)}
          class="flex flex-col items-center gap-1.5 shrink-0 group focus:outline-none cursor-pointer"
        >
          <div class="size-16 rounded-full overflow-hidden bg-[var(--surface-3)] border border-[var(--brand)]/40 p-0.5 group-hover:scale-105 transition-transform">
            <img src={roomPreviewUrl} alt={name || t("office.fallbackName")} class="size-full rounded-full object-cover" />
          </div>
          <span class="text-[11px] text-[var(--brand-text)] flex items-center gap-1 group-hover:underline font-medium">
            <Sparkles class="size-3" />
            {t("office.changeIcon")}
          </span>
        </button>

        <div class="flex-1 space-y-3 w-full">
          <div class="space-y-1">
            <Label for="room-name" class="text-xs font-semibold text-[var(--text-tertiary)]">
              {t("room.name")}
            </Label>
            <Input
              id="room-name"
              bind:value={name}
              placeholder={t("office.createNamePh")}
              class="h-9 text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)]"
            />
          </div>
          <div class="space-y-1">
            <Label for="room-desc" class="text-xs font-semibold text-[var(--text-tertiary)]">
              {t("office.createMission")}
            </Label>
            <Textarea
              id="room-desc"
              bind:value={description}
              placeholder={t("office.createMissionPh")}
              rows={2}
              class="text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)] resize-none"
            />
          </div>
        </div>
      </div>

      {#if showAvatarPicker}
        <div class="p-4 rounded-2xl border border-[var(--brand)]/40 bg-[var(--surface-1)] shadow-2xl">
          <div class="flex items-center justify-between mb-2">
            <span class="text-xs font-bold text-[var(--text-primary)]">{t("office.createAvatar")}</span>
            <Button variant="ghost" size="xs" onclick={() => (showAvatarPicker = false)}>{t("ui.done")}</Button>
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
      {/if}

      <!-- STEP 2: Template Selection -->
      {#if step === 1}
      <div class="space-y-2">
        <Label class="text-xs font-bold uppercase tracking-wider text-[var(--text-tertiary)]">
          {t("office.templateLabel")}
        </Label>
        <div class="grid grid-cols-2 md:grid-cols-3 gap-2.5">
          {#each templates as [key, tmpl]}
            {@const IconComponent = templateIcons[key] || Building2}
            {@const isSelected = officeTemplate === key}
            <button
              type="button"
              onclick={() => (officeTemplate = key as OfficeTemplateKey)}
              aria-pressed={isSelected}
              class={cn(
 "text-left rounded-2xl border p-3 transition-all flex flex-col justify-between focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--brand)]/60 cursor-pointer",
                isSelected
                  ? "border-[var(--brand)] bg-[var(--brand-soft)] ring-1 ring-[var(--brand)]/60"
                  : "border-[var(--hairline)] bg-[var(--surface-1)] hover:border-[var(--brand)]/40 hover:bg-[var(--surface-2)]"
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
                <div class="font-bold text-xs text-[var(--text-primary)] mt-2">{tmpl.name}</div>
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
            <div class="text-[11px] font-bold text-[var(--text-primary)] flex items-center gap-1.5">
              <Sparkles class="size-3.5 text-[var(--brand-text)]" />
              {t("office.rolesFor", { name: currentTemplate.name })}
            </div>
            <div class="grid grid-cols-1 sm:grid-cols-2 gap-1.5 pt-1">
              {#each currentTemplate.ranks as r}
                <div class="flex items-center gap-2 text-xs bg-[var(--surface-2)] px-2.5 py-1.5 rounded-xl border border-[var(--hairline)]">
                  <span class="size-2 rounded-full shrink-0" style="background-color: {r.color}"></span>
                  <span class="font-bold text-[var(--text-primary)] text-[11px]">{r.rank}</span>
                  <span class="text-[var(--text-tertiary)] text-[11px] truncate">— {r.specialty}</span>
                </div>
              {/each}
            </div>
          </div>
        {/if}
      </div>
      {/if}

      <!-- STEP 3: Staffing -->
      {#if step === 2}
      <div class="space-y-2.5">
        <label class="flex items-start gap-3 p-3 rounded-2xl border border-[var(--brand)]/30 bg-[var(--brand-soft)] cursor-pointer">
          <input type="checkbox" bind:checked={autoStaff} class="mt-0.5 size-4 accent-[var(--brand)] cursor-pointer" />
          <span class="space-y-0.5">
            <span class="text-xs font-bold text-[var(--text-primary)] block">{t("office.autoStaff")}</span>
            <span class="text-[11px] text-[var(--text-tertiary)] block">
              {t("office.autoStaffDesc", { name: currentTemplate.name })}
            </span>
          </span>
        </label>
      </div>

      {#if !autoStaff}
      <!-- Team Members -->
      <div class="space-y-2.5">
        <div class="flex items-center justify-between">
          <Label class="text-xs font-bold uppercase tracking-wider text-[var(--text-tertiary)]">
            {t("office.staffing", { n: selectedMembers.length })}
          </Label>
          {#if bots.length > 0}
            <Button variant="outline" size="xs" class="h-7 text-xs gap-1 bg-[var(--surface-3)] border-[var(--hairline)] text-[var(--text-secondary)] hover:bg-[var(--surface-3)]" onclick={autoAssign}>
              <Wand2 class="size-3 text-[var(--brand-text)]" />
              {t("office.autoFill")}
            </Button>
          {/if}
        </div>

        {#if bots.length === 0}
          <div class="p-8 text-center border border-dashed border-[var(--hairline)] rounded-2xl">
            <Users class="size-8 text-[var(--text-muted)] mx-auto mb-2" />
            <p class="text-xs text-[var(--text-muted)]">{t("office.noBotsYet")}</p>
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
                  <div class="size-9 rounded-full overflow-hidden bg-[var(--surface-3)] border border-[var(--hairline)] shrink-0">
                    <img
                      src={bot.avatar_url || getDiceBearUrl(bot.name, bot.avatar_style || "avataaars")}
                      alt={bot.name}
                      class="size-full object-cover"
                    />
                  </div>
                  <div class="min-w-0">
                    <div class="font-bold text-xs text-[var(--text-primary)] truncate">{bot.name}</div>
                    {#if isSelected && mem}
                      <div class="flex items-center gap-1.5 mt-0.5">
                        <span
                          class="text-[10px] py-0 px-1.5 rounded text-[var(--text-primary)] font-mono font-bold"
                          style="background-color: {rankInfo?.color || '#79b8ff'}"
                        >
                          {mem.rank}
                        </span>
                        <span class="text-[11px] text-[var(--text-tertiary)] truncate">{mem.specialty}</span>
                      </div>
                    {:else}
                      <span class="text-[11px] text-[var(--text-muted)] truncate block">
                        {bot.description || t("office.generalAgent")}
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
                    {t("office.removeMember")}
                  </Button>
                {:else}
                  {@const autoR = currentTemplate.ranks[selectedMembers.length % (currentTemplate.ranks.length || 1)]}
                  <Button
                    variant="outline"
                    size="xs"
                    class="h-7 text-xs gap-1 shrink-0 bg-[var(--surface-3)] border-[var(--hairline)] text-[var(--text-secondary)] hover:bg-[var(--surface-3)]"
                    onclick={() => toggleMember(bot, autoR?.rank || "Member", autoR?.specialty || "Generalist")}
                  >
                    <Plus class="size-3" />
                    {t("office.assignRank", { rank: autoR?.rank || t("ui.fallbackMember") })}
                  </Button>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      </div>
      {/if}
      {/if}
    </div>

    <div class="flex items-center justify-between gap-2 pt-3 border-t border-[var(--hairline)]">
      <Button variant="outline" size="sm" class="bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)] hover:bg-[var(--surface-3)]" onclick={onClose} disabled={isCreating}>
        {t("ui.cancel")}
      </Button>
      <div class="flex items-center gap-2">
        {#if step > 0}
          <Button variant="outline" size="sm" class="h-9 bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)] hover:bg-[var(--surface-3)] gap-1.5 cursor-pointer" onclick={() => (step -= 1)} disabled={isCreating}>
            {t("office.back")}
          </Button>
        {/if}
        {#if step < 2}
          <Button
            size="sm"
            class="h-9 gap-1.5 bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-[var(--text-on-light)] font-medium shadow-md cursor-pointer disabled:opacity-50"
            onclick={() => (step += 1)}
            disabled={!name.trim()}
          >
            {t("office.next")}
            <span class="font-mono">→</span>
          </Button>
        {:else}
          <Button
            size="sm"
            class="h-9 gap-1.5 bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-[var(--text-on-light)] font-medium shadow-md cursor-pointer"
            onclick={create}
            disabled={isCreating}
          >
            <Building2 class="size-3.5" />
            {isCreating ? t("office.establishing") : t("office.createBtn", { name: name || t("office.fallbackName") })}
          </Button>
        {/if}
      </div>
    </div>
  </Dialog.Content>
</Dialog.Root>
