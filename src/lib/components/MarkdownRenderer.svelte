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

  // Move any <think>…</think> section out of the markdown into a collapsible
  // reasoning panel (native reasoning and external-engine reasoning both use it).
  function extractThinkBlock(text: string): { text: string; thinkHtml: string | null } {
    const match = text.match(/<think>([\s\S]*?)<\/think>/i);
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
      wrapper.className = "code-block-wrapper my-3 rounded-xl overflow-hidden border border-[var(--hairline)] bg-[var(--surface-0)] shadow-lg text-xs";

      const artifactBtn = onOpenArtifact && code && qualifiesAsArtifact(code.textContent || "")
        ? `<button type="button" class="artifact-open-btn flex items-center gap-1 text-[10px] text-[var(--brand-text)] hover:text-[var(--brand-hover)] px-2 py-0.5 rounded hover:bg-[var(--brand-soft)] transition-colors cursor-pointer" data-artifact-lang="${lang}" title="Open in artifact panel (canvas)">
            <svg class="size-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M15 3h6v6"/><path d="M10 14 21 3"/><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/></svg>
            <span>Artifact</span>
          </button>`
        : "";

      const header = document.createElement("div");
      header.className = "flex items-center justify-between px-3.5 py-1.5 bg-[#0f0f16] border-[var(--hairline)] border-[var(--hairline)] text-[11px] font-mono text-[var(--text-tertiary)]";
      header.innerHTML = `
        <span class="font-bold text-[var(--text-secondary)] uppercase tracking-wider text-[10px]">${lang}</span>
        <div class="flex items-center gap-1">
          ${artifactBtn}
          <button type="button" class="code-copy-btn flex items-center gap-1 text-[10px] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] px-2 py-0.5 rounded hover:bg-[var(--surface-3)] transition-colors cursor-pointer" title="Copy code">
            <svg class="copy-default-icon size-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>
            <svg class="copy-check-icon size-3 hidden text-success" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><polyline points="20 6 9 17 4 12"/></svg>
            <span class="copy-status-text">Copy</span>
          </button>
        </div>
      `;

      pre.parentNode?.insertBefore(wrapper, pre);
      wrapper.appendChild(header);
      wrapper.appendChild(pre);

      pre.className = "p-3.5 overflow-x-auto text-[11.5px] font-mono leading-relaxed text-[var(--text-secondary)] bg-transparent";
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
  class="markdown-content text-[var(--text-secondary)] text-xs leading-relaxed font-sans select-text {customClass}"
>
  {@html renderedHtml}
</div>

