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
    Webhook,
    Copy,
    Check,
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

  // Webhook triggers: enabled state per routine + the URL shown once on enable.
  let webhookEnabled = $state<Record<string, boolean>>({});
  let webhookUrl = $state<{ routineId: string; url: string; header: string } | null>(null);
  let copied = $state(false);

  onMount(async () => {
    await load();
    try {
      schedulerStatus = await invoke("get_scheduler_status");
    } catch (e) {
      console.error(e);
    }
  });

  async function refreshWebhookStatus() {
    const next: Record<string, boolean> = {};
    for (const r of routines) {
      try {
        const s = await invoke<{ enabled: boolean }>("get_routine_webhook_status", { routineId: r.id });
        next[r.id] = s.enabled;
      } catch {
        next[r.id] = false;
      }
    }
    webhookEnabled = next;
  }

  async function toggleWebhook(routine: Routine) {
    try {
      if (webhookEnabled[routine.id]) {
        await invoke("disable_routine_webhook", { routineId: routine.id });
        webhookEnabled = { ...webhookEnabled, [routine.id]: false };
      } else {
        const res = await invoke<{ url: string; header: string }>("enable_routine_webhook", { routineId: routine.id });
        webhookEnabled = { ...webhookEnabled, [routine.id]: true };
        webhookUrl = { routineId: routine.id, url: res.url, header: res.header };
        copied = false;
      }
    } catch (e: any) {
      error = e?.toString() || "Failed to update webhook";
    }
  }

  async function copyWebhook() {
    if (!webhookUrl) return;
    try {
      await navigator.clipboard.writeText(webhookUrl.url);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch (e) {
      console.error(e);
    }
  }

  async function load() {
    loading = true;
    error = null;
    try {
      routines = await invoke("list_routines", { botId: bot.id });
      await refreshWebhookStatus();
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
      <Clock class="size-3.5 text-[var(--brand-text)]" />
      <span class="text-[11px] font-bold text-[var(--text-secondary)] uppercase tracking-wider font-mono">
        Scheduled Routines
      </span>
      {#if schedulerStatus}
        <span class="text-[9px] font-mono px-1.5 py-0.5 rounded {schedulerStatus.running ? 'bg-success/15 text-success border border-success/30' : 'bg-[var(--surface-2)] text-[var(--text-muted)] border border-[var(--hairline)]'}">
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
    <div class="flex items-center gap-2 text-[10px] text-danger font-mono">
      <AlertTriangle class="size-3 shrink-0" />
      <span>{error}</span>
    </div>
  {/if}

  <!-- Create form -->
  {#if showCreate}
    <div class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] p-3 space-y-2">
      <div class="grid grid-cols-2 gap-2">
        <input
          bind:value={newName}
          placeholder="Routine name"
          class="h-7 px-2.5 rounded-lg bg-[var(--surface-1)] border border-[var(--hairline)] text-[11px] text-white placeholder:text-[var(--text-muted)] focus:outline-none focus:border-[var(--brand)]/50"
        />
        <div class="relative">
          <input
            bind:value={newSchedule}
            placeholder="cron schedule"
            class="w-full h-7 px-2.5 rounded-lg bg-[var(--surface-1)] border border-[var(--hairline)] text-[11px] font-mono text-white placeholder:text-[var(--text-muted)] focus:outline-none focus:border-[var(--brand)]/50"
          />
        </div>
      </div>

      <div class="flex flex-wrap gap-1">
        {#each schedulePresets as preset}
          <button
            type="button"
            class="h-5 px-1.5 rounded-md text-[9px] font-mono text-[var(--text-tertiary)] hover:text-[var(--brand-hover)] bg-[var(--surface-2)] border border-[var(--hairline)] hover:border-[var(--brand)]/40 cursor-pointer transition-colors {newSchedule === preset.value ? 'text-[var(--brand-text)] border-[var(--brand)]/50' : ''}"
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
        class="w-full px-2.5 py-2 rounded-lg bg-[var(--surface-1)] border border-[var(--hairline)] text-[11px] text-white placeholder:text-[var(--text-muted)] resize-none focus:outline-none focus:border-[var(--brand)]/50"
      ></textarea>

      <div class="flex justify-end">
        <Button
          size="sm"
          class="h-7 px-3 text-[10px] gap-1 bg-[var(--surface-light)] text-[var(--text-on-light)] hover:bg-white cursor-pointer"
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
    <div class="flex items-center justify-center py-6 text-[var(--text-muted)]">
      <Loader2 class="size-4 animate-spin" />
    </div>
  {:else if routines.length === 0}
    <div class="text-center py-5 text-[11px] text-[var(--text-muted)] font-mono">
      No routines yet — schedule {bot.name} to work on a cron.
    </div>
  {:else}
    <div class="space-y-1.5 max-h-56 overflow-y-auto pr-1">
      {#each routines as routine (routine.id)}
        <div class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] p-2.5 space-y-1.5">
          <div class="flex items-center justify-between gap-2">
            <div class="flex items-center gap-2 min-w-0">
              {#if routine.is_enabled}
                <CheckCircle2 class="size-3 text-success shrink-0" />
              {:else}
                <Circle class="size-3 text-[var(--text-muted)] shrink-0" />
              {/if}
              <span class="text-[11px] font-bold text-white truncate">{routine.name}</span>
            </div>
            <div class="flex items-center gap-1 shrink-0">
              <button
                type="button"
                class="size-5 rounded-md bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--brand-hover)] hover:border-[var(--brand)]/40 flex items-center justify-center cursor-pointer transition-colors"
                onclick={() => runNow(routine)}
                title="Run now"
              >
                <Play class="size-2.5" />
              </button>
              <button
                type="button"
                class="size-5 rounded-md border flex items-center justify-center cursor-pointer transition-colors {webhookEnabled[routine.id] ? 'bg-success/15 border-success/40 text-success' : 'bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-success hover:border-success/40'}"
                onclick={() => toggleWebhook(routine)}
                title={webhookEnabled[routine.id] ? "Webhook enabled — click to disable" : "Enable inbound webhook"}
              >
                <Webhook class="size-2.5" />
              </button>
              <button
                type="button"
                class="size-5 rounded-md bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-danger hover:border-danger/40 flex items-center justify-center cursor-pointer transition-colors"
                onclick={() => deleteRoutine(routine)}
                title="Delete routine"
              >
                <Trash2 class="size-2.5" />
              </button>
            </div>
          </div>

          <div class="flex items-center gap-2 text-[9px] font-mono text-[var(--text-muted)]">
            <span class="px-1.5 py-0.5 rounded bg-[var(--brand-soft)] border border-[var(--brand)]/25 text-[var(--brand-text)]">
              {routine.schedule}
            </span>
            <span>last run: {formatLastRun(routine.last_run_at)}</span>
          </div>

          <p class="text-[10px] text-[var(--text-tertiary)] leading-relaxed line-clamp-2">{routine.instruction}</p>

          <button
            type="button"
            class="text-[9px] font-mono text-[var(--text-muted)] hover:text-[var(--text-primary)] cursor-pointer transition-colors"
            onclick={() => toggleRoutine(routine)}
          >
            {routine.is_enabled ? "Disable" : "Enable"}
          </button>

          {#if webhookUrl?.routineId === routine.id}
            <div class="mt-1.5 rounded-lg border border-success/30 bg-success/20 p-2 space-y-1">
              <div class="text-[9px] text-success">POST to this URL (copy it now — shown once):</div>
              <div class="flex items-center gap-1.5">
                <code class="flex-1 text-[9px] font-mono text-success break-all">{webhookUrl.url}</code>
                <button
                  type="button"
                  class="size-5 shrink-0 rounded-md bg-success/15 border border-success/40 text-success flex items-center justify-center cursor-pointer"
                  onclick={copyWebhook}
                  title="Copy URL"
                >
                  {#if copied}<Check class="size-2.5" />{:else}<Copy class="size-2.5" />{/if}
                </button>
              </div>
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>
