<script lang="ts">
  /**
   * Every tool the turn ran, in order, above the answer it produced.
   *
   * This list did not exist. The schema had `ToolCall` and `ToolResult` message
   * variants since the beginning and nothing ever wrote one — so tool calls
   * lived only as a transient line in the browser, and a reload left answers
   * with no account of how they were reached: "fixed three issues" with no
   * files, no errors, no indication that any of it happened.
   *
   * Sits between the reasoning and the bubble, so the reading order is the
   * causal order: what it was thinking, what it did, what it concluded.
   *
   * Each entry is collapsed by default for one reason — most calls are noise
   * (`memory_search` six times) and the two that matter are the ones that
   * failed or took a second. Expanding is per-entry and remembered for the
   * session, because re-opening the same entry on every render is what makes a
   * disclosure useless.
   */
  import { t } from "$lib/i18n";
  import type { ToolTrace } from "$lib/types";
  import { summarizeArgs, renderToolValue } from "$lib/chat/toolSummary";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { ChevronRight, Check, X, Timer, FileText, ExternalLink, Presentation } from "@lucide/svelte";

  interface Props {
    tools: ToolTrace[];
    class?: string;
  }

  let { tools, class: cls = "" }: Props = $props();

  function failed(tool: ToolTrace): boolean {
    return !!tool.is_error;
  }

  /** Milliseconds as human time. `1.2s` beats `1200ms` when scanning a list. */
  function duration(tool: ToolTrace): string | null {
    const ms = tool.duration_ms;
    if (ms == null || ms <= 0) return null;
    return ms < 1000 ? `${ms}ms` : `${(ms / 1000).toFixed(1)}s`;
  }

  /** Per-entry expansion, remembered so it survives a re-render. */
  const open = $state<Record<number, boolean>>({});

  function toggle(i: number) {
    open[i] = !open[i];
  }

  /** A real exported artifact, not just a JSON line in the trace. */
  function documentResult(tool: ToolTrace): { path: string; format: string } | null {
    if (tool.name === "make_slides") return null;
    const result = tool.result as any;
    const output = result?.output ?? result;
    const path = output?.path;
    if (typeof path !== "string" || !/\.(docx|pdf|pptx|html)$/i.test(path)) return null;
    return { path, format: String(output?.format ?? path.split(".").pop() ?? "") };
  }

  interface DeckImage {
    path: string;
    src: string;
    alt: string;
  }

  interface SlideDeck {
    images: DeckImage[];
    pptx: string | null;
    count: number;
    title: string;
  }

  /**
   * A designed deck is a set of rendered images, so show the images instead of
   * making the reader expand a JSON path. The PPTX remains one click away.
   */
  function slideDeck(tool: ToolTrace): SlideDeck | null {
    if (tool.name !== "make_slides") return null;
    const result = tool.result as any;
    const output = result?.output ?? result;
    if (!output || !Array.isArray(output.slides)) return null;
    const folder = typeof output.folder === "string" ? output.folder.replace(/\/+$/, "") : "";
    const images: DeckImage[] = output.slides
      .map((entry: any, index: number): DeckImage | null => {
        const direct = typeof entry === "string" ? entry : entry?.path;
        const file = typeof entry?.file === "string" ? entry.file : "";
        const path = typeof direct === "string" ? direct : folder && file ? `${folder}/${file}` : "";
        if (!path || !/\.(png|jpe?g|webp)$/i.test(path)) return null;
        return { path, src: imageSource(path), alt: `Slide ${index + 1}` };
      })
      .filter((image: DeckImage | null): image is DeckImage => image !== null)
      .slice(0, 24);
    if (!images.length) return null;
    return {
      images,
      pptx: typeof output.pptx === "string" ? output.pptx : null,
      count: Number(output.count ?? images.length) || images.length,
      title: typeof output.title === "string" ? output.title : "Designed slides",
    };
  }

  /** `convertFileSrc` needs the native shell; tests and the browser preview do not. */
  function imageSource(path: string): string {
    try {
      return convertFileSrc(path);
    } catch {
      return path;
    }
  }

  function openSlide(deck: SlideDeck, index: number) {
    window.dispatchEvent(
      new CustomEvent("ravenbot:image", {
        detail: {
          images: deck.images.map((image) => ({ src: image.src, alt: image.alt })),
          index,
          src: deck.images[index]?.src,
          alt: deck.images[index]?.alt,
        },
      }),
    );
  }

  async function openDocument(path: string) {
    try {
      const { openPath } = await import("@tauri-apps/plugin-opener");
      await openPath(path);
    } catch {
      /* browser preview or a file outside the opener's reach */
    }
  }
