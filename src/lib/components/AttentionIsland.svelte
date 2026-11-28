<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
  import { t } from "$lib/i18n";
  import RavenAvatar from "$lib/components/RavenAvatar.svelte";
  import {
    islandAgentName,
    islandDotTone,
    islandMood,
    latestIslandItem,
    type IslandAttentionSummary,
    type IslandDotTone,
  } from "$lib/attentionIsland";
  import { ArrowUpRight, X } from "@lucide/svelte";

  let summary = $state<IslandAttentionSummary>({
    approvals: 0,
    questions: 0,
    active_runs: 0,
    items: [],
  });
  let error = $state("");
  let timer: ReturnType<typeof setInterval> | null = null;

  const DOT_CLASS: Record<IslandDotTone, string> = {
    danger: "bg-[var(--danger-text)]",
    warning: "bg-warning",
    info: "bg-info",
    success: "bg-success",
  };

  let waiting = $derived(summary.approvals + summary.questions);
  let latest = $derived(latestIslandItem(summary.items));
  let agentName = $derived(islandAgentName(latest));
  let mood = $derived(islandMood(latest));
  let dotTone = $derived(islandDotTone(latest, waiting));

  async function load() {
    try {
      summary = await invoke<IslandAttentionSummary>("get_attention_summary");
      error = "";
    } catch (cause) {
      error = String(cause);
    }
  }

  async function openHost() {
    try {
      await invoke("open_main_window");
      await getCurrentWindow().hide();
    } catch (cause) {
      error = String(cause);
    }
  }

  async function hide() {
    try {
      await getCurrentWindow().hide();
    } catch (cause) {
      error = String(cause);
    }
  }

  function onVisible() {
    if (!document.hidden) void load();
  }

  onMount(() => {
    // The island window is transparent; the shared app shell paints a
    // background, so clear it for this window only.
    document.documentElement.style.background = "transparent";
    document.body.style.background = "transparent";
    void load();
    timer = setInterval(() => {
      if (!document.hidden) void load();
    }, 2000);
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      if (timer) clearInterval(timer);
      document.removeEventListener("visibilitychange", onVisible);
      document.documentElement.style.background = "";
      document.body.style.background = "";
    };
  });
</script>

<div class="flex h-screen w-full items-center justify-center p-1">
<div
  class="flex h-14 w-full items-center gap-2 rounded-full border border-[var(--hairline-strong)] bg-[var(--surface-1)]/95 px-3 shadow-[var(--shadow-lg)] backdrop-blur"
  role="region"
  aria-label={t("island.title")}
>
  {#if latest}
    <span class="relative inline-flex shrink-0" data-island-avatar>
      <RavenAvatar
        name={agentName || "RAVENBOT"}
        imageUrl={latest.avatar_url}
        style={latest.avatar_style}
        {mood}
        class="size-8"
        decorative
      />
      <span
        class="absolute -bottom-0.5 -right-0.5 size-2.5 rounded-full ring-2 ring-[var(--surface-1)] {DOT_CLASS[dotTone]}"
        data-island-dot={dotTone}
        data-island-state={latest.state ?? "pending"}
        aria-hidden="true"
      ></span>
    </span>
  {:else}
    <span
      class="ml-1 size-2.5 shrink-0 rounded-full {DOT_CLASS[dotTone]}"
      data-island-dot={dotTone}
      aria-hidden="true"
    ></span>
  {/if}
  <div class="min-w-0 flex-1">
    <p class="truncate text-[11px] font-bold text-[var(--text-primary)]" aria-live="polite">
      {#if error}
        {t("island.error")}
      {:else if waiting > 0}
        {t("island.waiting", { count: waiting })}
      {:else}
        {t("island.clear")}
      {/if}
    </p>
    <p class="truncate text-[11px] leading-4 text-[var(--text-tertiary)]" title={latest?.detail || ""}>
      {#if latest}
        {#if agentName}
          <span class="font-semibold text-[var(--text-secondary)]">{agentName}</span>
          <span aria-hidden="true"> · </span>
        {/if}{latest.label}
      {:else}
        {t("island.hint")}
      {/if}
    </p>
  </div>
  {#if summary.active_runs > 0}
    <span class="hidden shrink-0 font-mono text-[11px] text-[var(--text-muted)] sm:inline">
      {t("island.running", { count: summary.active_runs })}
    </span>
  {/if}
  <button
    type="button"
    class="icon-btn size-8 shrink-0"
    aria-label={t("island.open")}
    title={t("island.open")}
    onclick={openHost}
  >
    <ArrowUpRight class="size-3.5" />
  </button>
  <button
    type="button"
    class="icon-btn size-8 shrink-0"
    aria-label={t("island.hide")}
    title={t("island.hide")}
    onclick={hide}
  >
    <X class="size-3.5" />
  </button>
</div>
</div>
