<script lang="ts">
  /**
   * An interactive HTML visual reply.
   *
   * Unlike a ```html artifact, this page runs its own scripts. It is served on
   * a separate `ravenvisual://` origin inside a frame without
   * `allow-same-origin`, so it cannot read the app, its storage, or its IPC
   * bridge. The page reports its height and asks the app to open a link over
   * postMessage; both messages are validated here.
   *
   * In the browser preview (no Tauri) the page falls back to `srcdoc`, which is
   * enough for development and keeps the component testable.
   */
  import { onDestroy, onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { openExternal } from "$lib/external";
  import { isExternal, safeHref } from "$lib/markdown/safeUrl";
  import MarkdownCode from "./MarkdownCode.svelte";
  import { Code2, Eye, Maximize2, Minimize2, ShieldCheck } from "@lucide/svelte";

  interface Props {
    code: string;
    streaming?: boolean;
  }

  let { code, streaming = false }: Props = $props();

  const MIN_HEIGHT = 80;
  const MAX_HEIGHT = 2000;
  const DEFAULT_HEIGHT = 360;

  let expanded = $state(false);
  let view = $state<"visual" | "source">("visual");
  let height = $state(DEFAULT_HEIGHT);
  let frameEl = $state<HTMLIFrameElement | null>(null);
  let frameWindow: Window | null = null;
  let srcdoc = $state("");
  let visualUrl = $state<string | null>(null);
  let error = $state("");
  let registeredId: string | null = null;
  let loadToken = 0;
  let lastLinkAt = 0;
  let themeObserver: MutationObserver | null = null;

  const scriptTag = (source: string) => "<script>" + source + "</" + "script>";

  /** The page-side bridge. It reports height, forwards links, and accepts theme updates. */
  const BOOTSTRAP = String.raw`
(() => {
  const root = document.documentElement;
  const send = (payload) => {
    try { parent.postMessage(Object.assign({ __ravenbotVisual: true }, payload), "*"); } catch {}
  };
  const apply = (variables) => {
    if (!variables || typeof variables !== "object") return;
    for (const [name, value] of Object.entries(variables)) {
      if (name.startsWith("--") && typeof value === "string") root.style.setProperty(name, value);
    }
  };
  addEventListener("message", (event) => {
    if (event.source !== parent) return;
    const data = event.data;
    if (!data || typeof data !== "object") return;
    if (data.__ravenbotTheme) apply(data.variables);
    if (data.__ravenbotDisplayMode) root.dataset.displayMode = String(data.mode);
    if (data.__ravenbotRequestSize) report();
  });
  let lastHeight = -1;
  const report = () => {
    const height = Math.ceil(Math.max(
      root.getBoundingClientRect().height,
      document.body ? document.body.scrollHeight : 0
    ));
    if (height === lastHeight) return;
    lastHeight = height;
    send({ type: "size", height });
  };
  if (typeof ResizeObserver !== "undefined") {
    const observer = new ResizeObserver(report);
    observer.observe(root);
    if (document.body) observer.observe(document.body);
  }
  addEventListener("load", report);
  addEventListener("DOMContentLoaded", report);
  addEventListener("resize", report);
  setTimeout(report, 0);
  setTimeout(report, 250);

  let userActed = false;
  addEventListener("pointerdown", () => { userActed = true; }, true);
  addEventListener("keydown", () => { userActed = true; }, true);
  document.addEventListener("click", (event) => {
    if (!event.isTrusted || !(event.target instanceof Element)) return;
    const link = event.target.closest("a[href]");
    if (!link) return;
    const href = link.getAttribute("href") || "";
    if (href.startsWith("#")) {
      event.preventDefault();
      const target = document.getElementById(decodeURIComponent(href.slice(1)));
      if (target) target.scrollIntoView();
      return;
    }
    let url;
    try { url = new URL(href, document.baseURI); } catch { return; }
    if (url.protocol !== "http:" && url.protocol !== "https:") return;
    event.preventDefault();
    send({ type: "open-link", url: url.href });
  }, true);

  window.ravenbot = Object.freeze({
    requestFullscreen: () => userActed && send({ type: "display-mode", mode: "fullscreen" }),
    exitFullscreen: () => send({ type: "display-mode", mode: "inline" }),
  });
})();
`;

  const COLOR_NAMES = new Set([
    "--background",
    "--foreground",
    "--muted-foreground",
    "--muted",
    "--card",
    "--card-foreground",
    "--border",
    "--accent",
    "--success",
    "--warning",
    "--danger",
    "--brand",
    "--brand-text",
  ]);
  const HSL_TRIPLET =
    /^\d+(?:\.\d+)?\s+\d+(?:\.\d+)?%\s+\d+(?:\.\d+)?%(?:\s*\/\s*[\d.]+)?$/u;

  /** The design tokens store colour as bare HSL triplets; a page needs a real colour. */
  function asCssColor(value: string): string {
    const trimmed = value.trim();
    return HSL_TRIPLET.test(trimmed) ? `hsl(${trimmed})` : trimmed;
  }

  function themeVariables(): Record<string, string> {
    const names = [
      "--background",
      "--foreground",
      "--muted-foreground",
      "--muted",
      "--card",
      "--card-foreground",
      "--border",
      "--accent",
      "--success",
      "--warning",
      "--danger",
      "--brand",
      "--brand-text",
      "--chart-1",
      "--chart-2",
      "--chart-3",
      "--chart-other",
      "--radius",
      "--font-sans",
      "--font-mono",
    ];
    try {
      const style = getComputedStyle(document.documentElement);
      const out: Record<string, string> = {};
      for (const name of names) {
        const value = style.getPropertyValue(name).trim();
        if (!value || /[<>{};]/u.test(value)) continue;
        out[name] = COLOR_NAMES.has(name) ? asCssColor(value) : value;
      }
      const brand = style.getPropertyValue("--brand").trim();
      if (brand) out["--accent"] = asCssColor(brand);
      return out;
    } catch {
      return {};
    }
  }

  function themeCss(): string {
    const variables = themeVariables();
    const declarations = Object.entries(variables)
      .map(([name, value]) => `${name}:${value};`)
      .join("");
    return (
      ":root{" +
      "color-scheme:dark;" +
      "--background:transparent;--foreground:#f2f2f2;--muted-foreground:#9b9b9b;" +
      "--border:rgba(255,255,255,.12);--card:rgba(255,255,255,.04);--accent:#8b5cf6;" +
      "--font-sans:system-ui,sans-serif;--font-mono:ui-monospace,monospace;" +
      declarations +
      "}" +
      "html,body{margin:0;padding:0;background:transparent;color:var(--foreground);" +
      "font:14px/1.5 var(--font-sans)}" +
      "*,*::before,*::after{box-sizing:border-box}" +
      "a{color:var(--accent)}" +
      "::-webkit-scrollbar{width:8px;height:8px}" +
      "::-webkit-scrollbar-thumb{background:var(--border);border-radius:999px}" +
      "img{max-width:100%;height:auto}" +
      "body{min-height:1px}"
    );
  }

  /** Put the base styles and bridge at the start of the page's head. */
  function buildDocument(html: string): string {
    const head = "<" + "style id=\"ravenbot-visual-base\">" + themeCss() + "</style>" + scriptTag(BOOTSTRAP);
    const headTag = /<head\b[^>]*>/i;
    if (headTag.test(html)) {
      return html.replace(headTag, (tag) => tag + head);
    }
    const htmlTag = /<html\b[^>]*>/i;
    if (htmlTag.test(html)) {
      return html.replace(htmlTag, (tag) => tag + "<head>" + head + "</head>");
    }
    const bodyTag = /<body\b[^>]*>/i;
    if (bodyTag.test(html)) {
      return html.replace(bodyTag, (tag) => "<head>" + head + "</head>" + tag);
    }
    return "<!doctype html><html><head>" + head + "</head><body>" + html + "</body></html>";
  }

  function idFromUrl(url: string): string | null {
    const parts = url.split("/").filter(Boolean);
    return parts.length > 0 ? parts[parts.length - 1] : null;
  }

  function release(id: string | null) {
    if (!id) return;
    void invoke("release_visual", { id }).catch(() => {
      /* no Tauri bridge in the browser preview */
    });
  }

  function clampHeight(value: number): number {
    return Math.min(MAX_HEIGHT, Math.max(MIN_HEIGHT, Math.ceil(value)));
  }

  $effect(() => {
    const html = code;
    if (streaming) return;
    if (!html.trim()) {
      error = "This visual page is empty.";
      return;
    }
    const token = ++loadToken;
    error = "";
    view = "visual";
    const previous = registeredId;
    registeredId = null;
    visualUrl = null;
    srcdoc = "";
    release(previous);
    const doc = buildDocument(html);
    void (async () => {
      try {
        const url = await invoke<string>("register_visual", { html: doc });
        if (token !== loadToken) {
          release(idFromUrl(url));
          return;
        }
        visualUrl = url;
        registeredId = idFromUrl(url);
      } catch {
        // No Tauri bridge (browser preview) or the page was refused. The
        // srcdoc fallback still renders in a browser; in the app it shows the
        // source through the error path if the frame is blocked.
        if (token === loadToken) srcdoc = doc;
      }
    })();
    return () => {
      loadToken += 1;
    };
  });

  function sendTheme(frame: HTMLIFrameElement | null = frameEl) {
    const target = frame?.contentWindow;
    if (!target) return;
    target.postMessage({ __ravenbotTheme: true, variables: themeVariables() }, "*");
  }

  function sendDisplayMode(frame: HTMLIFrameElement | null = frameEl) {
    const target = frame?.contentWindow;
    if (!target) return;
    target.postMessage({ __ravenbotDisplayMode: true, mode: expanded ? "fullscreen" : "inline" }, "*");
  }

  function onFrameLoad(event: Event) {
    const frame = event.currentTarget as HTMLIFrameElement;
    frameWindow = frame.contentWindow;
    sendTheme(frame);
    sendDisplayMode(frame);
    frame.contentWindow?.postMessage({ __ravenbotRequestSize: true }, "*");
  }

  function onMessage(event: MessageEvent) {
    if (!frameWindow || event.source !== frameWindow) return;
    const data = event.data as Record<string, unknown> | null;
    if (!data || data.__ravenbotVisual !== true) return;
    if (data.type === "size") {
      const value = Number(data.height);
      if (Number.isFinite(value)) height = clampHeight(value);
      return;
    }
    if (data.type === "open-link") {
      const href = safeHref(String(data.url ?? ""));
      if (!href || !isExternal(href)) return;
      const now = Date.now();
      if (now - lastLinkAt < 800) return;
      lastLinkAt = now;
      void openExternal(href);
      return;
    }
    if (data.type === "display-mode") {
      if (data.mode === "fullscreen") expanded = true;
      else if (data.mode === "inline") expanded = false;
    }
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === "Escape" && expanded) expanded = false;
  }

  onMount(() => {
    themeObserver = new MutationObserver(() => sendTheme());
    themeObserver.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["class", "style", "data-theme"],
    });
    return () => {
      themeObserver?.disconnect();
      themeObserver = null;
    };
  });

  onDestroy(() => {
    release(registeredId);
    frameWindow = null;
  });
