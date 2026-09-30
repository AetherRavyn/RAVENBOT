<script lang="ts">
  import MarkdownRenderer from "./MarkdownRenderer.svelte";
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

<div class="flex flex-col h-full overflow-hidden bg-[#07070a] {panelWidthClass}">
  <!-- Header -->
  <header class="h-12 px-3 border-b border-[#1c1c24] bg-[#09090d]/95 backdrop-blur-md flex items-center gap-2 shrink-0">
    <div class="min-w-0 flex-1">
      <div class="flex items-center gap-2 min-w-0">
        <span class="text-[9px] font-mono uppercase tracking-wider px-1.5 py-0.5 rounded bg-sky-500/15 border border-sky-500/30 text-sky-300 shrink-0">
          Artifact
        </span>
        <span class="text-xs font-bold text-white truncate">{artifact.title}</span>
        <span class="text-[9px] font-mono text-zinc-500 truncate hidden sm:inline">{artifact.language}</span>
      </div>
    </div>

    <!-- View tabs (code/preview) -->
    <div class="flex items-center rounded-lg border border-white/10 bg-white/5 p-0.5 shrink-0">
      <button
        type="button"
        class="h-6 px-2 rounded-md text-[10px] font-mono flex items-center gap-1 cursor-pointer transition-colors {view === 'code'
          ? 'bg-white/10 text-white'
          : 'text-zinc-400 hover:text-white'}"
        onclick={() => (view = "code")}
        title="View source"
      >
        <Code class="size-3" />
        <span class="hidden sm:inline">Code</span>
      </button>
      <button
        type="button"
        class="h-6 px-2 rounded-md text-[10px] font-mono flex items-center gap-1 cursor-pointer transition-colors {view === 'preview'
          ? 'bg-white/10 text-white'
          : 'text-zinc-400 hover:text-white'}"
        onclick={() => (view = "preview")}
        title="View rendered preview"
      >
        <Eye class="size-3" />
        <span class="hidden sm:inline">Preview</span>
      </button>
    </div>

    <!-- Actions -->
    <div class="flex items-center gap-1 shrink-0">
      <button
        type="button"
        class="size-7 rounded-lg border border-white/10 bg-white/5 flex items-center justify-center text-zinc-400 hover:text-white hover:border-zinc-500 transition-colors cursor-pointer"
        onclick={() => (expanded = !expanded)}
        title={expanded ? "Dock panel" : "Expand panel"}
      >
        {#if expanded}
          <Minimize2 class="size-3.5" />
        {:else}
          <Maximize2 class="size-3.5" />
        {/if}
      </button>

      <button
        type="button"
        class="size-7 rounded-lg border border-white/10 bg-white/5 flex items-center justify-center text-zinc-400 hover:text-white hover:border-zinc-500 transition-colors cursor-pointer"
        onclick={downloadArtifact}
        title="Download artifact"
      >
        <Download class="size-3.5" />
      </button>

      <button
        type="button"
        class="size-7 rounded-lg border border-white/10 bg-white/5 flex items-center justify-center text-zinc-400 hover:text-white hover:border-zinc-500 transition-colors cursor-pointer"
        onclick={copyArtifact}
        title="Copy artifact"
      >
        {#if copied}
          <Check class="size-3.5 text-emerald-400" />
        {:else}
          <Copy class="size-3.5" />
        {/if}
      </button>

      <button
        type="button"
        class="size-7 rounded-lg border border-white/10 bg-white/5 flex items-center justify-center text-zinc-400 hover:text-white hover:border-zinc-500 transition-colors cursor-pointer"
        onclick={onClose}
        title="Close artifact"
      >
        <X class="size-3.5" />
      </button>
    </div>
  </header>

  <!-- Body -->
  <div class="flex-1 overflow-hidden">
    {#if view === "code"}
      <div class="h-full overflow-y-auto">
        <pre class="p-4 text-[11.5px] font-mono leading-relaxed text-zinc-200 whitespace-pre select-text">{artifact.content}</pre>
      </div>
    {:else if artifact.kind === "html"}
      <!-- Fully sandboxed static preview (no scripts, no forms) -->
      <div class="h-full overflow-hidden p-3">
        <iframe
          class="w-full h-full rounded-xl border border-white/10 bg-white"
          title="Artifact preview"
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
