<script lang="ts">
  import MarkdownRenderer from "./MarkdownRenderer.svelte";
  import { t } from "$lib/i18n";
  import type { Artifact } from "$lib/artifact";
  import {
    X,
    Copy,
    Check,
    Download,
    Code,
    Eye,
    Maximize2,
    Minimize2,
  } from "@lucide/svelte";

  interface Props {
    artifact: Artifact;
    onClose: () => void;
  }

  let { artifact, onClose }: Props = $props();

  let view = $state<"code" | "preview">("code");
  let copied = $state(false);
  let expanded = $state(false);

  $effect(() => {
    // Reset view sensibly when a new artifact arrives: HTML/markdown default
    // to preview, everything else to code
    view = artifact.kind === "code" ? "code" : "preview";
    copied = false;
  });

  function copyArtifact() {
    navigator.clipboard.writeText(artifact.content).then(() => {
      copied = true;
      setTimeout(() => (copied = false), 2000);
    });
  }

  function downloadArtifact() {
    const safeTitle = (artifact.title || "artifact").replace(/[^a-zA-Z0-9._-]+/g, "_");
    const ext =
      artifact.kind === "html"
        ? "html"
        : artifact.kind === "markdown"
          ? "md"
          : (artifact.language && artifact.language !== "code" ? artifact.language : "txt");
    const blob = new Blob([artifact.content], { type: "text/plain" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `${safeTitle}.${ext}`;
    a.click();
    URL.revokeObjectURL(url);
  }

  const panelWidthClass = $derived(expanded ? "w-full" : "");
</script>

<div class="flex flex-col h-full overflow-hidden bg-[var(--surface-0)] {panelWidthClass}">
  <!-- Header -->
  <header class="h-12 px-3 border-b border-[var(--hairline)] bg-[var(--surface-0)]/95  flex items-center gap-2 shrink-0">
    <div class="min-w-0 flex-1">
      <div class="flex items-center gap-2 min-w-0">
        <span class="text-[11px] font-mono uppercase tracking-wider px-1.5 py-0.5 rounded bg-[var(--brand-soft)] border border-[var(--brand)]/30 text-[var(--brand-text)] shrink-0">
          {t("artifact.title")}
        </span>
        <span class="text-xs font-bold text-[var(--text-primary)] truncate">{artifact.title}</span>
        <span class="text-[11px] font-mono text-[var(--text-muted)] truncate hidden sm:inline">{artifact.language}</span>
      </div>
    </div>

    <!-- View tabs (code/preview) -->
    <div class="flex items-center rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] p-0.5 shrink-0">
      <button
        type="button"
        aria-pressed={view === 'code'}
        class="h-6 px-2 rounded-md text-[11px] font-mono flex items-center gap-1 cursor-pointer transition-colors {view === 'code'
          ? 'bg-[var(--surface-3)] text-[var(--text-primary)]'
          : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)]'}"
        onclick={() => (view = "code")}
        title={t("artifact.viewSource")}
      >
        <Code class="size-3" />
        <span class="hidden sm:inline">{t("artifact.code")}</span>
      </button>
      <button
        type="button"
        aria-pressed={view === 'preview'}
        class="h-6 px-2 rounded-md text-[11px] font-mono flex items-center gap-1 cursor-pointer transition-colors {view === 'preview'
          ? 'bg-[var(--surface-3)] text-[var(--text-primary)]'
          : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)]'}"
        onclick={() => (view = "preview")}
        title={t("artifact.viewPreview")}
      >
        <Eye class="size-3" />
        <span class="hidden sm:inline">{t("artifact.preview")}</span>
      </button>
    </div>

    <!-- Actions -->
    <div class="flex items-center gap-1 shrink-0">
      <button
        type="button"
        aria-label={expanded ? "Dock panel" : "Expand panel"}
        aria-pressed={expanded}
        class="size-7 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer"
        onclick={() => (expanded = !expanded)}
        title={expanded ? t("artifact.dock") : t("artifact.expand")}
      >
        {#if expanded}
          <Minimize2 class="size-3.5" />
        {:else}
          <Maximize2 class="size-3.5" />
        {/if}
      </button>

      <button
        type="button"
        aria-label="Download"
        class="size-7 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer"
        onclick={downloadArtifact}
        title={t("artifact.download")}
      >
        <Download class="size-3.5" />
      </button>

      <button
        type="button"
        aria-label="Copy"
        class="size-7 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer"
        onclick={copyArtifact}
        title={t("artifact.copy")}
      >
        {#if copied}
          <Check class="size-3.5 text-success" />
        {:else}
          <Copy class="size-3.5" />
        {/if}
      </button>

      <button
        type="button"
        aria-label="Close"
        class="size-7 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer"
        onclick={onClose}
        title={t("artifact.close")}
      >
        <X class="size-3.5" />
      </button>
    </div>
  </header>

  <!-- Body -->
  <div class="flex-1 overflow-hidden">
    {#if view === "code"}
      <div class="h-full overflow-y-auto">
        <pre class="p-4 text-[12px] font-mono leading-relaxed text-[var(--text-secondary)] whitespace-pre select-text">{artifact.content}</pre>
      </div>
    {:else if artifact.kind === "html"}
      <!-- Fully sandboxed static preview (no scripts, no forms) -->
      <div class="h-full overflow-hidden p-3">
        <iframe
          class="w-full h-full rounded-xl border border-[var(--hairline)] bg-white"
          title={t("artifact.previewTitle")}
          srcdoc={artifact.content}
          sandbox=""
        ></iframe>
      </div>
    {:else}
      <!-- Markdown preview -->
      <div class="h-full overflow-y-auto p-4">
        <MarkdownRenderer content={artifact.content} />
      </div>
    {/if}
  </div>
</div>
