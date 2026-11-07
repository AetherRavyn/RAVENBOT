<script lang="ts">
  /**
   * The browser dock's chrome.
   *
   * The page is a native child webview owned by Rust; this component owns a
   * transparent placeholder whose measured rectangle becomes the webview's
   * bounds. That is why the dock can sit inside the grid like any other pane:
   * the native view is composite above the app webview, but its position is
   * driven entirely by this element.
   *
   * Hiding the panel does not destroy the webview. History, cookies, and
   * scroll position survive, which is the difference between a browser dock
   * and a screenshot.
   */
  import { onDestroy, onMount, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { browserDock } from "$lib/browserDock.svelte";
  import { agentActivity } from "$lib/agentActivity.svelte";
  import { t } from "$lib/i18n";
  import {
    ArrowLeft,
    ArrowRight,
    Bot,
    Download,
    FolderOpen,
    Globe,
    Hand,
    Loader2,
    RotateCw,
    X,
  } from "@lucide/svelte";

  let { bots = [] }: { bots?: any[] } = $props();

  interface Bounds {
    x: number;
    y: number;
    width: number;
    height: number;
  }

  let viewport = $state<HTMLDivElement | null>(null);
  let body = $state<HTMLDivElement | null>(null);
  let address = $state("");
  let wasOpen = false;
  let lastNonce = 0;
  let openSeq = 0;
  let lastBounds = "";
  let frame = 0;
  let downloadsDir = $state("");
  let downloadError = $state<string | null>(null);
  let browserActivity = $derived(agentActivity.latest("browser"));

  function botName(id: string): string {
    return bots.find((bot) => bot.id === id)?.name || t("ui.fallbackAgent");
  }

  function activityTime(iso: string): string {
    const date = new Date(iso);
    if (Number.isNaN(date.getTime())) return "";
    return date.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  }
  const unlisteners: UnlistenFn[] = [];
  // Evaluated at call time, not module load: the Tauri internals object is
  // injected by the shell and must not be sampled before the bridge exists.
  function isTauri(): boolean {
    return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
  }

  function normalizeUrl(raw: string): string {
    const trimmed = raw.trim();
    if (!trimmed) return browserDock.currentUrl || browserDock.url;
    if (/^https?:\/\//i.test(trimmed)) return trimmed;
    // A bare host reads as a place; anything else reads as a search.
    if (
      /^[a-z0-9.-]+\.[a-z]{2,}(?:[/:?#]|$)/i.test(trimmed) ||
      /^localhost(?::\d+)?(?:[/?#]|$)/i.test(trimmed)
    ) {
      return `https://${trimmed}`;
    }
    return `https://duckduckgo.com/?q=${encodeURIComponent(trimmed)}`;
  }

  function measure(): Bounds | null {
    const element = viewport ?? body;
    if (!element) return null;
    const rect = element.getBoundingClientRect();
    if (rect.width < 40 || rect.height < 40) return null;
    return {
      x: Math.round(rect.x),
      y: Math.round(rect.y),
      width: Math.round(rect.width),
      height: Math.round(rect.height),
    };
  }

  function boundsKey(bounds: Bounds): string {
    return `${bounds.x}:${bounds.y}:${bounds.width}:${bounds.height}`;
  }

  /**
   * A panel that was just opened has its element in the DOM but may not have
   * been laid out yet. One `tick` is the usual answer; a couple of animation
   * frames make it reliable across the first open after a cold start.
   */
  async function waitForBounds(): Promise<Bounds | null> {
    for (let attempt = 0; attempt < 5; attempt += 1) {
      await tick();
      await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
      const bounds = measure();
      if (bounds) return bounds;
    }
    return null;
  }

  async function openDock(raw: string) {
    const seq = ++openSeq;
    const url = normalizeUrl(raw);
    address = url;
    if (!isTauri()) {
      browserDock.error = t("browser.desktopOnly");
      browserDock.loading = false;
      return;
    }
    browserDock.error = null;
    browserDock.loading = true;
    const bounds = await waitForBounds();
    if (!bounds || seq !== openSeq) {
      if (seq === openSeq) browserDock.loading = false;
      return;
    }
    lastBounds = boundsKey(bounds);
    try {
      const status = await invoke<{
        url?: string;
        title?: string;
        taken_over?: boolean;
      }>("browser_dock_open", { url, bounds });
      if (seq !== openSeq) return;
      browserDock.url = url;
      if (status?.url) browserDock.currentUrl = status.url;
      if (status?.title) browserDock.setTitle(status.title);
      browserDock.takenOver = status?.taken_over === true;
      if (status?.url && status.url === browserDock.currentUrl) {
        // Reopening an unchanged page does not fire a load event, so the
        // spinner would otherwise stay on forever.
        browserDock.loading = false;
      }
    } catch (error) {
      if (seq !== openSeq) return;
      browserDock.error = String(error);
      browserDock.loading = false;
    }
  }

  function scheduleBounds() {
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      void syncBounds();
    });
  }

  async function syncBounds() {
    if (!browserDock.open || !isTauri()) return;
    const bounds = measure();
    if (!bounds) return;
    const key = boundsKey(bounds);
    if (key === lastBounds) return;
    lastBounds = key;
    try {
      await invoke("browser_dock_set_bounds", { bounds });
    } catch {
      /* the dock may have been closed while the frame was queued */
    }
  }

  async function submit(event?: SubmitEvent) {
    event?.preventDefault();
    const url = normalizeUrl(address);
    browserDock.url = url;
    await openDock(url);
  }

  async function back() {
    if (isTauri()) await invoke("browser_dock_back").catch(() => undefined);
  }

  async function forward() {
    if (isTauri()) await invoke("browser_dock_forward").catch(() => undefined);
  }

  async function reload() {
    if (!isTauri()) return;
    browserDock.loading = true;
    await invoke("browser_dock_reload").catch((error) => {
      browserDock.error = String(error);
      browserDock.loading = false;
    });
  }

  function close() {
    browserDock.hide();
  }

  async function loadDownloads() {
    if (!isTauri()) return;
    try {
      const [items, directory] = await Promise.all([
        invoke<import("$lib/browserDock.svelte").BrowserDownload[]>("list_browser_downloads"),
        invoke<string>("browser_downloads_dir"),
      ]);
      browserDock.setDownloads(items ?? []);
      downloadsDir = directory;
      downloadError = null;
    } catch (cause) {
      downloadError = String(cause);
    }
  }

  async function openDownloads() {
    browserDock.downloadsOpen = true;
    if (isTauri()) {
      await invoke("browser_dock_hide").catch(() => undefined);
    }
    await loadDownloads();
  }

  function closeDownloads() {
    browserDock.downloadsOpen = false;
    void openDock(browserDock.currentUrl || browserDock.url);
  }

  async function openDownload(path: string) {
    try {
      const { openPath } = await import("@tauri-apps/plugin-opener");
      await openPath(path);
    } catch (cause) {
      downloadError = String(cause);
    }
  }

  async function showDownloadsFolder() {
    if (!downloadsDir) return;
    await openDownload(downloadsDir);
  }

  function formatBytes(size?: number | null): string {
    if (size == null) return "";
    if (size < 1024) return `${size} B`;
    if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
    return `${(size / (1024 * 1024)).toFixed(1)} MB`;
  }

  function downloadState(state: string): string {
    if (state === "downloading") return t("browser.downloading");
    if (state === "failed") return t("browser.downloadFailed");
    return t("browser.downloadDone");
  }

  async function toggleTakeover() {
    if (!isTauri()) return;
    const next = !browserDock.takenOver;
    try {
      browserDock.takenOver = await invoke<boolean>("browser_dock_set_takeover", {
        taken: next,
      });
    } catch (cause) {
      browserDock.error = String(cause);
    }
  }

  $effect(() => {
    const nonce = browserDock.nonce;
    const open = browserDock.open;
    if (open && (nonce !== lastNonce || !wasOpen)) {
      // A fresh request can name a new URL; reopening the panel should
      // restore the page the user was actually on, redirects and all.
      const fresh = nonce !== lastNonce;
      lastNonce = nonce;
      wasOpen = true;
      const target = fresh
        ? browserDock.url
        : browserDock.currentUrl || browserDock.url;
      address = target;
      void openDock(target);
    } else if (!open && wasOpen) {
      wasOpen = false;
      if (isTauri()) void invoke("browser_dock_hide").catch(() => undefined);
    }
  });

  // A resize observer cannot be created for an element that does not exist
  // yet, so it is tied to the panel's open state rather than to mount.
  $effect(() => {
    if (!browserDock.open || !viewport) return;
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(scheduleBounds);
    observer.observe(viewport);
    return () => observer.disconnect();
  });

  onMount(() => {
    window.addEventListener("resize", scheduleBounds);
    if (!isTauri()) return;

    void (async () => {
      try {
        unlisteners.push(
          await listen<{ url?: string }>("browser-dock:request", (event) => {
            const url = event.payload?.url;
            if (url) browserDock.show(url);
            else browserDock.show();
          }),
        );
        unlisteners.push(
          await listen<{ url?: string; event?: string }>(
            "browser-dock:page-load",
            (event) => {
              const url = event.payload?.url ?? "";
              const phase = event.payload?.event ?? "finished";
              browserDock.pageLoad(url, phase);
              if (url) address = url;
              if (phase === "started") browserDock.loading = true;
              if (phase === "finished") {
                // The title signal can arrive before or after load depending
                // on the engine. One small DOM read makes it deterministic and
                // also exercises the same evaluator the agent tools use.
                void invoke<string>("browser_dock_eval", { script: "document.title" })
                  .then((title) => {
                    if (typeof title === "string" && title.trim()) {
                      browserDock.setTitle(title.trim());
                    }
                  })
                  .catch(() => undefined);
              }
            },
          ),
        );
        unlisteners.push(
          await listen<{ title?: string }>("browser-dock:title", (event) => {
            browserDock.setTitle(event.payload?.title ?? "");
          }),
        );
        unlisteners.push(
          await listen("browser-dock:closed", () => {
            browserDock.loading = false;
          }),
        );
        unlisteners.push(
          await listen<{
            download?: import("$lib/browserDock.svelte").BrowserDownload;
            success?: boolean;
          }>("browser-dock:download", (event) => {
            if (event.payload?.download) {
              browserDock.upsertDownload(event.payload.download);
            } else {
              void loadDownloads();
            }
          }),
        );
      } catch (error) {
        // Browser previews have no Tauri event bridge; the dock itself already
        // says it needs the desktop app, so this is not a second error.
        console.debug("browser dock events unavailable:", error);
      }
    })();
  });

  onDestroy(() => {
    window.removeEventListener("resize", scheduleBounds);
    if (frame) cancelAnimationFrame(frame);
    for (const unlisten of unlisteners) unlisten();
    if (wasOpen && isTauri()) void invoke("browser_dock_hide").catch(() => undefined);
  });
</script>

{#if browserDock.open}
  <aside class="browser-dock" aria-label={t("browser.title")}>
    <header class="browser-dock__header">
      <div class="browser-dock__controls">
        <button
          type="button"
          class="browser-dock__button"
          title={t("browser.back")}
          aria-label={t("browser.back")}
          onclick={back}
        >
          <ArrowLeft class="size-4" />
        </button>
        <button
          type="button"
          class="browser-dock__button"
          title={t("browser.forward")}
          aria-label={t("browser.forward")}
          onclick={forward}
        >
          <ArrowRight class="size-4" />
        </button>
        <button
          type="button"
          class="browser-dock__button"
          title={t("browser.reload")}
          aria-label={t("browser.reload")}
          onclick={reload}
        >
          <RotateCw class="size-4 {browserDock.loading ? 'animate-spin' : ''}" />
        </button>
      </div>

      <form class="browser-dock__address" onsubmit={submit}>
        {#if browserDock.loading}
          <Loader2 class="browser-dock__address-icon size-3.5 animate-spin" />
        {:else}
          <Globe class="browser-dock__address-icon size-3.5" />
        {/if}
        <input
          class="browser-dock__input"
          bind:value={address}
          aria-label={t("browser.address")}
          placeholder="https://"
          spellcheck="false"
          autocomplete="off"
        />
      </form>

      <button
        type="button"
        class="browser-dock__button browser-dock__download-button"
        title={t("browser.downloads")}
        aria-label={t("browser.downloads")}
        onclick={openDownloads}
      >
        <Download class="size-4" />
        {#if browserDock.downloads.length > 0}
          <span class="browser-dock__badge" aria-hidden="true">
            {browserDock.downloads.length > 9 ? "9+" : browserDock.downloads.length}
          </span>
        {/if}
      </button>

      <button
        type="button"
        class="browser-dock__button"
        class:is-active={browserDock.takenOver}
        title={browserDock.takenOver ? t("browser.release") : t("browser.takeover")}
        aria-label={browserDock.takenOver ? t("browser.release") : t("browser.takeover")}
        aria-pressed={browserDock.takenOver}
        onclick={toggleTakeover}
      >
        <Hand class="size-4" />
      </button>

      <button
        type="button"
        class="browser-dock__button"
        title={t("browser.close")}
        aria-label={t("browser.close")}
        onclick={close}
      >
        <X class="size-4" />
      </button>
    </header>

    <div class="browser-dock__caption" title={browserDock.title || address}>
      <span class="browser-dock__title">{browserDock.title || t("browser.title")}</span>
      <span class="browser-dock__host">{browserDock.currentUrl || address}</span>
    </div>

    {#if browserActivity}
      <p class="browser-dock__activity" data-ok={browserActivity.ok} role="status">
        <span class="browser-dock__activity-dot" aria-hidden="true"></span>
        <Bot class="size-3.5 shrink-0" strokeWidth={1.8} />
        <strong>{botName(browserActivity.bot_id)}</strong>
        <span class="browser-dock__activity-detail">{browserActivity.detail}</span>
        <time>{activityTime(browserActivity.at)}</time>
      </p>
    {/if}

    {#if browserDock.takenOver}
      <p class="browser-dock__takeover" role="status">
        <Hand class="size-3.5" />
        <span>{t("browser.takenOver")}</span>
        <button type="button" onclick={toggleTakeover}>{t("browser.release")}</button>
      </p>
    {/if}

    {#if browserDock.error}
      <p class="browser-dock__error" role="alert">{browserDock.error}</p>
    {/if}

    <div class="browser-dock__body" bind:this={body}>
      <div class="browser-dock__viewport" bind:this={viewport} aria-hidden="true"></div>
      {#if !isTauri()}
        <div class="browser-dock__fallback">{t("browser.desktopOnly")}</div>
      {/if}
    </div>
  </aside>

  {#if browserDock.downloadsOpen}
    <div
      class="downloads-overlay"
      role="dialog"
      aria-modal="true"
      aria-label={t("browser.downloads")}
      tabindex="-1"
      onkeydown={(event) => {
        if (event.key === "Escape") closeDownloads();
      }}
      onclick={(event) => {
        if (event.target === event.currentTarget) closeDownloads();
      }}
    >
      <div class="downloads-dialog__panel">
        <header class="downloads-dialog__head">
          <div>
            <h2>{t("browser.downloads")}</h2>
            <code>{downloadsDir || "~/RAVENBOT/downloads"}</code>
          </div>
          <div class="downloads-dialog__actions">
            <button type="button" onclick={() => loadDownloads()} title={t("browser.downloadRefresh")}>
              <RotateCw class="size-3.5" />
            </button>
            <button type="button" onclick={showDownloadsFolder} title={t("browser.downloadFolder")}>
              <FolderOpen class="size-3.5" />
            </button>
            <button type="button" onclick={closeDownloads} title={t("browser.downloadClose")}>
              <X class="size-3.5" />
            </button>
          </div>
        </header>

        {#if downloadError}
          <p class="downloads-dialog__error" role="alert">{downloadError}</p>
        {/if}

        {#if browserDock.downloads.length === 0}
          <div class="downloads-dialog__empty">
            <Download class="size-5" />
            <p>{t("browser.downloadsEmpty")}</p>
          </div>
        {:else}
          <div class="downloads-dialog__list">
            {#each browserDock.downloads as item (item.id)}
              <article class="downloads-dialog__item" data-state={item.state}>
                <div class="downloads-dialog__meta">
                  <strong>{item.name}</strong>
                  <span>
                    {downloadState(item.state)}
                    {#if item.size} · {formatBytes(item.size)}{/if}
                  </span>
                </div>
                <button
                  type="button"
                  disabled={item.state !== "done"}
                  onclick={() => openDownload(item.path)}
                >
                  {t("browser.downloadOpen")}
                </button>
              </article>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  {/if}
{/if}
