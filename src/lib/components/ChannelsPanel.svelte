<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import SimpleSelect from "$lib/components/SimpleSelect.svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import { Textarea } from "$lib/components/ui/textarea";
  import {
    Hash, X, Plus, Trash2, Loader2, Save, FolderOpen, Users,
  } from "@lucide/svelte";

  interface Props {
    onClose: () => void;
    onChanged?: () => void;
  }
  let { onClose, onChanged }: Props = $props();

  interface Channel {
    id: string;
    name: string;
    description: string;
    instructions: string;
    working_folder: string | null;
    color: string | null;
    position: number;
    bot_ids: string[];
    responder_rules?: { mode?: "all" | "lead" | "manual"; lead_bot_id?: string | null } | null;
  }

  let channels = $state<Channel[]>([]);
  let bots = $state<any[]>([]);
  let selected = $state<Channel | null>(null);
  let loading = $state(false);
  let saving = $state(false);
  let error = $state<string | null>(null);

  async function load() {
    loading = true;
    try {
      channels = await invoke<Channel[]>("list_channels");
      bots = await invoke<any[]>("list_bots");
      if (selected) {
        selected = channels.find((c) => c.id === selected!.id) ?? null;
      }
    } catch (e: any) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    load();
  });

  async function addChannel() {
    try {
      const id = await invoke<string>("create_channel", {
        name: `Channel ${channels.length + 1}`,
        description: "",
        instructions: "",
        workingFolder: null,
        color: null,
        botIds: [],
      });
      await load();
      selected = channels.find((c) => c.id === id) ?? null;
      onChanged?.();
    } catch (e: any) {
      error = String(e);
    }
  }

  async function save() {
    if (!selected) return;
    saving = true;
    try {
      await invoke("update_channel", {
        channel: {
          id: selected.id,
          name: selected.name,
          description: selected.description,
          instructions: selected.instructions,
          working_folder: selected.working_folder,
          color: selected.color,
          position: selected.position,
          bot_ids: selected.bot_ids,
          responder_rules: selected.responder_rules ?? null,
        },
      });
      await invoke("set_channel_bots", { channelId: selected.id, botIds: selected.bot_ids });
      await load();
      onChanged?.();
    } catch (e: any) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  async function removeChannel() {
    if (!selected) return;
    try {
      await invoke("delete_channel", { channelId: selected.id });
      selected = null;
      await load();
      onChanged?.();
    } catch (e: any) {
      error = String(e);
    }
  }

  function setResponderMode(mode: string) {
    if (!selected) return;
    selected.responder_rules = {
      mode: mode as any,
      lead_bot_id: selected.responder_rules?.lead_bot_id ?? null,
    };
    selected = { ...selected };
  }

  function setResponderLead(value: string) {
    if (!selected) return;
    selected.responder_rules = {
      mode: selected.responder_rules?.mode ?? "lead",
      lead_bot_id: value || null,
    };
    selected = { ...selected };
  }

  function toggleBot(botId: string) {
    if (!selected) return;
    const has = selected.bot_ids.includes(botId);
    selected.bot_ids = has
      ? selected.bot_ids.filter((b) => b !== botId)
      : [...selected.bot_ids, botId];
  }
</script>

<div class="fixed inset-0 z-[60] flex items-center justify-center bg-black/60  p-4">
  <div class="modal-panel w-full max-w-3xl flex flex-col max-h-[88vh]">
    <div class="modal-header flex items-center justify-between shrink-0">
      <div class="flex items-center gap-2">
        <Hash class="size-4 text-success" />
        <span class="font-bold text-sm text-[var(--text-primary)]">{t("channel.title")}</span>
        <span class="text-[10px] text-[var(--text-muted)]">{t("channel.subtitle")}</span>
      </div>
      <div class="flex items-center gap-2">
        <button
          type="button"
          onclick={addChannel}
          class="h-7 px-3 rounded-full bg-success/15 border border-success/40 text-success text-[11px] font-bold flex items-center gap-1 cursor-pointer hover:bg-success/25"
        >
          <Plus class="size-3" /> {t("channel.new")}
        </button>
        <button
          type="button"
          onclick={onClose}
          aria-label="Close"
          class="size-7 rounded-full bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] flex items-center justify-center cursor-pointer"
        >
          <X class="size-3.5" />
        </button>
      </div>
    </div>

    <div class="flex flex-1 min-h-0">
      <!-- List -->
      <div class="w-52 border-r border-[var(--hairline)] overflow-y-auto shrink-0">
        {#if loading && channels.length === 0}
          <div class="p-4 text-[var(--text-muted)] text-xs flex items-center gap-2"><Loader2 class="size-3.5 animate-spin" /> {t("ui.loading")}</div>
        {:else if channels.length === 0}
          <div class="p-4 text-[var(--text-muted)] text-xs">{t("channel.empty")}</div>
        {/if}
        {#each channels as c (c.id)}
          <button
            type="button"
            onclick={() => (selected = c)}
            aria-current={selected?.id === c.id ? "true" : undefined}
            class="w-full text-left px-3.5 py-2.5 border-b border-[var(--hairline)] transition-colors cursor-pointer {selected?.id === c.id ? 'bg-success/10' : 'hover:bg-[var(--surface-3)]'}"
          >
            <div class="flex items-center gap-1.5 text-xs font-semibold text-[var(--text-primary)]">
              <Hash class="size-3 text-success shrink-0" style={c.color ? `color:${c.color}` : ''} />
              <span class="truncate">{c.name}</span>
            </div>
            <div class="text-[10px] text-[var(--text-muted)] mt-0.5 truncate">{t("channel.botsN", { n: c.bot_ids.length })}</div>
          </button>
        {/each}
      </div>

      <!-- Editor -->
      <div class="flex-1 overflow-y-auto p-5">
        {#if !selected}
          <div class="h-full flex items-center justify-center text-[var(--text-muted)] text-xs">
            {t("channel.selectHint")}
          </div>
        {:else}
          <div class="space-y-4">
            {#if error}
              <div class="rounded-lg border border-danger/30 bg-danger/20 px-3 py-2 text-[11px] text-danger" role="alert">{error}</div>
            {/if}
            <div class="space-y-1.5">
              <Label for="channel-name" class="text-xs font-bold uppercase tracking-wider text-[var(--text-tertiary)]">{t("room.name")}</Label>
              <Input id="channel-name" bind:value={selected.name} class="h-9 text-xs bg-[var(--surface-2)] border-[var(--hairline)]" />
            </div>
            <div class="space-y-1.5">
              <Label for="channel-description" class="text-xs font-bold uppercase tracking-wider text-[var(--text-tertiary)]">{t("channel.description")}</Label>
              <Input id="channel-description" bind:value={selected.description} class="h-9 text-xs bg-[var(--surface-2)] border-[var(--hairline)]" />
            </div>
            <div class="space-y-1.5">
              <Label for="channel-instructions" class="text-xs font-bold uppercase tracking-wider text-[var(--text-tertiary)]">{t("channel.instructions")}</Label>
              <Textarea
                id="channel-instructions"
                bind:value={selected.instructions}
                rows={5}
                placeholder={t("channel.instructionsPh")}
                class="text-xs bg-[var(--surface-2)] border-[var(--hairline)]"
              />
            </div>
            <div class="space-y-1.5">
              <Label for="channel-folder" class="text-xs font-bold uppercase tracking-wider text-[var(--text-tertiary)] flex items-center gap-1.5">
                <FolderOpen class="size-3.5" /> {t("channel.folder")}
              </Label>
              <Input id="channel-folder" bind:value={selected.working_folder} placeholder="/home/you/project" class="h-9 text-xs font-mono bg-[var(--surface-2)] border-[var(--hairline)]" />
            </div>
            <div class="space-y-1.5">
              <Label class="text-xs font-bold uppercase tracking-wider text-[var(--text-tertiary)] flex items-center gap-1.5">
                <Users class="size-3.5" /> {t("channel.rules")}
              </Label>
              <p class="text-[10px] text-[var(--text-muted)]">
                {t("channel.rulesHint")}
              </p>
              <div class="grid grid-cols-3 gap-2" role="radiogroup" aria-label={t("channel.rules")}>
                {#each [["all", t("channel.modeAll")], ["lead", t("channel.modeLead")], ["manual", t("channel.modeManual")]] as [mode, label]}
                  <button
                    type="button"
                    onclick={() => setResponderMode(mode as string)}
                    role="radio"
                    aria-checked={(selected!.responder_rules?.mode ?? 'all') === mode}
                    class="h-8 rounded-lg border text-[11px] transition-colors cursor-pointer {(selected!.responder_rules?.mode ?? 'all') === mode ? 'bg-success/20 border-success/40 text-success' : 'bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)]'}"
                  >
                    {label}
                  </button>
                {/each}
              </div>
              {#if (selected!.responder_rules?.mode ?? "all") === "lead"}
                <SimpleSelect
                  value={selected!.responder_rules?.lead_bot_id ?? ""}
                  options={[
                    { value: "", label: t("channel.autoLead") },
                    ...bots
                      .filter((b) => selected!.bot_ids.includes(b.id))
                      .map((b) => ({ value: b.id, label: b.name })),
                  ]}
                  onValueChange={setResponderLead}
                  placeholder={t("channel.auto")}
                  class="h-8 mt-1"
                />
              {/if}
              {#if (selected!.responder_rules?.mode ?? "all") === "manual"}
                <p class="text-[10px] text-[var(--text-muted)]">{t("channel.manualHint")}</p>
              {/if}
            </div>

            <div class="space-y-1.5">
              <Label class="text-xs font-bold uppercase tracking-wider text-[var(--text-tertiary)] flex items-center gap-1.5">
                <Users class="size-3.5" /> {t("channel.roster")}
              </Label>
              <div class="flex flex-wrap gap-1.5">
                {#each bots as b (b.id)}
                  <button
                    type="button"
                    onclick={() => toggleBot(b.id)}
                    aria-pressed={selected.bot_ids.includes(b.id)}
                    class="h-7 px-2.5 rounded-full text-[11px] border transition-colors cursor-pointer {selected.bot_ids.includes(b.id) ? 'bg-success/20 border-success/40 text-success' : 'bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)]'}"
                  >
                    {b.name}
                  </button>
                {/each}
              </div>
            </div>
            <div class="flex items-center justify-between pt-1">
              <Button variant="destructive" size="sm" onclick={removeChannel} class="gap-1.5 bg-red-950/40 text-red-400 hover:bg-red-900/60 border border-red-800/40">
                <Trash2 class="size-3.5" /> {t("ui.delete")}
              </Button>
              <Button size="sm" onclick={save} disabled={saving} class="gap-1.5">
                {#if saving}<Loader2 class="size-3.5 animate-spin" />{:else}<Save class="size-3.5" />{/if}
                {t("channel.save")}
              </Button>
            </div>
          </div>
        {/if}
      </div>
    </div>
  </div>
</div>
