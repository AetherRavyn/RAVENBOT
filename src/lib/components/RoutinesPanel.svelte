<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { t } from "$lib/i18n";
  import { Button } from "$lib/components/ui/button";
  import SimpleSelect from "$lib/components/SimpleSelect.svelte";
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
    /** Fleet for the in-header bot switcher (Workspace pane). Modal hosts omit it. */
    bots?: any[];
    onBotChange?: (id: string) => void;
  }

  let { bot, bots = [], onBotChange }: Props = $props();

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

  const schedulePresets = $derived([
    { label: t("routines.p1"), value: "0 * * * *" },
    { label: t("routines.p2"), value: "0 9 * * 1-5" },
    { label: t("routines.p3"), value: "0 0 * * *" },
    { label: t("routines.p4"), value: "0 9 * * 1" },
    { label: t("routines.p5"), value: "*/5 * * * *" },
  ]);

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
    if (!iso) return t("routines.never");
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
  <!-- Compact pane header (OpenBot ConnectorCenter pattern): icon + bold title
       + chips on the left, bot switcher + New on the right. This header is the
       panel's ONLY title — hosts (Workspace pane, thread modal) must not add
       a second one. -->
  <div class="flex items-center justify-between gap-2">
    <div class="flex items-center gap-2 min-w-0">
      <Clock class="size-4 text-[var(--brand-text)] shrink-0" />
      <span class="text-[13px] font-bold text-[var(--text-primary)] truncate">{t("routines.title")}</span>
      <span class="text-[10px] px-1.5 py-0.5 rounded-md bg-[var(--brand-soft)] border border-[var(--brand)]/25 text-[var(--brand-text)] font-mono">
        {routines.length}
      </span>
      {#if schedulerStatus}
        <span class="text-[9px] font-mono px-1.5 py-0.5 rounded {schedulerStatus.running ? 'bg-success/15 text-success border border-success/30' : 'bg-[var(--surface-2)] text-[var(--text-muted)] border border-[var(--hairline)]'}">
          {schedulerStatus.running ? t("routines.running") : t("routines.idle")}
        </span>
      {/if}
    </div>
    <div class="flex items-center gap-2 shrink-0">
      {#if onBotChange && bots.length > 1}
        <SimpleSelect
          value={bot.id}
          options={bots.map((b: any) => ({ value: b.id, label: b.name }))}
          onValueChange={(v) => onBotChange(v)}
          class="h-7 w-40 rounded-md text-xs"
        />
      {/if}
      <Button
        size="sm"
        variant="outline"
        class="h-7 px-2.5 text-[11px] gap-1 cursor-pointer"
        onclick={() => (showCreate = !showCreate)}
      >
        <Plus class="size-3.5" />
        {t("routines.new")}
      </Button>
    </div>
  </div>

  {#if error}
    <div class="flex items-center gap-2 text-[10px] text-danger font-mono" role="alert">
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
          aria-label="Routine name"
          placeholder={t("routines.name")}
          class="h-7 px-2.5 rounded-lg bg-[var(--surface-1)] border border-[var(--hairline)] text-[11px] text-[var(--text-primary)] placeholder:text-[var(--text-muted)] focus:outline-none focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--brand)]/60 focus:border-[var(--brand)]/50"
        />
        <div class="relative">
          <input
            bind:value={newSchedule}
            aria-label="Schedule (cron expression)"
            placeholder={t("routines.schedule")}
            class="w-full h-7 px-2.5 rounded-lg bg-[var(--surface-1)] border border-[var(--hairline)] text-[11px] font-mono text-[var(--text-primary)] placeholder:text-[var(--text-muted)] focus:outline-none focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--brand)]/60 focus:border-[var(--brand)]/50"
          />
        </div>
      </div>

      <div class="flex flex-wrap gap-1">
        {#each schedulePresets as preset}
          <button
            type="button"
            aria-pressed={newSchedule === preset.value}
            class="h-5 px-1.5 rounded-md text-[9px] font-mono text-[var(--text-tertiary)] hover:text-[var(--brand-hover)] bg-[var(--surface-2)] border border-[var(--hairline)] hover:border-[var(--brand)]/40 cursor-pointer transition-colors {newSchedule === preset.value ? 'text-[var(--brand-text)] border-[var(--brand)]/50' : ''}"
            onclick={() => (newSchedule = preset.value)}
          >
            {preset.label}
          </button>
        {/each}
      </div>

      <textarea
        bind:value={newInstruction}
        aria-label="Instruction"
        placeholder={t("routines.instruction")}
        rows={2}
        class="w-full px-2.5 py-2 rounded-lg bg-[var(--surface-1)] border border-[var(--hairline)] text-[11px] text-[var(--text-primary)] placeholder:text-[var(--text-muted)] resize-none focus:outline-none focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--brand)]/60 focus:border-[var(--brand)]/50"
      ></textarea>

      <div class="flex justify-end">
        <Button
          size="sm"
          class="h-7 px-3 text-[10px] gap-1 bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-[var(--text-on-light)] cursor-pointer"
          disabled={!newName.trim() || !newInstruction.trim() || creating}
          onclick={createRoutine}
        >
          {#if creating}
            <Loader2 class="size-3 animate-spin" />
          {/if}
          {t("routines.create")}
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
      {t("routines.empty", { name: bot.name })}
    </div>
  {:else}
    <div class="space-y-1.5 max-h-56 overflow-y-auto pr-1 no-scrollbar">
      {#each routines as routine (routine.id)}
        <div class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] p-2.5 space-y-1.5">
          <div class="flex items-center justify-between gap-2">
            <div class="flex items-center gap-2 min-w-0">
              {#if routine.is_enabled}
                <CheckCircle2 class="size-3 text-success shrink-0" />
              {:else}
                <Circle class="size-3 text-[var(--text-muted)] shrink-0" />
              {/if}
              <span class="text-[11px] font-bold text-[var(--text-primary)] truncate">{routine.name}</span>
            </div>
            <div class="flex items-center gap-1 shrink-0">
              <button
                type="button"
                aria-label="Run now"
                class="size-5 rounded-md bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--brand-hover)] hover:border-[var(--brand)]/40 flex items-center justify-center cursor-pointer transition-colors"
                onclick={() => runNow(routine)}
                title={t("routines.runNow")}
              >
                <Play class="size-2.5" />
              </button>
              <button
                type="button"
                aria-label="Toggle webhook"
                aria-pressed={!!webhookEnabled[routine.id]}
                class="size-5 rounded-md border flex items-center justify-center cursor-pointer transition-colors {webhookEnabled[routine.id] ? 'bg-success/15 border-success/40 text-success' : 'bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-success hover:border-success/40'}"
                onclick={() => toggleWebhook(routine)}
                title={webhookEnabled[routine.id] ? t("routines.webhookOn") : t("routines.webhookOff")}
              >
                <Webhook class="size-2.5" />
              </button>
              <button
                type="button"
                aria-label="Delete routine"
                class="size-5 rounded-md bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-danger hover:border-danger/40 flex items-center justify-center cursor-pointer transition-colors"
                onclick={() => deleteRoutine(routine)}
                title={t("routines.delete")}
              >
                <Trash2 class="size-2.5" />
              </button>
            </div>
          </div>

          <div class="flex items-center gap-2 text-[9px] font-mono text-[var(--text-muted)]">
            <span class="px-1.5 py-0.5 rounded bg-[var(--brand-soft)] border border-[var(--brand)]/25 text-[var(--brand-text)]">
              {routine.schedule}
            </span>
            <span>{t("routines.lastRun", { when: formatLastRun(routine.last_run_at) })}</span>
          </div>

          <p class="text-[10px] text-[var(--text-tertiary)] leading-relaxed line-clamp-2">{routine.instruction}</p>

          <button
            type="button"
            aria-pressed={routine.is_enabled}
            class="text-[9px] font-mono text-[var(--text-muted)] hover:text-[var(--text-primary)] cursor-pointer transition-colors"
            onclick={() => toggleRoutine(routine)}
          >
            {routine.is_enabled ? t("routines.disable") : t("routines.enable")}
          </button>

          {#if webhookUrl?.routineId === routine.id}
            <div class="mt-1.5 rounded-lg border border-success/30 bg-success/20 p-2 space-y-1">
              <div class="text-[9px] text-success">{t("routines.webhookHint")}</div>
              <div class="flex items-center gap-1.5">
                <code class="flex-1 text-[9px] font-mono text-success break-all">{webhookUrl.url}</code>
                <button
                  type="button"
                  aria-label="Copy webhook URL"
                  class="size-5 shrink-0 rounded-md bg-success/15 border border-success/40 text-success flex items-center justify-center cursor-pointer"
                  onclick={copyWebhook}
                  title={t("routines.copyUrl")}
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
