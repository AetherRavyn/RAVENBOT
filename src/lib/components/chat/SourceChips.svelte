<script lang="ts">
  /**
   * Source chips shared by a 1:1 thread and an office lane.
   *
   * A research answer without its citations in the main surface is a claim the
   * reader has to take on trust. The URLs are the same here as in the tool
   * result; only the presentation is shared so the office cannot quietly lose
   * what the 1:1 chat already shows.
   */
  import { Globe } from "@lucide/svelte";
  import { t } from "$lib/i18n";

  interface Source {
    url: string;
    title?: string;
    snippet?: string;
  }

  let { sources = [] }: { sources: Source[] } = $props();

  async function openSource(url: string) {
    try {
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(url);
    } catch (error) {
      console.error("Failed to open source:", error);
    }
  }

  function domainOf(url: string): string {
    try {
      return new URL(url).hostname.replace(/^www\./, "");
    } catch {
      return url;
    }
  }
</script>

{#if sources.length > 0}
  <div class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] p-2.5 space-y-1.5">
    <div class="flex items-center gap-1.5 text-[11px] font-mono text-[var(--text-tertiary)] uppercase tracking-wider">
      <Globe class="size-3 text-[var(--brand-text)]" />
      <span>{t("thread.sources", { count: sources.length })}</span>
    </div>
    <div class="flex flex-wrap gap-1.5">
      {#each sources as src, idx (`${src.url}:${idx}`)}
        <button
          type="button"
          class="max-w-[220px] h-6 px-2 rounded-lg bg-[var(--surface-2)] border border-[var(--hairline)] hover:bg-[var(--brand-soft)] hover:border-[var(--brand)]/40 flex items-center gap-1.5 text-[11px] text-[var(--text-secondary)] hover:text-[var(--brand-hover)] transition-colors cursor-pointer"
          title={src.title || src.url}
          onclick={() => void openSource(src.url)}
        >
          <span class="size-3.5 rounded bg-[var(--brand-soft)] text-[var(--brand-text)] font-mono flex items-center justify-center text-[11px] shrink-0">{idx + 1}</span>
          <span class="truncate font-mono">{domainOf(src.url)}</span>
        </button>
      {/each}
    </div>
  </div>
{/if}