</script>

<svelte:window onmessage={onMessage} onkeydown={onKeydown} />

{#if streaming}
  <figure class="md-visual md-visual--loading" aria-label="Interactive visual is being written">
    <div class="md-visual-loading">
      <span class="md-visual-loading-dot"></span>
      <span class="md-visual-loading-dot"></span>
      <span class="md-visual-loading-dot"></span>
    </div>
    <figcaption>Building interactive visual…</figcaption>
  </figure>
{:else if error}
  <figure class="md-visual md-visual--error" aria-label="Interactive visual could not be rendered">
    <figcaption>Interactive visual could not be rendered</figcaption>
    <p class="md-visual-error-text">{error}</p>
    <MarkdownCode code={code} language="html" />
  </figure>
{:else}
  <figure class="md-visual" class:md-visual--expanded={expanded}>
    <div class="md-visual-bar">
      <span class="md-visual-badge" title="Scripts run in an isolated origin">
        <ShieldCheck class="size-3" />
        Interactive
      </span>
      <div class="md-visual-tabs" role="group" aria-label="Visual view">
        <button
          type="button"
          class="md-visual-tab"
          class:md-visual-tab--on={view === "visual"}
          aria-pressed={view === "visual"}
          onclick={() => (view = "visual")}
        >
          <Eye class="size-3" />
          Visual
        </button>
        <button
          type="button"
          class="md-visual-tab"
          class:md-visual-tab--on={view === "source"}
          aria-pressed={view === "source"}
          onclick={() => (view = "source")}
        >
          <Code2 class="size-3" />
          Source
        </button>
      </div>
      <button
        type="button"
        class="md-visual-expand"
        aria-label={expanded ? "Return visual to the chat" : "Expand visual"}
        title={expanded ? "Return visual to the chat" : "Expand visual"}
        onclick={() => {
          expanded = !expanded;
          sendDisplayMode();
        }}
      >
        {#if expanded}
          <Minimize2 class="size-3.5" />
        {:else}
          <Maximize2 class="size-3.5" />
        {/if}
      </button>
    </div>

    {#if view === "source"}
      <div class="md-visual-source">
        <MarkdownCode code={code} language="html" />
      </div>
    {:else}
      <div class="md-visual-frame" style={expanded ? undefined : `height:${height}px`}>
        {#if visualUrl}
          <iframe
            bind:this={frameEl}
            src={visualUrl}
            title="Interactive visual"
            sandbox="allow-scripts"
            referrerpolicy="no-referrer"
            onload={onFrameLoad}
          ></iframe>
        {:else if srcdoc}
          <iframe
            bind:this={frameEl}
            {srcdoc}
            title="Interactive visual"
            sandbox="allow-scripts"
            referrerpolicy="no-referrer"
            onload={onFrameLoad}
          ></iframe>
        {:else}
          <div class="md-visual-pending">Preparing interactive visual…</div>
        {/if}
      </div>
    {/if}
  </figure>
{/if}
