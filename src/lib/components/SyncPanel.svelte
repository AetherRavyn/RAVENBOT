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

  let fileInput: HTMLInputElement | null = null;

  function openFilePicker() {
    fileInput?.click();
  }

  async function onFilePicked(e: Event) {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file) return;
    importing = true;
    importResult = null;
    try {
      const text = await file.text();
      const parsed = JSON.parse(text);
      const signed = Boolean(parsed?.signature && parsed?.pubkey);
      // Webview can't expose server-side file reads, so the file text goes
      // through the same verified JSON import path as paste-import.
      const botId: string = await invoke("import_bot_bundle", { bundleJson: text });
      importResult = {
        ok: true,
        message: signed
          ? `Bot imported from ${file.name} — Ed25519 signature verified (TOFU-trusted). New agent: ${botId}`
          : `Bot imported from ${file.name} (unsigned). New agent: ${botId}`,
      };
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
    <Boxes class="size-4 text-[var(--brand-text)]" />
    <span class="text-[11px] font-bold text-[var(--text-secondary)] uppercase tracking-wider font-mono">
      Fleet Sync & Backup
    </span>
    <span class="text-[9px] font-mono px-1.5 py-0.5 rounded bg-success/10 border border-success/30 text-success flex items-center gap-1">
      <ShieldCheck class="size-2.5" />
      Ed25519 signed
    </span>
  </div>

  <p class="text-[10px] text-[var(--text-muted)] leading-relaxed">
    Export bots as signed bundles (agent + skills + memories) to back up or move
    your fleet. Import verifies the Ed25519 signature: first import from a new
    signer is trusted on first use — later imports must carry the same key.
  </p>

  <!-- Bot list / export -->
  <div class="space-y-1.5 max-h-52 overflow-y-auto pr-1">
    {#if loading}
      <div class="flex items-center justify-center py-4 text-[var(--text-muted)]">
        <Loader2 class="size-4 animate-spin" />
      </div>
    {:else}
      {#each bots as botRow (botRow.id)}
        <div class="flex items-center justify-between gap-2 rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] p-2.5">
          <div class="min-w-0">
            <div class="flex items-center gap-1.5">
              <span class="text-[11px] font-bold text-white truncate">{botRow.name}</span>
              {#if botRow.id === bot?.id}
                <span class="text-[8px] font-mono px-1 py-0.5 rounded bg-[var(--brand-soft)] text-[var(--brand-text)] border border-[var(--brand)]/30 shrink-0">current</span>
              {/if}
            </div>
            <span class="text-[9px] font-mono text-[var(--text-muted)] truncate">{providerLabel(botRow)}</span>
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
  <div class="space-y-1.5 pt-2 border-t border-[var(--hairline)]">
    <div class="flex items-center gap-1.5">
      <Upload class="size-3 text-[var(--brand-text)]" />
      <span class="text-[10px] font-bold text-[var(--text-secondary)] uppercase tracking-wider font-mono">Import bundle</span>
    </div>
    <textarea
      bind:value={importJson}
      rows={3}
      placeholder='Paste a ravenbot-*.json bundle here…'
      class="w-full px-2.5 py-2 rounded-xl bg-[var(--surface-1)] border border-[var(--hairline)] text-[10px] font-mono text-white placeholder:text-[var(--text-muted)] resize-none focus:outline-none focus:border-[var(--brand)]/50"
    ></textarea>

    {#if importResult}
      <div class="flex items-start gap-1.5 text-[10px] {importResult.ok ? 'text-success' : 'text-danger'}">
        {#if importResult.ok}
          <CheckCircle2 class="size-3 shrink-0 mt-0.5" />
        {:else}
          <AlertTriangle class="size-3 shrink-0 mt-0.5" />
        {/if}
        <span class="leading-relaxed">{importResult.message}</span>
      </div>
    {/if}

    <input
      bind:this={fileInput}
      type="file"
      accept=".json,application/json"
      class="hidden"
      onchange={onFilePicked}
    />
    <div class="flex justify-end gap-2">
      <Button
        size="sm"
        variant="outline"
        class="h-7 px-3 text-[10px] gap-1 border-[var(--hairline)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] cursor-pointer"
        disabled={importing}
        onclick={openFilePicker}
        title="Import bundle from a .json file on disk"
      >
        <Upload class="size-3" />
        Open File
      </Button>
      <Button
        size="sm"
        class="h-7 px-3 text-[10px] gap-1 bg-[var(--surface-light)] text-[var(--text-on-light)] hover:bg-white cursor-pointer"
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

    <div class="flex items-center gap-1.5 text-[9px] text-[var(--text-muted)] font-mono">
      <ShieldAlert class="size-2.5 shrink-0" />
      <span>Tampered or key-swapped bundles are rejected outright.</span>
    </div>
  </div>
</div>
