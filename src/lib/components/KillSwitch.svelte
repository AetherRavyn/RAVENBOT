<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import * as Dialog from "$lib/components/ui/dialog";
  import { ShieldAlert, Play, AlertOctagon } from "@lucide/svelte";

  let isActive = $state(false);
  let reason = $state("");
  let showConfirm = $state(false);
  let triggerReason = $state("");
  let isSubmitting = $state(false);

  async function checkStatus() {
    try {
      const status = await invoke("get_kill_switch_status");
      isActive = (status as any).state === "Triggered";
      reason = (status as any).reason || "";
    } catch (e) {
      console.error("Failed to check kill switch status:", e);
    }
  }

  async function triggerKillSwitch() {
    isSubmitting = true;
    try {
      await invoke("trigger_kill_switch", { reason: triggerReason || "Manual trigger" });
      isActive = true;
      reason = triggerReason || "Manual trigger";
      showConfirm = false;
      triggerReason = "";
    } catch (e) {
      console.error("Failed to trigger kill switch:", e);
    } finally {
      isSubmitting = false;
    }
  }

  async function releaseKillSwitch() {
    isSubmitting = true;
    try {
      await invoke("release_kill_switch");
      isActive = false;
      reason = "";
    } catch (e) {
      console.error("Failed to release kill switch:", e);
    } finally {
      isSubmitting = false;
    }
  }

  $effect(() => {
    checkStatus();
    const interval = setInterval(checkStatus, 5000);
    return () => clearInterval(interval);
  });
</script>

<div class="flex items-center">
  {#if isActive}
    <div class="flex items-center gap-3 px-3.5 py-2 rounded-xl bg-red-950/60 border border-red-500/60 text-red-400 shadow-[0_0_20px_rgba(239,68,68,0.2)] animate-pulse">
      <ShieldAlert class="size-5 shrink-0 text-red-400" />
      <div class="flex flex-col">
        <span class="text-xs font-bold tracking-wider text-red-300">KILL SWITCH ACTIVE</span>
        {#if reason}
          <span class="text-[11px] text-red-400/80 truncate max-w-xs">{reason}</span>
        {/if}
      </div>
      <Button
        size="xs"
        variant="default"
        class="bg-emerald-600 hover:bg-emerald-500 text-white gap-1 ml-2 font-medium h-7"
        onclick={releaseKillSwitch}
        disabled={isSubmitting}
      >
        <Play class="size-3 fill-current" />
        Resume All
      </Button>
    </div>
  {:else}
    <button
      type="button"
      class="border border-red-900/60 bg-red-950/25 hover:bg-red-950/45 hover:border-red-700/80 rounded-xl px-4 py-2 flex items-center gap-3 transition-all cursor-pointer text-left focus:outline-none focus:ring-1 focus:ring-red-500/50 shadow-sm"
      onclick={() => (showConfirm = true)}
    >
      <!-- Circular Red Power / Target Icon -->
      <div class="size-8 rounded-lg bg-red-950/80 border border-red-700/50 flex items-center justify-center text-red-400 shadow-[0_0_10px_rgba(239,68,68,0.25)] shrink-0">
        <svg viewBox="0 0 24 24" class="size-4 text-red-400" fill="none" stroke="currentColor" stroke-width="2.5">
          <circle cx="12" cy="12" r="8" stroke-opacity="0.5" stroke-dasharray="3 3" />
          <circle cx="12" cy="12" r="4" fill="currentColor" fill-opacity="0.3" />
          <path d="M12 2v6M12 16v6M2 12h6M16 12h6" stroke-linecap="round" />
        </svg>
      </div>

      <div class="flex flex-col">
        <span class="text-red-400 font-bold text-sm leading-tight">Emergency Stop</span>
        <span class="text-zinc-400 text-xs mt-0.5">Halt all agents immediately</span>
      </div>
    </button>
  {/if}
</div>

<!-- Confirm Kill Switch Dialog -->
<Dialog.Root open={showConfirm} onOpenChange={(o) => (!o && (showConfirm = false))}>
  <Dialog.Content class="sm:max-w-md bg-[#0e0e16] border-[#252538]">
    <Dialog.Header class="gap-2">
      <div class="size-14 rounded-2xl bg-red-950/60 text-red-400 flex items-center justify-center mx-auto ring-8 ring-red-900/20 mb-1 border border-red-800/40">
        <AlertOctagon class="size-7" />
      </div>
      <Dialog.Title class="text-center text-lg font-bold text-white">
        Activate Sovereign Kill Switch?
      </Dialog.Title>
      <Dialog.Description class="text-center text-xs text-zinc-400">
        This will immediately suspend all running agents, terminate active tool processes, and revoke outbound network access across your entire bot fleet.
      </Dialog.Description>
    </Dialog.Header>

    <div class="space-y-2 py-2">
      <Label for="kill-reason" class="text-xs font-semibold text-zinc-400 uppercase tracking-wider">
        Reason (optional)
      </Label>
      <Input
        id="kill-reason"
        bind:value={triggerReason}
        placeholder="e.g. Suspicious command loop, manual audit..."
        class="h-9 text-xs bg-[#141420] border-[#252538]"
        onkeydown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            triggerKillSwitch();
          }
        }}
      />
    </div>

    <Dialog.Footer class="gap-2 sm:gap-0 pt-2 border-t border-[#202030]">
      <Button variant="outline" size="sm" onclick={() => (showConfirm = false)} disabled={isSubmitting}>
        Cancel
      </Button>
      <Button
        variant="destructive"
        size="sm"
        class="gap-1.5 bg-red-600 hover:bg-red-500 font-medium"
        onclick={triggerKillSwitch}
        disabled={isSubmitting}
      >
        <ShieldAlert class="size-3.5" />
        {isSubmitting ? "Stopping Fleet..." : "Confirm Emergency Stop"}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
