<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { Button } from "$lib/components/ui/button";
  import {
    Plus,
    Clock,
    Loader2,
    Trash2,
    Play,
    CheckCircle2,
    Circle,
    AlertTriangle,
  } from "@lucide/svelte";

  interface Props {
    bot: any;
  }

  let { bot }: Props = $props();

  interface Routine {
    id: string;
    bot_id: string;
    name: string;
    description: string;
    schedule: string;
    instruction: string;
    is_enabled: boolean;
    last_run_at: string | null;
    created_at: string;
    updated_at: string;
  }

  let routines = $state<Routine[]>([]);
  let loading = $state(true);
  let creating = $state(false);
  let showCreate = $state(false);
  let error = $state<string | null>(null);

  // Create form state
  let newName = $state("");
  let newSchedule = $state("0 9 * * 1-5");
  let newInstruction = $state("");

  const schedulePresets = [
    { label: "Every hour", value: "0 * * * *" },
    { label: "Weekdays 9am", value: "0 9 * * 1-5" },
    { label: "Daily midnight", value: "0 0 * * *" },
    { label: "Every Monday 9am", value: "0 9 * * 1" },
    { label: "Every 5 min", value: "*/5 * * * *" },
  ];

  let schedulerStatus = $state<{ running: boolean } | null>(null);

  onMount(async () => {
    await load();
    try {
      schedulerStatus = await invoke("get_scheduler_status");
    } catch (e) {
      console.error(e);
    }
  });

  async function load() {
    loading = true;
    error = null;
    try {
      routines = await invoke("list_routines", { botId: bot.id });
    } catch (e: any) {
      error = e?.toString() || "Failed to load routines";
    } finally {
      loading = false;
    }
  }

  async function createRoutine() {
    if (!newName.trim() || !newInstruction.trim() || !newSchedule.trim()) return;
    creating = true;
    error = null;
    try {
      const routine = await invoke("create_routine", {
        botId: bot.id,
        name: newName.trim(),
        schedule: newSchedule.trim(),
        description: "",
        instruction: newInstruction.trim(),
      });
      routines = [routine as Routine, ...routines];
      showCreate = false;
      newName = "";
      newInstruction = "";
    } catch (e: any) {
      error = e?.toString() || "Failed to create routine";
    } finally {
      creating = false;
    }
  }

  async function toggleRoutine(routine: Routine) {
    try {
      const updated = { ...routine, is_enabled: !routine.is_enabled };
      await invoke("update_routine", { routine: updated });
      routines = routines.map((r) => (r.id === routine.id ? updated : r));
    } catch (e: any) {
      error = e?.toString() || "Failed to update routine";
    }
  }

  async function deleteRoutine(routine: Routine) {
    try {
      await invoke("delete_routine", { routineId: routine.id });
      routines = routines.filter((r) => r.id !== routine.id);
    } catch (e: any) {
      error = e?.toString() || "Failed to delete routine";
    }
  }

  async function runNow(routine: Routine) {
    try {
      await invoke("run_routine_now", { routineId: routine.id });
    } catch (e: any) {
      error = e?.toString() || "Failed to run routine";
    }
  }

  function formatLastRun(iso: string | null) {
    if (!iso) return "never";
    try {
      return new Date(iso).toLocaleString([], {
        month: "short",
        day: "numeric",
        hour: "2-digit",
        minute: "2-digit",
      });
    } catch {
      return "unknown";
    }
  }
</script>

