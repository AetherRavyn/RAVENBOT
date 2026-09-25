<script lang="ts">
  import { marked } from "marked";
  import { onMount } from "svelte";
  import { t } from "$lib/i18n";
  import { qualifiesAsArtifact, makeArtifact, artifactKindFor, type Artifact } from "$lib/artifact";

  interface Props {
    content: string;
    class?: string;
    /** Last revealed word — appended with the OpenBot blur-in tail animation. */
    streamTail?: string;
    onOpenArtifact?: (artifact: Artifact) => void;
  }

  let { content = "", class: customClass = "", streamTail = "", onOpenArtifact }: Props = $props();

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
      `<summary class="think-summary">${t("markdown.reasoning")}</summary>` +
      `<pre class="think-body">${escaped}</pre>` +
      `</details>`;
    return { text: text.replace(match[0], ""), thinkHtml };
  }

  // Handle copy + artifact buttons inside rendered code blocks
  function handleClick(e: MouseEvent) {
    const artifactBtn = (e.target as HTMLElement).closest(".md-code-artifact");
    if (artifactBtn) {
      if (!onOpenArtifact) return;
      const codeBlock = artifactBtn.closest(".md-code-wrapper")?.querySelector("code");
      const codeText = codeBlock?.textContent || "";
      const lang = artifactBtn.getAttribute("data-artifact-lang") || "";
      onOpenArtifact(makeArtifact(codeText, lang));
      return;
    }

    const target = (e.target as HTMLElement).closest(".md-code-copy");
    if (!target) return;

    const codeBlock = target.closest(".md-code-wrapper")?.querySelector("code");
    if (!codeBlock) return;

    const codeText = codeBlock.textContent || "";
    navigator.clipboard.writeText(codeText).then(() => {
      const span = target.querySelector(".copy-status-text");
      const checkIcon = target.querySelector(".copy-check-icon");
      const copyIcon = target.querySelector(".copy-default-icon");

      if (span) span.textContent = t("markdown.copied");
      if (checkIcon) checkIcon.classList.remove("hidden");
      if (copyIcon) copyIcon.classList.add("hidden");

      setTimeout(() => {
        if (span) span.textContent = t("markdown.copy");
        if (checkIcon) checkIcon.classList.add("hidden");
        if (copyIcon) copyIcon.classList.remove("hidden");
      }, 2000);
    });
  }

  function enhanceCodeBlocks() {
    if (!containerEl) return;
    const pres = containerEl.querySelectorAll("pre:not(.enhanced)");
    pres.forEach((pre) => {
      if (pre.parentElement?.classList.contains("md-code-wrapper")) return;
      pre.classList.add("enhanced");

      const code = pre.querySelector("code");
      let lang = "code";
      if (code) {
        const langClass = Array.from(code.classList).find((c) => c.startsWith("language-"));
        if (langClass) {
          lang = langClass.replace("language-", "").replace(/[^a-zA-Z0-9+#._-]/g, "") || "code";
        }
      }

      const wrapper = document.createElement("div");
      wrapper.className = "md-code-wrapper my-3";

      const artifactBtn = onOpenArtifact && code && qualifiesAsArtifact(code.textContent || "")
        ? `<button type="button" class="md-code-artifact" data-artifact-lang="${lang}" title="${t("markdown.artifactTip")}">
            <svg class="size-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M15 3h6v6"/><path d="M10 14 21 3"/><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/></svg>
            <span>${t("markdown.artifact")}</span>
          </button>`
        : "";

      const header = document.createElement("div");
      header.className = "md-code-header";
      header.innerHTML = `
        <span class="md-code-lang">${lang}</span>
        <div class="md-code-actions">
          ${artifactBtn}
          <button type="button" class="md-code-copy" title="${t("markdown.copyTitle")}">
            <svg class="copy-default-icon size-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>
            <svg class="copy-check-icon size-3 hidden text-success" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><polyline points="20 6 9 17 4 12"/></svg>
            <span class="copy-status-text">${t("markdown.copy")}</span>
          </button>
        </div>
      `;

      pre.parentNode?.insertBefore(wrapper, pre);
      wrapper.appendChild(header);
      wrapper.appendChild(pre);

      pre.className = "md-code-pre";
    });
  }

  // OpenBot StreamingTailText: the newest word animates in (blur→sharp) at the
  // end of the last rendered block. Recreating the span per word retriggers it.
  // The target must be an inline-safe host: a <td> (span directly in <table>
  // is hoisted out and the tail vanishes) or a code block's <code> node.
  const TEXTY = new Set(["P", "LI", "BLOCKQUOTE", "H1", "H2", "H3", "H4", "H5", "H6"]);
  function resolveTailTarget(root: HTMLElement): HTMLElement | null {
    let node = root.querySelector(":scope > :last-child") as HTMLElement | null;
    if (!node) return root; // partial render with no wrapping block yet
    while (node) {
      if (TEXTY.has(node.tagName)) return node;
      if (node.tagName === "TABLE") {
        return node.querySelector(
          "tbody tr:last-child > td:last-child, tbody tr:last-child > th:last-child, tr:last-child > td:last-child, tr:last-child > th:last-child",
        ) as HTMLElement | null;
      }
      if (node.tagName === "PRE") return (node.querySelector("code") ?? node) as HTMLElement;
      node = node.querySelector(":scope > :last-child") as HTMLElement | null;
    }
    return null;
  }
  function applyStreamTail() {
    if (!containerEl) return;
    containerEl.querySelector(".stream-tail")?.remove();
    if (!streamTail) return;
    const target = resolveTailTarget(containerEl);
    if (!target) return;
    const span = document.createElement("span");
    span.className = "stream-tail";
    span.textContent = " " + streamTail;
    target.appendChild(span);
  }

  onMount(() => {
    enhanceCodeBlocks();
  });

  $effect(() => {
    renderedHtml;
    streamTail;
    setTimeout(() => {
      enhanceCodeBlocks();
      applyStreamTail();
    }, 0);
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  bind:this={containerEl}
  onclick={handleClick}
  class="markdown-content font-sans select-text {customClass}"
>
  {@html renderedHtml}
</div>

