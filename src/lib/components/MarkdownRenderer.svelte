<script lang="ts">
  import { marked } from "marked";
  import { onMount } from "svelte";
  import { qualifiesAsArtifact, makeArtifact, artifactKindFor, type Artifact } from "$lib/artifact";

  interface Props {
    content: string;
    class?: string;
    onOpenArtifact?: (artifact: Artifact) => void;
  }

  let { content = "", class: customClass = "", onOpenArtifact }: Props = $props();

  let containerEl = $state<HTMLDivElement | null>(null);
  let renderedHtml = $state("");

  // Configure marked for clean, secure GFM rendering
  marked.setOptions({
    gfm: true,
    breaks: true,
  });

  $effect(() => {
    if (!content) {
      renderedHtml = "";
      return;
    }

    try {
      // Split reasoning into a collapsible panel before parsing markdown
      const { text, thinkHtml } = extractThinkBlock(content);
      // Parse markdown synchronously
      const rawHtml = marked.parse(text) as string;
      renderedHtml = (thinkHtml ? thinkHtml + "\n" : "") + rawHtml;
    } catch (e) {
      console.error("Markdown parse error:", e);
      renderedHtml = `<p>${content.replace(/</g, "&lt;").replace(/>/g, "&gt;")}</p>`;
    }
  });

  // Move any  section out of the markdown into a collapsible reasoning panel
  function extractThinkBlock(text: string): { text: string; thinkHtml: string | null } {
    const match = text.match(/([\s\S]*?)<\/think>/);
    if (!match) return { text, thinkHtml: null };
    const reasoning = match[1].trim();
    const escaped = reasoning
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;");
    const thinkHtml =
      `<details class="think-details" open>` +
      `<summary class="think-summary">🧠 Reasoning</summary>` +
      `<pre class="think-body">${escaped}</pre>` +
      `</details>`;
    return { text: text.replace(match[0], ""), thinkHtml };
  }

  // Handle copy + artifact buttons inside rendered code blocks
  function handleClick(e: MouseEvent) {
    const artifactBtn = (e.target as HTMLElement).closest(".artifact-open-btn");
    if (artifactBtn) {
      if (!onOpenArtifact) return;
      const codeBlock = artifactBtn.closest(".code-block-wrapper")?.querySelector("code");
      const codeText = codeBlock?.textContent || "";
      const lang = artifactBtn.getAttribute("data-artifact-lang") || "";
      onOpenArtifact(makeArtifact(codeText, lang));
      return;
    }

    const target = (e.target as HTMLElement).closest(".code-copy-btn");
    if (!target) return;

    const codeBlock = target.closest(".code-block-wrapper")?.querySelector("code");
    if (!codeBlock) return;

    const codeText = codeBlock.textContent || "";
    navigator.clipboard.writeText(codeText).then(() => {
      const span = target.querySelector(".copy-status-text");
      const checkIcon = target.querySelector(".copy-check-icon");
      const copyIcon = target.querySelector(".copy-default-icon");

      if (span) span.textContent = "Copied!";
      if (checkIcon) checkIcon.classList.remove("hidden");
      if (copyIcon) copyIcon.classList.add("hidden");

      setTimeout(() => {
        if (span) span.textContent = "Copy";
        if (checkIcon) checkIcon.classList.add("hidden");
        if (copyIcon) copyIcon.classList.remove("hidden");
      }, 2000);
    });
  }

  function enhanceCodeBlocks() {
    if (!containerEl) return;
    const pres = containerEl.querySelectorAll("pre:not(.enhanced)");
    pres.forEach((pre) => {
      if (pre.parentElement?.classList.contains("code-block-wrapper")) return;
      pre.classList.add("enhanced");

      const code = pre.querySelector("code");
      let lang = "code";
      if (code) {
        const langClass = Array.from(code.classList).find((c) => c.startsWith("language-"));
        if (langClass) {
          lang = langClass.replace("language-", "");
        }
      }

      const wrapper = document.createElement("div");
      wrapper.className = "code-block-wrapper my-3 rounded-xl overflow-hidden border border-white/10 bg-[#08080c] shadow-lg text-xs";

      const artifactBtn = onOpenArtifact && code && qualifiesAsArtifact(code.textContent || "")
        ? `<button type="button" class="artifact-open-btn flex items-center gap-1 text-[10px] text-sky-400 hover:text-sky-300 px-2 py-0.5 rounded hover:bg-sky-500/10 transition-colors cursor-pointer" data-artifact-lang="${lang}" title="Open in artifact panel (canvas)">
            <svg class="size-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M15 3h6v6"/><path d="M10 14 21 3"/><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/></svg>
            <span>Artifact</span>
          </button>`
        : "";

      const header = document.createElement("div");
      header.className = "flex items-center justify-between px-3.5 py-1.5 bg-[#0f0f16] border-b border-white/10 text-[11px] font-mono text-zinc-400";
      header.innerHTML = `
        <span class="font-bold text-zinc-300 uppercase tracking-wider text-[10px]">${lang}</span>
        <div class="flex items-center gap-1">
          ${artifactBtn}
          <button type="button" class="code-copy-btn flex items-center gap-1 text-[10px] text-zinc-400 hover:text-white px-2 py-0.5 rounded hover:bg-white/10 transition-colors cursor-pointer" title="Copy code">
            <svg class="copy-default-icon size-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>
            <svg class="copy-check-icon size-3 hidden text-emerald-400" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><polyline points="20 6 9 17 4 12"/></svg>
            <span class="copy-status-text">Copy</span>
          </button>
        </div>
      `;

      pre.parentNode?.insertBefore(wrapper, pre);
      wrapper.appendChild(header);
      wrapper.appendChild(pre);

      pre.className = "p-3.5 overflow-x-auto text-[11.5px] font-mono leading-relaxed text-zinc-200 bg-transparent";
    });
  }

  onMount(() => {
    enhanceCodeBlocks();
  });

  $effect(() => {
    renderedHtml;
    setTimeout(enhanceCodeBlocks, 10);
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  bind:this={containerEl}
  onclick={handleClick}
  class="markdown-content text-zinc-200 text-xs leading-relaxed font-sans select-text {customClass}"
>
  {@html renderedHtml}
</div>

<style>
  :global(.markdown-content p) {
    margin-bottom: 0.65rem;
    line-height: 1.6;
  }
  :global(.markdown-content p:last-child) {
    margin-bottom: 0;
  }
  :global(.markdown-content h1) {
    font-size: 1.15rem;
    font-weight: 800;
    color: #ffffff;
    margin-top: 1rem;
    margin-bottom: 0.5rem;
    letter-spacing: -0.01em;
  }
  :global(.markdown-content h2) {
    font-size: 1rem;
    font-weight: 700;
    color: #ffffff;
    margin-top: 0.85rem;
    margin-bottom: 0.4rem;
  }
  :global(.markdown-content h3) {
    font-size: 0.875rem;
    font-weight: 600;
    color: #f4f4f5;
    margin-top: 0.75rem;
    margin-bottom: 0.35rem;
  }
  :global(.markdown-content ul) {
    list-style-type: disc;
    padding-left: 1.25rem;
    margin-bottom: 0.65rem;
  }
  :global(.markdown-content ol) {
    list-style-type: decimal;
    padding-left: 1.25rem;
    margin-bottom: 0.65rem;
  }
  :global(.markdown-content li) {
    margin-bottom: 0.25rem;
    line-height: 1.5;
  }
  :global(.markdown-content code:not(pre code)) {
    font-family: var(--font-mono, monospace);
    font-size: 0.725rem;
    padding: 0.15rem 0.35rem;
    border-radius: 0.35rem;
    background-color: rgba(255, 255, 255, 0.08);
    color: #38bdf8;
    border: 1px solid rgba(255, 255, 255, 0.08);
  }
  :global(.markdown-content blockquote) {
    border-left: 2px solid #38bdf8;
    background: rgba(56, 189, 248, 0.04);
    padding: 0.4rem 0.8rem;
    border-radius: 0 0.5rem 0.5rem 0;
    margin: 0.65rem 0;
    color: #a1a1aa;
    font-style: italic;
  }
  :global(.markdown-content hr) {
    border: 0;
    border-top: 1px solid rgba(255, 255, 255, 0.1);
    margin: 1rem 0;
  }
  :global(.markdown-content a) {
    color: #38bdf8;
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  :global(.markdown-content a:hover) {
    color: #7dd3fc;
  }
  :global(.think-details) {
    margin-bottom: 0.75rem;
    border: 1px solid rgba(99, 102, 241, 0.3);
    background: rgba(99, 102, 241, 0.06);
    border-radius: 0.75rem;
    overflow: hidden;
  }
  :global(.think-summary) {
    cursor: pointer;
    padding: 0.5rem 0.8rem;
    font-family: var(--font-mono, monospace);
    font-size: 11px;
    font-weight: 600;
    color: #a5b4fc;
    user-select: none;
    list-style: none;
  }
  :global(.think-summary::-webkit-details-marker) {
    display: none;
  }
  :global(.think-body) {
    padding: 0.6rem 0.9rem;
    margin: 0;
    border-top: 1px solid rgba(99, 102, 241, 0.2);
    white-space: pre-wrap;
    word-break: break-word;
    font-family: var(--font-mono, monospace);
    font-size: 10.5px;
    line-height: 1.55;
    color: #c7d2fe;
    background: rgba(0, 0, 0, 0.25);
  }
  :global(.markdown-content table) {
    width: 100%;
    border-collapse: collapse;
    margin: 0.75rem 0;
    font-size: 0.75rem;
  }
  :global(.markdown-content img) {
    max-width: 100%;
    border-radius: 0.75rem;
    border: 1px solid rgba(255, 255, 255, 0.12);
    margin: 0.5rem 0;
    background: #101016;
  }
  :global(.markdown-content th) {
    border: 1px solid rgba(255, 255, 255, 0.15);
    background-color: rgba(255, 255, 255, 0.05);
    padding: 0.4rem 0.65rem;
    text-align: left;
    font-weight: 600;
    color: #ffffff;
  }
  :global(.markdown-content td) {
    border: 1px solid rgba(255, 255, 255, 0.1);
    padding: 0.35rem 0.65rem;
  }
</style>