<div class="space-y-3">
  <!-- Header -->
  <div class="flex items-center justify-between">
    <div class="flex items-center gap-2">
      <Clock class="size-3.5 text-sky-400" />
      <span class="text-[11px] font-bold text-zinc-300 uppercase tracking-wider font-mono">
        Scheduled Routines
      </span>
      {#if schedulerStatus}
        <span class="text-[9px] font-mono px-1.5 py-0.5 rounded {schedulerStatus.running ? 'bg-emerald-500/15 text-emerald-300 border border-emerald-500/30' : 'bg-zinc-800 text-zinc-500 border border-white/10'}">
          {schedulerStatus.running ? "scheduler running" : "scheduler idle"}
        </span>
      {/if}
    </div>
    <Button
      size="sm"
      variant="outline"
      class="h-6 px-2 text-[10px] gap-1 cursor-pointer"
      onclick={() => (showCreate = !showCreate)}
    >
      <Plus class="size-3" />
      New
    </Button>
  </div>

  {#if error}
    <div class="flex items-center gap-2 text-[10px] text-rose-400 font-mono">
      <AlertTriangle class="size-3 shrink-0" />
      <span>{error}</span>
    </div>
  {/if}

  <!-- Create form -->
  {#if showCreate}
    <div class="rounded-xl border border-white/10 bg-[#0b0b10] p-3 space-y-2">
      <div class="grid grid-cols-2 gap-2">
        <input
          bind:value={newName}
          placeholder="Routine name"
          class="h-7 px-2.5 rounded-lg bg-[#0e0e14] border border-white/10 text-[11px] text-white placeholder:text-zinc-500 focus:outline-none focus:border-sky-500/50"
        />
        <div class="relative">
          <input
            bind:value={newSchedule}
            placeholder="cron schedule"
            class="w-full h-7 px-2.5 rounded-lg bg-[#0e0e14] border border-white/10 text-[11px] font-mono text-white placeholder:text-zinc-500 focus:outline-none focus:border-sky-500/50"
          />
        </div>
      </div>

      <div class="flex flex-wrap gap-1">
        {#each schedulePresets as preset}
          <button
            type="button"
            class="h-5 px-1.5 rounded-md text-[9px] font-mono text-zinc-400 hover:text-sky-300 bg-white/5 border border-white/10 hover:border-sky-500/40 cursor-pointer transition-colors {newSchedule === preset.value ? 'text-sky-300 border-sky-500/50' : ''}"
            onclick={() => (newSchedule = preset.value)}
          >
            {preset.label}
          </button>
        {/each}
      </div>

      <textarea
        bind:value={newInstruction}
        placeholder="Instruction the agent should execute on schedule…"
        rows={2}
        class="w-full px-2.5 py-2 rounded-lg bg-[#0e0e14] border border-white/10 text-[11px] text-white placeholder:text-zinc-500 resize-none focus:outline-none focus:border-sky-500/50"
      ></textarea>

      <div class="flex justify-end">
        <Button
          size="sm"
          class="h-7 px-3 text-[10px] gap-1 bg-white text-black hover:bg-zinc-200 cursor-pointer"
          disabled={!newName.trim() || !newInstruction.trim() || creating}
          onclick={createRoutine}
        >
          {#if creating}
            <Loader2 class="size-3 animate-spin" />
          {/if}
          Create
        </Button>
      </div>
    </div>
  {/if}

  <!-- Routines list -->
  {#if loading}
    <div class="flex items-center justify-center py-6 text-zinc-500">
      <Loader2 class="size-4 animate-spin" />
    </div>
  {:else if routines.length === 0}
    <div class="text-center py-5 text-[11px] text-zinc-500 font-mono">
      No routines yet — schedule {bot.name} to work on a cron.
    </div>
  {:else}
    <div class="space-y-1.5 max-h-56 overflow-y-auto pr-1">
      {#each routines as routine (routine.id)}
        <div class="rounded-xl border border-white/10 bg-[#0e0e14] p-2.5 space-y-1.5">
          <div class="flex items-center justify-between gap-2">
            <div class="flex items-center gap-2 min-w-0">
              {#if routine.is_enabled}
                <CheckCircle2 class="size-3 text-emerald-400 shrink-0" />
              {:else}
                <Circle class="size-3 text-zinc-600 shrink-0" />
              {/if}
              <span class="text-[11px] font-bold text-white truncate">{routine.name}</span>
            </div>
            <div class="flex items-center gap-1 shrink-0">
              <button
                type="button"
                class="size-5 rounded-md bg-white/5 border border-white/10 text-zinc-400 hover:text-sky-300 hover:border-sky-500/40 flex items-center justify-center cursor-pointer transition-colors"
                onclick={() => runNow(routine)}
                title="Run now"
              >
                <Play class="size-2.5" />
              </button>
              <button
                type="button"
                class="size-5 rounded-md bg-white/5 border border-white/10 text-zinc-400 hover:text-rose-400 hover:border-rose-500/40 flex items-center justify-center cursor-pointer transition-colors"
                onclick={() => deleteRoutine(routine)}
                title="Delete routine"
              >
                <Trash2 class="size-2.5" />
              </button>
            </div>
          </div>

          <div class="flex items-center gap-2 text-[9px] font-mono text-zinc-500">
            <span class="px-1.5 py-0.5 rounded bg-sky-500/10 border border-sky-500/25 text-sky-300">
              {routine.schedule}
            </span>
            <span>last run: {formatLastRun(routine.last_run_at)}</span>
          </div>

          <p class="text-[10px] text-zinc-400 leading-relaxed line-clamp-2">{routine.instruction}</p>

          <button
            type="button"
            class="text-[9px] font-mono text-zinc-500 hover:text-zinc-300 cursor-pointer transition-colors"
            onclick={() => toggleRoutine(routine)}
          >
            {routine.is_enabled ? "Disable" : "Enable"}
          </button>
        </div>
      {/each}
    </div>
  {/if}
</div>