</script>

{#if tools.length}
  <div class={"tools " + cls} aria-label={t("tools.title")}>
    {#each tools as tool, i (i)}
      {@const bad = failed(tool)}
      {@const ms = duration(tool)}
      {@const document = documentResult(tool)}
      {@const deck = slideDeck(tool)}
      <div class="tools__item" data-failed={bad}>
        <button
          type="button"
          class="tools__head"
          aria-expanded={open[i] ?? false}
          onclick={() => toggle(i)}
        >
          <ChevronRight class="tools__chev size-3 shrink-0" strokeWidth={2} />
          {#if bad}
            <X class="tools__state size-3 shrink-0" strokeWidth={2.5} />
          {:else}
            <Check class="tools__state size-3 shrink-0" strokeWidth={2.5} />
          {/if}
          <span class="tools__name">{tool.name}</span>
          {#if summarizeArgs(tool.arguments)}
            <span class="tools__args">{summarizeArgs(tool.arguments)}</span>
          {/if}
          {#if ms}
            <span class="tools__ms">
              <Timer class="size-2.5 shrink-0" strokeWidth={2} />
              {ms}
            </span>
          {/if}
        </button>

        {#if document}
          <div class="tools__document">
            <FileText class="size-3.5 shrink-0" />
            <span class="tools__document-name" title={document.path}>{document.path.split("/").pop()}</span>
            <span class="tools__document-format">{document.format}</span>
            <button type="button" onclick={() => openDocument(document.path)}>
              <ExternalLink class="size-3" />
              Open
            </button>
          </div>
        {/if}

        {#if deck}
          <div class="tools__deck">
            <div class="tools__deck-head">
              <span class="tools__deck-title">
                <Presentation class="size-3.5 shrink-0" />
                <span>{deck.count} designed slide{deck.count === 1 ? "" : "s"}</span>
              </span>
              {#if deck.pptx}
                <button type="button" onclick={() => openDocument(deck.pptx!)}>
                  <ExternalLink class="size-3" />
                  Open PPTX
                </button>
              {/if}
            </div>
            <div class="tools__deck-strip">
              {#each deck.images as image, index (image.path)}
                <button
                  type="button"
                  class="tools__deck-slide"
                  aria-label={`Open slide ${index + 1} of ${deck.images.length}`}
                  title={image.path}
                  onclick={() => openSlide(deck, index)}
                >
                  <img src={image.src} alt={image.alt} loading="lazy" decoding="async" />
                  <span>{index + 1}</span>
                </button>
              {/each}
            </div>
          </div>
        {/if}

        {#if open[i] ?? false}
          <div class="tools__body">
            {#if summarizeArgs(tool.arguments)}
              <div class="tools__block">
                <span class="tools__label">{t("tools.input")}</span>
                <pre class="tools__pre">{renderToolValue(tool.arguments)}</pre>
              </div>
            {/if}
            {#if renderToolValue(tool.result)}
              <div class="tools__block">
                <span class="tools__label" data-failed={bad}>
                  {bad ? t("tools.failed") : t("tools.output")}
                </span>
                <pre class="tools__pre">{renderToolValue(tool.result)}</pre>
              </div>
            {/if}
          </div>
        {/if}
      </div>
    {/each}
  </div>
{/if}
