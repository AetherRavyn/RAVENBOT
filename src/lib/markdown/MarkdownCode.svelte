<script lang="ts">
  /**
   * A fenced code block: language chip, copy, and the artifact hand-off.
   *
   * This used to be a `marked` `<pre>` that `enhanceCodeBlocks` walked the DOM
   * to find, wrap in a header, and give a copy button — on every re-render,
   * for every block in the message. Here the header is part of the component,
   * so it exists the moment the block does, and the copy handler closes over
   * the text instead of doing a `closest(".md-code-wrapper code")` lookup that
   * breaks the moment the markup changes.
   */
  import { Check, Copy, Expand } from "@lucide/svelte";  import { t } from "$lib/i18n";
  import { qualifiesAsArtifact } from "$lib/artifact";
  import { hl, hlLanguage } from "$lib/highlight";

  interface Props {
    code: string;
    language: string;
    streaming?: boolean;
    onOpenArtifact?: (code: string, lang: string) => void;
  }

  let { code, language, streaming = false, onOpenArtifact }: Props = $props();

  let copied = $state(false);
  let copyTimer: ReturnType<typeof setTimeout> | null = null;

  const display = $derived(language || "text");
  const canOpen = $derived(Boolean(onOpenArtifact) && !streaming && qualifiesAsArtifact(code));

  // Highlighting is skipped while the block is still arriving: the result
  // changes on every token, and re-tokenising per keystroke is the most
  // expensive thing this component could do.
  const tokens = $derived(streaming ? [{ type: "text" as const, text: code }] : hl(code, hlLanguage(language)));

  async function copy() {
    try {
      await navigator.clipboard.writeText(code);
      copied = true;
      if (copyTimer) clearTimeout(copyTimer);
      copyTimer = setTimeout(() => (copied = false), 2000);
    } catch {
      // Permission denied: leave the button at rest rather than showing a
      // checkmark for a copy that never happened.
    }
  }

  function open() {
    if (!onOpenArtifact || !canOpen) return;
    onOpenArtifact(code, language);
  }

  $effect(() => {
    return () => {
      if (copyTimer) clearTimeout(copyTimer);
    };
  });
</script>

<div class="md-code">
  <div class="md-code-header">
    <span class="md-code-lang">{display}</span>
    <div class="md-code-actions">
      {#if canOpen}
        <button type="button" class="md-code-btn" title={t("markdown.artifactTip")} onclick={open}>
          <Expand class="size-3" />
          <span>{t("markdown.artifact")}</span>
        </button>
      {/if}
      <button
        type="button"
        class="md-code-btn"
        title={t("markdown.copyTitle")}
        aria-label={copied ? t("markdown.copied") : t("markdown.copy")}
        onclick={copy}
      >
        {#if copied}
          <Check class="size-3 text-[var(--success-text)]" />
        {:else}
          <Copy class="size-3" />
        {/if}
        <span>{copied ? t("markdown.copied") : t("markdown.copy")}</span>
      </button>
    </div>
  </div>
  <pre class="md-code-pre"><code class="language-{display}">{#each tokens as tok, i (i)}{#if tok.type === "text"}{tok.text}{:else if tok.type === "newline"}{"\n"}{:else}<span class="tok-{tok.type}">{tok.text}</span>{/if}{/each}</code></pre>
</div>
