<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
  import { t } from "$lib/i18n";
  import { ArrowUpRight, X } from "@lucide/svelte";

  interface AttentionItem {
    kind: "approval" | "question";
    label: string;
    detail: string;
    thread_id: string;
    risk?: string;
  }

  interface AttentionSummary {
    approvals: number;
    questions: number;
    active_runs: number;
    items: AttentionItem[];
  }

  let summary = $state<AttentionSummary>({
    approvals: 0,
    questions: 0,
    active_runs: 0,
    items: [],
  });
  let error = $state("");
  let timer: ReturnType<typeof setInterval> | null = null;

  let waiting = $derived(summary.approvals + summary.questions);
  let latest = $derived(summary.items[0]);
  let urgent = $derived(
    summary.items.some((item) => item.risk === "high" || item.risk === "critical"),
  );
  let dotTone = $derived(
    urgent
      ? "bg-[var(--danger-text)]"
      : waiting > 0
        ? "bg-warning"
        : "bg-success",
  );

  async function load() {
    try {
      summary = await invoke<AttentionSummary>("get_attention_summary");
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
  <span class="size-2.5 shrink-0 rounded-full {dotTone}" aria-hidden="true"></span>
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
      {latest ? latest.label : t("island.hint")}
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
