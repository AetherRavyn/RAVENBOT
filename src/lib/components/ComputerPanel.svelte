<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onDestroy } from "svelte";
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

<div class="fixed inset-0 z-[60] flex items-center justify-center bg-[var(--surface-2)]lack/60  p-4">
  <div class="modal-panel w-full max-w-4xl flex flex-col max-h-[90vh]">
    <!-- Header -->
    <div class="modal-header flex items-center justify-between shrink-0">
      <div class="flex items-center gap-2 min-w-0">
        <Monitor class="size-4 text-[var(--brand-text)] shrink-0" />
        <span class="font-bold text-sm text-white truncate">{botName} · Computer</span>
        {#if tab === "this" && capture}
          <span class="text-[10px] font-mono text-[var(--text-muted)]">{capture.width}×{capture.height} · {fmtTime(capture.timestamp)}</span>
        {/if}
      </div>
      <div class="flex items-center gap-2">
        <div class="flex items-center rounded-full border border-[var(--hairline)] bg-[var(--surface-2)] p-0.5">
          <button
            type="button"
            onclick={() => (tab = "this")}
            class="h-6 px-3 rounded-full text-[11px] font-bold cursor-pointer {tab === 'this' ? 'bg-[var(--brand-soft)] text-[var(--brand-text)]' : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)]'}"
          >
            This computer
          </button>
          <button
            type="button"
            onclick={() => (tab = "desktop")}
            class="h-6 px-3 rounded-full text-[11px] font-bold cursor-pointer {tab === 'desktop' ? 'bg-success/20 text-success' : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)]'}"
          >
            Isolated desktop
          </button>
        </div>
        {#if tab === "this"}
          <button
            type="button"
            onclick={() => (live = !live)}
            class="h-7 px-3 rounded-full text-[11px] font-bold transition-colors cursor-pointer {live ? 'bg-[var(--brand-soft)] text-[var(--brand-text)] border border-[var(--brand)]/40' : 'bg-[var(--surface-2)] text-[var(--text-tertiary)] border border-[var(--hairline)]'}"
          >
            {live ? "Live" : "Paused"}
          </button>
          <button
            type="button"
            onclick={grab}
            disabled={loading}
            class="size-7 rounded-full bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] flex items-center justify-center cursor-pointer disabled:opacity-50"
            title="Capture now"
          >
            <RefreshCw class="size-3.5 {loading ? 'animate-spin' : ''}" />
          </button>
        {/if}
        <button
          type="button"
          onclick={onClose}
          class="size-7 rounded-full bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] flex items-center justify-center cursor-pointer"
          title="Close"
        >
          <X class="size-3.5" />
        </button>
      </div>
    </div>

    <!-- Capability / policy strip -->
    <div class="px-5 py-2 border-[var(--hairline)] border-[var(--hairline)] shrink-0 space-y-1">
      {#if capabilities}
        <div class="flex flex-wrap items-center gap-x-4 gap-y-1 text-[11px]">
          <span class="flex items-center gap-1.5 {capabilities.host_control && capabilities.host_control_policy !== 'blocked' ? 'text-success' : 'text-warning'}">
            {#if capabilities.host_control_policy === "blocked"}
              <ShieldOff class="size-3.5 shrink-0" />
              <span>Host control blocked ({capabilities.session} safety gate)</span>
            {:else if capabilities.host_control}
              <ShieldCheck class="size-3.5 shrink-0" />
              <span>Host control available — requires per-agent opt-in · <span class="font-mono">{capabilities.host_control_backend}</span></span>
            {:else}
              <AlertTriangle class="size-3.5 shrink-0" />
              <span>No input backend. Install <span class="font-mono">xdotool</span> (X11) or <span class="font-mono">ydotool</span>/<span class="font-mono">wtype</span> (Wayland).</span>
            {/if}
          </span>
          <span class="flex items-center gap-1.5 {capabilities.docker.available ? 'text-[var(--text-tertiary)]' : 'text-[var(--text-muted)]'}">
            <Boxes class="size-3.5 shrink-0" />
            {#if capabilities.docker.available}
              <span>Docker {capabilities.docker.version}</span>
            {:else}
              <span>Docker not installed — isolated desktops unavailable</span>
            {/if}
          </span>
        </div>
      {/if}
    </div>

    <!-- Body -->
    {#if tab === "this"}
      <div class="flex-1 overflow-auto bg-[var(--surface-2)]lack flex items-center justify-center min-h-[300px] p-4">
        {#if error}
          <div class="text-center text-warning text-xs max-w-md">
            <AlertTriangle class="size-6 mx-auto mb-2" />
            <p class="font-semibold mb-1">Screen capture unavailable</p>
            <p class="text-[var(--text-tertiary)]">{error}</p>
          </div>
        {:else if capture}
          <img
            src={capture.data_url}
            alt="Current screen"
            class="max-w-full max-h-[68vh] rounded-lg border border-[var(--hairline)] shadow-lg object-contain"
          />
        {:else}
          <div class="flex items-center gap-2 text-[var(--text-muted)] text-xs">
            <Loader2 class="size-4 animate-spin" /> Capturing screen…
          </div>
        {/if}
      </div>
    {:else}
      <div class="flex-1 overflow-y-auto p-5 space-y-4 min-h-[300px]">
        <div class="rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-4 space-y-3">
          <div class="flex items-center gap-2">
            <Boxes class="size-4 text-success" />
            <span class="text-sm font-bold text-white">Isolated desktop</span>
            <span class="text-[10px] text-[var(--text-muted)]">a disposable Linux desktop for this agent only</span>
          </div>
          <p class="text-[11px] text-[var(--text-tertiary)] leading-relaxed">
            Runs in Docker with a dedicated workspace volume, bounded CPU/RAM, and its noVNC port
            published on loopback only. The workspace survives stop/start; the container is rebuilt
            on demand.
          </p>

          {#if !capabilities?.docker.available}
            <div class="text-[11px] text-warning bg-warning/30 border border-warning/20 rounded-lg px-3 py-2">
              Docker isn't available on this machine. Install Docker Engine to use isolated desktops —
              screen preview and host control still work.
            </div>
          {/if}

          {#if desktopError}
            <div class="text-[11px] text-danger bg-danger/30 border border-danger/20 rounded-lg px-3 py-2">{desktopError}</div>
          {/if}

          <div class="flex items-center justify-between gap-3 pt-1">
            <div class="text-[11px] text-[var(--text-tertiary)] min-w-0">
              {#if desktop?.running}
                <span class="text-success font-bold">Running</span>
                <span class="font-mono text-[var(--text-muted)]"> · {desktop.container}</span>
              {:else}
                <span class="text-[var(--text-muted)]">Stopped</span>
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
                  class="h-8 px-3 rounded-lg bg-success hover:bg-success text-white text-xs font-medium flex items-center gap-1.5 cursor-pointer disabled:opacity-50"
                >
                  <ExternalLink class="size-3.5" /> Open desktop
                </button>
                <button
                  type="button"
                  onclick={stopDesktop}
                  disabled={desktopBusy}
                  class="h-8 px-3 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] text-xs flex items-center gap-1.5 cursor-pointer disabled:opacity-50"
                >
                  {#if desktopBusy}<Loader2 class="size-3.5 animate-spin" />{:else}<Square class="size-3.5" />{/if}
                  Stop
                </button>
              {:else}
                <button
                  type="button"
                  onclick={startDesktop}
                  disabled={desktopBusy || !capabilities?.docker.available}
                  class="h-8 px-3 rounded-lg bg-success hover:bg-success text-white text-xs font-medium flex items-center gap-1.5 cursor-pointer disabled:opacity-50"
                >
                  {#if desktopBusy}<Loader2 class="size-3.5 animate-spin" />{:else}<Play class="size-3.5" />{/if}
                  {desktopBusy ? "Starting…" : "Start desktop"}
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
            <MousePointerClick class="size-3.5" /> Host control policy
          </div>
          <p>
            {#if capabilities?.host_control_policy === "blocked"}
              Blocked on this Wayland session. Set <span class="font-mono">RAVENBOT_ALLOW_WAYLAND_CONTROL=1</span> before launch to opt in, or use the isolated desktop.
            {:else}
              Available with per-agent opt-in. Enable “Control this computer” in the agent's Model &amp; Engine settings.
            {/if}
          </p>
        </div>
      </div>
    {/if}
  </div>
</div>
