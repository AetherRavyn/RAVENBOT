<script lang="ts">
  /**
   * The result of an `image_gen` tool call.
   *
   * A URL in a JSON trace is not a generated image. This card shows the image
   * itself, the prompt that produced it, and the provider, with an explicit
   * failed state instead of a blank box when the tool returns no URL.
   */
  import { AlertTriangle, ExternalLink, Loader2, Sparkles } from "@lucide/svelte";
  import { t } from "$lib/i18n";

  type State = "generating" | "completed" | "failed" | "interrupted";

  interface Props {
    state: State;
    prompt?: string;
    provider?: string;
    url?: string;
    size?: string;
    error?: string;
    class?: string;
  }

  let {
    state,
    prompt = "",
    provider = "",
    url = "",
    size = "",
    error = "",
    class: cls = "",
  }: Props = $props();

  function openImage() {
    if (!url) return;
    window.dispatchEvent(
      new CustomEvent("ravenbot:image", {
        detail: {
          images: [{ src: url, alt: prompt || t("imageGen.title") }],
          index: 0,
          src: url,
          alt: prompt || t("imageGen.title"),
        },
      }),
    );
  }
</script>

<div class={"image-gen " + cls} data-state={state}>
  <div class="image-gen__head">
    <Sparkles class="size-3.5 shrink-0" />
    <span class="image-gen__title">{t("imageGen.title")}</span>
    {#if provider}
      <span class="image-gen__meta">{provider}</span>
    {/if}
    {#if size}
      <span class="image-gen__meta">{size}</span>
    {/if}
  </div>

  {#if state === "generating"}
    <div class="image-gen__pending">
      <Loader2 class="size-4 animate-spin" />
      <span>{t("imageGen.generating")}</span>
    </div>
  {:else if state === "completed" && url}
    <button
      type="button"
      class="image-gen__figure"
      aria-label={t("imageGen.open")}
      onclick={openImage}
    >
      <img src={url} alt={prompt || t("imageGen.title")} loading="lazy" decoding="async" />
    </button>
  {:else}
    <div class="image-gen__error">
      <AlertTriangle class="size-4 shrink-0" />
      <span>
        {error ||
          (state === "interrupted" ? t("imageGen.interrupted") : t("imageGen.failed"))}
      </span>
    </div>
  {/if}

  {#if prompt}
    <p class="image-gen__prompt" title={prompt}>{prompt}</p>
  {/if}

  {#if state === "completed" && url}
    <a class="image-gen__open" href={url} target="_blank" rel="noopener noreferrer">
      <ExternalLink class="size-3" />
      {t("imageGen.open")}
    </a>
  {/if}
</div>
