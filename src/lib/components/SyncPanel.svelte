<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { Button } from "$lib/components/ui/button";
  import {
    Download,
    Upload,
    Loader2,
    ShieldCheck,
    ShieldAlert,
    Boxes,
    CheckCircle2,
    AlertTriangle,
  } from "@lucide/svelte";

  interface Props {
    bot: any;
    onBotImported?: (botId: string) => void;
  }

  let { bot, onBotImported }: Props = $props();

  interface BotRow {
    id: string;
    name: string;
    description: string;
    config: { model_provider: string; model_id: string };
  }

  let bots = $state<BotRow[]>([]);
  let loading = $state(true);
  let exporting = $state<string | null>(null);
  let importJson = $state("");
  let importing = $state(false);
  let importResult = $state<{ ok: boolean; message: string } | null>(null);

  onMount(async () => {
    try {
      bots = await invoke("list_bots");
    } catch (e) {
      console.error("Failed to load bots:", e);
    } finally {
      loading = false;
    }
  });

  async function exportBot(botRow: BotRow) {
    exporting = botRow.id;
    try {
      const bundle = await invoke("export_bot_bundle", {
        botId: botRow.id,
        includeMemory: true,
      });
      const json = JSON.stringify(bundle, null, 2);
      const safeName = botRow.name.replace(/[^a-zA-Z0-9._-]+/g, "_");
      const blob = new Blob([json], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `ravenbot-${safeName}-bundle.json`;
      a.click();
      URL.revokeObjectURL(url);
    } catch (e: any) {
      alert("Export failed: " + String(e));
    } finally {
      exporting = null;
    }
  }

  async function importBundle() {
    const json = importJson.trim();
    if (!json) return;
    importing = true;
    importResult = null;
    try {
      const parsed = JSON.parse(json);
      const signed = Boolean(parsed?.signature && parsed?.pubkey);
      const botId: string = await invoke("import_bot_bundle", { bundleJson: json });
      importResult = {
        ok: true,
        message: signed
          ? `Bot imported — Ed25519 signature verified (TOFU-trusted). New agent: ${botId}`
          : `Bot imported (unsigned — no authenticity proof). New agent: ${botId}`,
      };
      importJson = "";
      try {
        bots = await invoke("list_bots");
      } catch {}
      onBotImported?.(botId);
    } catch (e: any) {
      importResult = { ok: false, message: String(e) };
    } finally {
      importing = false;
    }
  }

  function providerLabel(botRow: BotRow) {
    const p = botRow.config?.model_provider || "ollama";
    const m = botRow.config?.model_id || "";
    return `${p} · ${m.split("/").pop()}`;
  }
</script>

<div class="space-y-4">
  <!-- Header -->
  <div class="flex items-center gap-2">
    <Boxes class="size-4 text-sky-400" />
    <span class="text-[11px] font-bold text-zinc-300 uppercase tracking-wider font-mono">
      Fleet Sync & Backup
    </span>
    <span class="text-[9px] font-mono px-1.5 py-0.5 rounded bg-emerald-500/10 border border-emerald-500/30 text-emerald-300 flex items-center gap-1">
      <ShieldCheck class="size-2.5" />
      Ed25519 signed
    </span>
  </div>

  <p class="text-[10px] text-zinc-500 leading-relaxed">
    Export bots as signed bundles (agent + skills + memories) to back up or move
    your fleet. Import verifies the Ed25519 signature: first import from a new
    signer is trusted on first use — later imports must carry the same key.
  </p>

  <!-- Bot list / export -->
  <div class="space-y-1.5 max-h-52 overflow-y-auto pr-1">
    {#if loading}
      <div class="flex items-center justify-center py-4 text-zinc-500">
        <Loader2 class="size-4 animate-spin" />
      </div>
    {:else}
      {#each bots as botRow (botRow.id)}
        <div class="flex items-center justify-between gap-2 rounded-xl border border-white/10 bg-[#0e0e14] p-2.5">
          <div class="min-w-0">
            <div class="flex items-center gap-1.5">
              <span class="text-[11px] font-bold text-white truncate">{botRow.name}</span>
              {#if botRow.id === bot?.id}
                <span class="text-[8px] font-mono px-1 py-0.5 rounded bg-sky-500/15 text-sky-300 border border-sky-500/30 shrink-0">current</span>
              {/if}
            </div>
            <span class="text-[9px] font-mono text-zinc-500 truncate">{providerLabel(botRow)}</span>
          </div>
          <Button
            size="sm"
            variant="outline"
            class="h-6 px-2 text-[10px] gap-1 cursor-pointer shrink-0"
            disabled={exporting === botRow.id}
            onclick={() => exportBot(botRow)}
            title="Export signed bundle (agent + skills + memories)"
          >
            {#if exporting === botRow.id}
              <Loader2 class="size-3 animate-spin" />
            {:else}
              <Download class="size-3" />
            {/if}
            Export
          </Button>
        </div>
      {/each}
    {/if}
  </div>

  <!-- Import -->
  <div class="space-y-1.5 pt-2 border-t border-white/10">
    <div class="flex items-center gap-1.5">
      <Upload class="size-3 text-sky-400" />
      <span class="text-[10px] font-bold text-zinc-300 uppercase tracking-wider font-mono">Import bundle</span>
    </div>
    <textarea
      bind:value={importJson}
      rows={3}
      placeholder='Paste a ravenbot-*.json bundle here…'
      class="w-full px-2.5 py-2 rounded-xl bg-[#0e0e14] border border-white/10 text-[10px] font-mono text-white placeholder:text-zinc-500 resize-none focus:outline-none focus:border-sky-500/50"
    ></textarea>

    {#if importResult}
      <div class="flex items-start gap-1.5 text-[10px] {importResult.ok ? 'text-emerald-400' : 'text-rose-400'}">
        {#if importResult.ok}
          <CheckCircle2 class="size-3 shrink-0 mt-0.5" />
        {:else}
          <AlertTriangle class="size-3 shrink-0 mt-0.5" />
        {/if}
        <span class="leading-relaxed">{importResult.message}</span>
      </div>
    {/if}

    <div class="flex justify-end">
      <Button
        size="sm"
        class="h-7 px-3 text-[10px] gap-1 bg-white text-black hover:bg-zinc-200 cursor-pointer"
        disabled={!importJson.trim() || importing}
        onclick={importBundle}
        title="Import and verify bundle"
      >
        {#if importing}
          <Loader2 class="size-3 animate-spin" />
        {:else}
          <ShieldCheck class="size-3" />
        {/if}
        Verify & Import
      </Button>
    </div>

    <div class="flex items-center gap-1.5 text-[9px] text-zinc-500 font-mono">
      <ShieldAlert class="size-2.5 shrink-0" />
      <span>Tampered or key-swapped bundles are rejected outright.</span>
    </div>
  </div>
</div>
