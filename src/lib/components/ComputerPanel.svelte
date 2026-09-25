<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onDestroy } from "svelte";
  import { t } from "$lib/i18n";
  import {
    Monitor,
    RefreshCw,
    X,
    MousePointerClick,
    AlertTriangle,
    Loader2,
    Play,
    Square,
    ExternalLink,
    Boxes,
    ShieldCheck,
    ShieldOff,
  } from "@lucide/svelte";

  interface Props {
    botName: string;
    botId?: string;
    onClose: () => void;
  }

  let { botName, botId, onClose }: Props = $props();

  interface ScreenCapture {
    width: number;
    height: number;
    data_url: string;
    timestamp: string;
  }
  interface Capabilities {
    os: string;
    session: string;
    preview: boolean;
    host_control: boolean;
    host_control_backend: string;
    host_control_policy: "opt_in_required" | "blocked";
    docker: {
      available: boolean;
      version: string | null;
      image: string;
      image_present: boolean;
    };
  }
  interface DesktopSession {
    container: string;
    running: boolean;
    url: string | null;
    image: string;
  }

  let tab = $state<"this" | "desktop">("this");
  let capture = $state<ScreenCapture | null>(null);
  let capabilities = $state<Capabilities | null>(null);
  let desktop = $state<DesktopSession | null>(null);
  let desktopBusy = $state(false);
  let desktopError = $state<string | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let live = $state(true);
  let timer: ReturnType<typeof setInterval> | null = null;

  async function grab() {
    loading = true;
    try {
      capture = await invoke<ScreenCapture>("capture_screen");
      error = null;
    } catch (e: any) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function loadCapabilities() {
    try {
      capabilities = await invoke<Capabilities>("computer_capabilities");
    } catch (e) {
      console.error("Failed to load computer capabilities:", e);
      capabilities = null;
    }
  }

  async function loadDesktop() {
    if (!botId) return;
    try {
      desktop = await invoke<DesktopSession | null>("bot_desktop_status", { botId });
      desktopError = null;
    } catch (e: any) {
      desktopError = String(e);
    }
  }

  async function startDesktop() {
    if (!botId) return;
    desktopBusy = true;
    desktopError = null;
    try {
      desktop = await invoke<DesktopSession>("start_bot_desktop", { botId });
    } catch (e: any) {
      desktopError = String(e);
    } finally {
      desktopBusy = false;
    }
  }

  async function stopDesktop() {
    if (!botId) return;
    desktopBusy = true;
    desktopError = null;
    try {
      await invoke("stop_bot_desktop", { botId });
      desktop = null;
    } catch (e: any) {
      desktopError = String(e);
    } finally {
      desktopBusy = false;
    }
  }

  async function openDesktop() {
    if (!desktop?.url) return;
    try {
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(desktop.url);
    } catch (e) {
      console.error("Failed to open desktop:", e);
    }
  }

  $effect(() => {
    loadCapabilities();
    loadDesktop();
    grab();
    return () => {
      if (timer) clearInterval(timer);
    };
  });

  // Live preview: poll while the panel is open, on the local tab, and `live`.
  $effect(() => {
    if (timer) {
      clearInterval(timer);
      timer = null;
    }
    if (live && tab === "this") {
      timer = setInterval(grab, 1500);
    }
    return () => {
      if (timer) clearInterval(timer);
    };
  });

  onDestroy(() => {
    if (timer) clearInterval(timer);
  });

  function fmtTime(iso?: string) {
    if (!iso) return "";
    try {
      return new Date(iso).toLocaleTimeString();
    } catch {
      return "";
    }
  }
</script>

<div class="fixed inset-0 z-[60] flex items-center justify-center bg-black/60  p-4">
  <div class="modal-panel w-full max-w-4xl flex flex-col max-h-[90vh]">
    <!-- Header -->
    <div class="modal-header flex items-center justify-between shrink-0">
      <div class="flex items-center gap-2 min-w-0">
        <Monitor class="size-4 text-[var(--brand-text)] shrink-0" />
        <span class="font-bold text-sm text-[var(--text-primary)] truncate">{botName} · {t("computer.title")}</span>
        {#if tab === "this" && capture}
          <span class="text-[10px] font-mono text-[var(--text-muted)]">{capture.width}×{capture.height} · {fmtTime(capture.timestamp)}</span>
        {/if}
      </div>
      <div class="flex items-center gap-2">
        <div class="flex items-center rounded-full border border-[var(--hairline)] bg-[var(--surface-2)] p-0.5">
          <button
            type="button"
            aria-pressed={tab === 'this'}
            onclick={() => (tab = "this")}
            class="h-6 px-3 rounded-full text-[11px] font-bold cursor-pointer {tab === 'this' ? 'bg-[var(--brand-soft)] text-[var(--brand-text)]' : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)]'}"
          >
            {t("computer.thisComputer")}
          </button>
          <button
            type="button"
            aria-pressed={tab === 'desktop'}
            onclick={() => (tab = "desktop")}
            class="h-6 px-3 rounded-full text-[11px] font-bold cursor-pointer {tab === 'desktop' ? 'bg-success/20 text-success' : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)]'}"
          >
            {t("computer.isolatedDesktop")}
          </button>
        </div>
        {#if tab === "this"}
          <button
            type="button"
            aria-pressed={live}
            onclick={() => (live = !live)}
            class="h-7 px-3 rounded-full text-[11px] font-bold transition-colors cursor-pointer {live ? 'bg-[var(--brand-soft)] text-[var(--brand-text)] border border-[var(--brand)]/40' : 'bg-[var(--surface-2)] text-[var(--text-tertiary)] border border-[var(--hairline)]'}"
          >
            {live ? t("computer.live") : t("computer.paused")}
          </button>
          <button
            type="button"
            aria-label="Refresh"
            onclick={grab}
            disabled={loading}
            class="size-7 rounded-full bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] flex items-center justify-center cursor-pointer disabled:opacity-50"
            title={t("computer.captureNow")}
          >
            <RefreshCw class="size-3.5 {loading ? 'animate-spin' : ''}" />
          </button>
        {/if}
        <button
          type="button"
          aria-label="Close"
          onclick={onClose}
          class="size-7 rounded-full bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] flex items-center justify-center cursor-pointer"
          title={t("ui.close")}
        >
          <X class="size-3.5" />
        </button>
      </div>
    </div>

    <!-- Capability / policy strip -->
    <div class="px-5 py-2 border-b border-[var(--hairline)] shrink-0 space-y-1">
      {#if capabilities}
        <div class="flex flex-wrap items-center gap-x-4 gap-y-1 text-[11px]">
          <span class="flex items-center gap-1.5 {capabilities.host_control && capabilities.host_control_policy !== 'blocked' ? 'text-success' : 'text-warning'}">
            {#if capabilities.host_control_policy === "blocked"}
              <ShieldOff class="size-3.5 shrink-0" />
              <span>{t("computer.hostBlocked", { session: capabilities.session })}</span>
            {:else if capabilities.host_control}
              <ShieldCheck class="size-3.5 shrink-0" />
              <span>{t("computer.hostAvailable")} <span class="font-mono">{capabilities.host_control_backend}</span></span>
            {:else}
              <AlertTriangle class="size-3.5 shrink-0" />
              <span>{t("computer.noBackend1")}<span class="font-mono">xdotool</span>{t("computer.noBackend2")}<span class="font-mono">ydotool</span>/<span class="font-mono">wtype</span>{t("computer.noBackend3")}</span>
            {/if}
          </span>
          <span class="flex items-center gap-1.5 {capabilities.docker.available ? 'text-[var(--text-tertiary)]' : 'text-[var(--text-muted)]'}">
            <Boxes class="size-3.5 shrink-0" />
            {#if capabilities.docker.available}
              <span>Docker {capabilities.docker.version}</span>
            {:else}
              <span>{t("computer.dockerMissing")}</span>
            {/if}
          </span>
        </div>
      {/if}
    </div>

    <!-- Body -->
    {#if tab === "this"}
      <div class="flex-1 overflow-auto bg-[var(--surface-0)] flex items-center justify-center min-h-[300px] p-4">
        {#if error}
          <div class="text-center text-warning text-xs max-w-md" role="alert">
            <AlertTriangle class="size-6 mx-auto mb-2" />
            <p class="font-semibold mb-1">{t("computer.captureUnavailable")}</p>
            <p class="text-[var(--text-tertiary)]">{error}</p>
          </div>
        {:else if capture}
          <img
            src={capture.data_url}
            alt={t("computer.screenAlt")}
            class="max-w-full max-h-[68vh] rounded-lg border border-[var(--hairline)] shadow-lg object-contain"
          />
        {:else}
          <div class="flex items-center gap-2 text-[var(--text-muted)] text-xs">
            <Loader2 class="size-4 animate-spin" /> {t("computer.capturing")}
          </div>
        {/if}
      </div>
    {:else}
      <div class="flex-1 overflow-y-auto p-5 space-y-4 min-h-[300px]">
        <div class="rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-4 space-y-3">
          <div class="flex items-center gap-2">
            <Boxes class="size-4 text-success" />
            <span class="text-sm font-bold text-[var(--text-primary)]">{t("computer.isolatedDesktop")}</span>
            <span class="text-[10px] text-[var(--text-muted)]">{t("computer.desktopHint")}</span>
          </div>
          <p class="text-[11px] text-[var(--text-tertiary)] leading-relaxed">
            {t("computer.desktopDesc")}
          </p>

          {#if !capabilities?.docker.available}
            <div class="text-[11px] text-warning bg-warning/15 border border-warning/50 rounded-lg px-3 py-2">
              {t("computer.desktopDockerWarn")}
            </div>
          {/if}

          {#if desktopError}
            <div class="text-[11px] text-danger bg-danger/15 border border-danger/50 rounded-lg px-3 py-2" role="alert">{desktopError}</div>
          {/if}

          <div class="flex items-center justify-between gap-3 pt-1">
            <div class="text-[11px] text-[var(--text-tertiary)] min-w-0">
              {#if desktop?.running}
                <span class="text-success font-bold">{t("computer.running")}</span>
                <span class="font-mono text-[var(--text-muted)]"> · {desktop.container}</span>
              {:else}
                <span class="text-[var(--text-muted)]">{t("computer.stopped")}</span>
                {#if capabilities?.docker.image}
                  <span class="font-mono text-[var(--text-muted)]"> · {capabilities.docker.image}</span>
                {/if}
              {/if}
            </div>
            <div class="flex items-center gap-2 shrink-0">
              {#if desktop?.running}
                <button
                  type="button"
                  onclick={openDesktop}
                  disabled={!desktop?.url}
                  class="h-8 px-3 rounded-lg bg-success/15 border border-success/40 text-success text-xs font-bold flex items-center gap-1.5 cursor-pointer hover:bg-success/25 disabled:opacity-50"
                >
                  <ExternalLink class="size-3.5" /> {t("computer.openDesktopBtn")}
                </button>
                <button
                  type="button"
                  onclick={stopDesktop}
                  disabled={desktopBusy}
                  class="h-8 px-3 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] text-xs flex items-center gap-1.5 cursor-pointer disabled:opacity-50"
                >
                  {#if desktopBusy}<Loader2 class="size-3.5 animate-spin" />{:else}<Square class="size-3.5" />{/if}
                  {t("computer.stop")}
                </button>
              {:else}
                <button
                  type="button"
                  onclick={startDesktop}
                  disabled={desktopBusy || !capabilities?.docker.available}
                  class="h-8 px-3 rounded-lg bg-success/15 border border-success/40 text-success text-xs font-bold flex items-center gap-1.5 cursor-pointer hover:bg-success/25 disabled:opacity-50"
                >
                  {#if desktopBusy}<Loader2 class="size-3.5 animate-spin" />{:else}<Play class="size-3.5" />{/if}
                  {desktopBusy ? t("computer.starting") : t("computer.startDesktop")}
                </button>
              {/if}
            </div>
          </div>

          {#if desktop?.url}
            <p class="text-[10px] font-mono text-[var(--text-muted)] truncate">{desktop.url}</p>
          {/if}
        </div>

        <div class="rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-4 text-[11px] text-[var(--text-tertiary)] space-y-1">
          <div class="font-bold text-[var(--text-secondary)] flex items-center gap-1.5">
            <MousePointerClick class="size-3.5" /> {t("computer.hostPolicy")}
          </div>
          <p>
            {#if capabilities?.host_control_policy === "blocked"}
              {t("computer.hostPolicyBlocked1")}<span class="font-mono">RAVENBOT_ALLOW_WAYLAND_CONTROL=1</span>{t("computer.hostPolicyBlocked2")}
            {:else}
              {t("computer.hostPolicyAvailable")}
            {/if}
          </p>
        </div>
      </div>
    {/if}
  </div>
</div>
