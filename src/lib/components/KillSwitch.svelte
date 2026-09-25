<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
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
    <div role="alert" class="flex items-center gap-3 px-3.5 py-2 rounded-xl bg-danger/15 border border-danger/50 text-danger animate-pulse">
      <ShieldAlert class="size-5 shrink-0" />
      <div class="flex flex-col">
        <span class="text-xs font-bold tracking-wider">{t("killSwitch.active")}</span>
        {#if reason}
          <span class="text-[11px] text-danger/80 truncate max-w-xs">{reason}</span>
        {/if}
      </div>
      <Button
        size="xs"
        variant="default"
        class="bg-success/15 border border-success/40 text-success hover:bg-success/25 gap-1 ml-2 font-bold h-7"
        onclick={releaseKillSwitch}
        disabled={isSubmitting}
      >
        <Play class="size-3 fill-current" />
        {t("killSwitch.resume")}
      </Button>
    </div>
  {:else}
    <button
      type="button"
      class="group flex items-center gap-2 rounded-xl border border-danger/25 bg-danger/5 hover:bg-danger/10 hover:border-danger/50 px-3 py-1.5 transition-all duration-200 cursor-pointer focus:outline-none focus-visible:ring-2 focus-visible:ring-danger/40"
      onclick={() => (showConfirm = true)}
      title={t("killSwitch.tip")}
    >
      <span class="relative flex size-2">
        <span class="absolute inline-flex h-full w-full rounded-full bg-danger/60 opacity-0 group-hover:opacity-75 group-hover:animate-ping"></span>
        <span class="relative inline-flex size-2 rounded-full bg-danger"></span>
      </span>
      <span class="text-[11px] font-semibold tracking-wide text-danger transition-colors">{t("killSwitch.trigger")}</span>
    </button>
  {/if}
</div>

<!-- Confirm Kill Switch Dialog -->
<Dialog.Root open={showConfirm} onOpenChange={(o) => (!o && (showConfirm = false))}>
  <Dialog.Content class="sm:max-w-md bg-[var(--surface-1)] border-[var(--hairline)]">
    <Dialog.Header class="gap-2">
      <div class="size-14 rounded-2xl bg-danger/15 text-danger flex items-center justify-center mx-auto mb-1 border border-danger/40">
        <AlertOctagon class="size-7" />
      </div>
      <Dialog.Title class="text-center text-lg font-bold text-[var(--text-primary)]">
        {t("killSwitch.confirmTitle")}
      </Dialog.Title>
      <Dialog.Description class="text-center text-xs text-[var(--text-tertiary)]">
        {t("killSwitch.confirmMessage")}
      </Dialog.Description>
    </Dialog.Header>

    <div class="space-y-2 py-2">
      <Label for="kill-reason" class="text-xs font-semibold text-[var(--text-tertiary)] uppercase tracking-wider">
        {t("killSwitch.reasonLabel")}
      </Label>
      <Input
        id="kill-reason"
        bind:value={triggerReason}
        placeholder={t("killSwitch.reasonPlaceholder")}
        class="h-9 text-xs bg-[var(--surface-2)] border-[var(--hairline)]"
        onkeydown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            triggerKillSwitch();
          }
        }}
      />
    </div>

    <Dialog.Footer class="gap-2 sm:gap-0 pt-2 border-t border-[var(--hairline)]">
      <Button variant="outline" size="sm" onclick={() => (showConfirm = false)} disabled={isSubmitting}>
        {t("ui.cancel")}
      </Button>
      <Button
        variant="destructive"
        size="sm"
        class="gap-1.5 font-medium"
        onclick={triggerKillSwitch}
        disabled={isSubmitting}
      >
        <ShieldAlert class="size-3.5" />
        {isSubmitting ? t("killSwitch.stopping") : t("killSwitch.confirmStop")}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
