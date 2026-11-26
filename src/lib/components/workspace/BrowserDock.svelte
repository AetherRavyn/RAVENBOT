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
  import { browserDock, type BrowserProfile } from "$lib/browserDock.svelte";
  import { COOKIE_REJECT_SCRIPT } from "$lib/browserCookies";
  import BrowserVaultSave from "./BrowserVaultSave.svelte";
  import LearnFromRun from "$lib/components/LearnFromRun.svelte";
  import { agentActivity } from "$lib/agentActivity.svelte";
  import { t } from "$lib/i18n";
  import { notify } from "$lib/toast";
  import {
    ArrowLeft,
    ArrowRight,
    Bot,
    Cookie,
    Download,
    FolderOpen,
    Globe,
    GraduationCap,
    Hand,
    Loader2,
    Lock,
    Plus,
    RotateCw,
    ShieldCheck,
    Square,
    X,
    ZoomIn,
    ZoomOut,
  } from "@lucide/svelte";

  let {
    bots = [],
    onClose,
  }: { bots?: any[]; onClose?: () => void } = $props();

  interface Bounds {
    x: number;
    y: number;
    width: number;
    height: number;
  }

  interface DockTabStatus {
    id: string;
    title: string;
    url: string;
  }

  interface DockStatus {
    url?: string;
    title?: string;
    taken_over?: boolean;
    profile?: string;
    active_tab?: string;
    tabs?: DockTabStatus[];
  }

  interface RecordStatus {
    recording: boolean;
    count: number;
    tab_id?: string | null;
    elapsed_ms: number;
    auto_stopped?: boolean;
  }

  let viewport = $state<HTMLDivElement | null>(null);
  let body = $state<HTMLDivElement | null>(null);
  let address = $state("");
  let wasOpen = false;
  let lastNonce = 0;
  let reloadNamedProfiles: (() => void) | null = null;
  let openSeq = 0;
  let lastBounds = "";
  let frame = 0;
  let downloadsDir = $state("");
  let downloadError = $state<string | null>(null);
  let secureOpen = $state(false);
  let secureMode = $state<"password" | "otp">("password");
  let secureValue = $state("");
  let secureBusy = $state(false);
  let secureError = $state("");
  let secureOrigin = $state("");
  let saveToVaultOpen = $state(false);
  let saveName = $state("");
  let saveUsername = $state("");
  let draggedTabId = $state<string | null>(null);
  let autoRejectCookies = $state(false);
  let cookiePolicyListener: ((event: Event) => void) | null = null;
  const autoRejectTimers = new Map<
    string,
    ReturnType<typeof setTimeout>[]
  >();
  const autoRejectSeen = new Set<string>();
  let zoom = $state(0.8);
  let recording = $state(false);
  let recordPending = $state(false);
  let recordBusy = $state(false);
  let recordCount = $state(0);
  let recordElapsed = $state(0);
  let recordPoll: ReturnType<typeof setInterval> | null = null;
  let recordPollBusy = false;
  let teachDraft = $state<any>(null);
  let teachOpen = $state(false);
  let selectedProfile = $derived(
    browserDock.profileOverrides[browserDock.scope] ?? "__auto__",
  );
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

  function applyStatus(status: DockStatus | null | undefined) {
    if (!status) return;
    if (Array.isArray(status.tabs)) {
      browserDock.setTabs(status.tabs, status.active_tab || "");
    }
    if (status.url) browserDock.currentUrl = status.url;
    if (status.title) browserDock.setTitle(status.title, status.active_tab);
    browserDock.takenOver = status.taken_over === true;
    if (status.profile) browserDock.setProfile(status.profile);
  }

  function tabLabel(tab: DockTabStatus): string {
    if (tab.title.trim()) return tab.title.trim();
    try {
      return new URL(tab.url).hostname.replace(/^www\./u, "") || t("browser.newTab");
    } catch {
      return t("browser.newTab");
    }
  }

  async function newTab(raw = "https://duckduckgo.com/") {
    if (!isTauri()) {
      browserDock.error = t("browser.desktopOnly");
      return;
    }
    const bounds = measure();
    if (!bounds) return;
    const url = normalizeUrl(raw);
    try {
      const status = await invoke<DockStatus>("browser_dock_new_tab", {
        url,
        bounds,
        profile: browserDock.profile,
      });
      applyStatus(status);
      address = browserDock.currentUrl || url;
      await tick();
      await setZoom(zoom);
      scheduleBounds();
    } catch (error) {
      browserDock.error = String(error);
    }
  }

  async function switchTab(tabId: string) {
    if (!isTauri() || tabId === browserDock.activeTabId) return;
    const bounds = measure();
    if (!bounds) return;
    try {
      const status = await invoke<DockStatus>("browser_dock_activate_tab", {
        tabId,
        bounds,
      });
      applyStatus(status);
      address = browserDock.currentUrl || "";
      await tick();
      await setZoom(zoom);
      scheduleBounds();
    } catch (error) {
      browserDock.error = String(error);
    }
  }

  function startTabDrag(event: DragEvent, tabId: string) {
    draggedTabId = tabId;
    if (event.dataTransfer) {
      event.dataTransfer.effectAllowed = "move";
      event.dataTransfer.setData("text/plain", tabId);
    }
  }

  function dropTab(event: DragEvent, targetId: string) {
    event.preventDefault();
    const sourceId = draggedTabId;
    draggedTabId = null;
    if (!sourceId || sourceId === targetId || !isTauri()) return;
    const previousTabs = [...browserDock.tabs];
    const previousActive = browserDock.activeTabId;
    const order = browserDock.reorderTabs(sourceId, targetId);
    if (!order) return;
    void invoke<DockStatus>("browser_dock_reorder_tabs", { tabIds: order })
      .then(applyStatus)
      .catch((error) => {
        browserDock.setTabs(previousTabs, previousActive);
        browserDock.error = String(error);
      });
  }

  async function closeTab(tabId: string) {

    if (!isTauri()) return;
    try {
      const status = await invoke<DockStatus>("browser_dock_close_tab", {
        tabId,
      });
      applyStatus(status);
      if (browserDock.tabs.length === 0) {
        close();
        return;
      }
      address = browserDock.currentUrl || "";
      await tick();
      scheduleBounds();
    } catch (error) {
      browserDock.error = String(error);
    }
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
      const status = await invoke<DockStatus>("browser_dock_open", {
        url,
        bounds,
        profile: browserDock.profile,
        restoreTabs: browserDock.restoreTabs,
      });
      if (seq !== openSeq) return;
      browserDock.url = url;
      applyStatus(status);
      await setZoom(zoom);
      if (status?.url) address = status.url;
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

  async function setZoom(next: number) {
    if (!isTauri()) return;
    const clamped = Math.min(2, Math.max(0.5, Math.round(next * 100) / 100));
    try {
      zoom = await invoke<number>("browser_dock_set_zoom", { zoom: clamped });
    } catch (error) {
      browserDock.error = String(error);
    }
  }

  async function submit(event?: SubmitEvent) {
    event?.preventDefault();
    const url = normalizeUrl(address);
    browserDock.url = url;
    browserDock.restoreTabs = false;
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
    if ((recording || recordPending) && isTauri()) {
      void invoke("browser_dock_record_cancel").catch(() => undefined);
      stopRecordPoll();
    }
    if (onClose) onClose();
    else browserDock.hide();
  }

  function origin(): string {
    try {
      return new URL(browserDock.currentUrl || browserDock.url || address).origin;
    } catch {
      return browserDock.currentUrl || address || "";
    }
  }

  function openSecure() {
    secureMode = "password";
    secureValue = "";
    secureError = "";
    secureOrigin = origin();
    saveToVaultOpen = false;
    secureOpen = true;
  }

  function closeSecure() {
    secureOpen = false;
    secureValue = "";
    secureError = "";
    saveToVaultOpen = false;
    saveName = "";
    saveUsername = "";
  }

  function hostnameOf(value: string): string {
    try {
      return new URL(value).hostname.replace(/^www\./u, "");
    } catch {
      return "";
    }
  }

  async function openSaveToVault() {
    if (secureMode !== "password" || !secureValue) return;
    saveName =
      browserDock.title.trim() ||
      hostnameOf(secureOrigin) ||
      t("browser.securePassword");
    saveUsername = "";
    if (!isTauri()) {
      saveToVaultOpen = true;
      return;
    }
    const script = `(() => {
      const visible = (el) => {
        const rect = el.getBoundingClientRect();
        const style = getComputedStyle(el);
        return rect.width > 1 && rect.height > 1 && style.visibility !== "hidden" && style.display !== "none";
      };
      const el = [...document.querySelectorAll('input[autocomplete="username"], input[type="email"], input[name*="user" i], input[name*="email" i]')].find(visible);
      return el ? String(el.value || "") : "";
    })()`;
    try {
      const found = await invoke<string>("browser_dock_user_eval", {
        script,
        tabId: browserDock.activeTabId || null,
      });
      if (typeof found === "string" && !saveUsername) {
        saveUsername = found.trim();
      }
    } catch {
      /* username is optional */
    }
    if (secureOpen && secureMode === "password") {
      saveToVaultOpen = true;
    }
  }

  function savedToVault(updated: boolean) {
    const message = updated ? t("browser.vaultUpdated") : t("browser.vaultSaved");
    closeSecure();
    notify(message, "success");
  }

  function secureScript(secret: string, mode: "password" | "otp"): string {
    return `(() => {
      const mode = ${JSON.stringify(mode)};
      const value = ${JSON.stringify(secret)};
      const selectors = mode === "otp"
        ? ['input[autocomplete="one-time-code"]', 'input[inputmode="numeric"]', 'input[name*="code" i]', 'input[type="tel"]']
        : ['input[type="password"]'];
      const fields = [];
      for (const selector of selectors) fields.push(...document.querySelectorAll(selector));
      const unique = [...new Set(fields)];
      const visible = (el) => {
        const rect = el.getBoundingClientRect();
        const style = getComputedStyle(el);
        return rect.width > 1 && rect.height > 1 && style.visibility !== "hidden" && style.display !== "none";
      };
      const el = unique.find((field) => visible(field) && !field.value) || unique.find(visible) || unique[0];
      if (!el) return { ok: false, error: "No secure field was found on this page." };
      el.focus();
      const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
      const setter = Object.getOwnPropertyDescriptor(proto, "value")?.set;
      if (setter) setter.call(el, value); else el.value = value;
      el.dispatchEvent(new Event("input", { bubbles: true }));
      el.dispatchEvent(new Event("change", { bubbles: true }));
      return { ok: true, field: el.name || el.id || el.type || "secure field", origin: location.origin };
    })()`;
  }

  async function fillSecure() {
    if (!secureValue || secureBusy || !isTauri()) return;
    secureBusy = true;
    secureError = "";
    try {
      const result = await invoke<any>("browser_dock_user_eval", {
        script: secureScript(secureValue, secureMode),
        tabId: browserDock.activeTabId || null,
      });
      if (result?.ok) {
        closeSecure();
        notify(t("browser.secureFilled"), "success");
      } else {
        secureError = String(result?.error || t("browser.secureNotFound"));
      }
    } catch (error) {
      secureError = String(error);
    } finally {
      secureValue = "";
      secureBusy = false;
    }
  }

  async function evaluateCookieScript(tabId: string | null) {
    return invoke<any>("browser_dock_user_eval", {
      script: COOKIE_REJECT_SCRIPT,
      tabId,
    });
  }

  async function refuseCookies() {
    if (!isTauri()) return;
    try {
      const result = await evaluateCookieScript(
        browserDock.activeTabId || null,
      );
      notify(
        result?.clicked ? t("browser.cookieRefused") : t("browser.cookieNone"),
        result?.clicked ? "success" : "info",
      );
    } catch (error) {
      notify(`${t("browser.cookieFailed")} ${String(error)}`, "error");
    }
  }

  async function autoRejectForTab(tabId: string) {
    try {
      await evaluateCookieScript(tabId);
    } catch {
      /* the tab may have closed or navigated; the next load retries */
    }
  }

  function clearCookieTimers() {
    for (const timers of autoRejectTimers.values()) {
      for (const timer of timers) clearTimeout(timer);
    }
    autoRejectTimers.clear();
    autoRejectSeen.clear();
  }

  function scheduleCookieReject(tabId: string, url: string) {
    if (!autoRejectCookies || !/^https?:/i.test(url)) return;
    const key = `${tabId}:${url}`;
    if (autoRejectSeen.has(key)) return;
    autoRejectSeen.add(key);
    if (autoRejectSeen.size > 200) {
      const oldest = autoRejectSeen.values().next().value;
      if (oldest) autoRejectSeen.delete(oldest);
    }
    if (autoRejectTimers.size > 100) {
      const first = autoRejectTimers.keys().next().value;
      if (first) {
        for (const timer of autoRejectTimers.get(first) ?? []) clearTimeout(timer);
        autoRejectTimers.delete(first);
      }
    }
    // Consent banners often appear after the load event. Two retries catch the
    // common late-render cases without turning every page into a polling loop.
    autoRejectTimers.set(key, [
      setTimeout(() => {
        if (autoRejectCookies) void autoRejectForTab(tabId);
      }, 800),
      setTimeout(() => {
        if (autoRejectCookies) void autoRejectForTab(tabId);
      }, 2600),
    ]);
  }

  async function refreshCookiePolicy() {
    if (!isTauri()) return;
    try {
      const enabled = await invoke<boolean>(
        "get_effective_browser_cookie_policy",
        { profileId: browserDock.profile },
      );
      updateCookiePolicy(enabled);
    } catch {
      updateCookiePolicy(false);
    }
  }

  $effect(() => {
    browserDock.profile;
    void refreshCookiePolicy();
  });

  function updateCookiePolicy(enabled: boolean) {
    autoRejectCookies = enabled;
    if (!enabled) {
      clearCookieTimers();
      return;
    }
    if (browserDock.open && browserDock.activeTabId) {
      void autoRejectForTab(browserDock.activeTabId);
    }
  }

  async function loadNamedProfiles() {
    if (!isTauri()) return;
    try {
      const profiles = await invoke<BrowserProfile[]>("list_browser_profiles");
      browserDock.setNamedProfiles(profiles);
    } catch {
      /* profile management is optional */
    }
  }

  function chooseProfile(event: Event) {
    const value = (event.currentTarget as HTMLSelectElement).value;
    browserDock.setProfileOverride(
      browserDock.scope,
      value === "__auto__" ? null : value,
    );
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

  function elapsedText(ms: number): string {
    const seconds = Math.max(0, Math.floor(ms / 1000));
    const minutes = Math.floor(seconds / 60);
    return `${minutes}:${String(seconds % 60).padStart(2, "0")}`;
  }

  function stopRecordPoll() {
    if (recordPoll) {
      clearInterval(recordPoll);
      recordPoll = null;
    }
  }

  async function pollRecordStatus() {
    if (!isTauri() || recordPollBusy) return;
    recordPollBusy = true;
    try {
      const status = await invoke<RecordStatus>("browser_dock_record_status");
      recordCount = status.count ?? 0;
      recordElapsed = status.elapsed_ms ?? 0;
      if (!status.recording && recording) {
        recording = false;
        stopRecordPoll();
        if (!status.tab_id) {
          recordPending = false;
          recordCount = 0;
          recordElapsed = 0;
          notify(t("demo.recordingLost"), "error");
        } else {
          recordPending = true;
          notify(t("demo.autoStopped"), "info");
        }
      }
    } catch {
      /* a poll can lose a navigation; the next tick retries */
    } finally {
      recordPollBusy = false;
    }
  }

  function startRecordPoll() {
    stopRecordPoll();
    recordPoll = setInterval(() => {
      void pollRecordStatus();
    }, 500);
  }

  async function startRecording() {
    if (!isTauri() || recordBusy || recording || recordPending) return;
    if (!browserDock.activeTabId) {
      notify(t("demo.noTab"), "error");
      return;
    }
    recordBusy = true;
    browserDock.error = null;
    try {
      const status = await invoke<RecordStatus>("browser_dock_record_start");
      recording = status.recording !== false;
      recordPending = status.recording === false;
      recordCount = status.count ?? 0;
      recordElapsed = status.elapsed_ms ?? 0;
      startRecordPoll();
      notify(t("demo.started"), "info");
    } catch (cause) {
      browserDock.error = String(cause);
      notify(t("demo.startFailed"), "error");
    } finally {
      recordBusy = false;
    }
  }

  async function stopRecording() {
    if (!isTauri() || recordBusy) return;
    recordBusy = true;
    stopRecordPoll();
    try {
      const draft = await invoke<any>("browser_dock_record_stop");
      // The native page is painted above the app webview; hide it before the
      // review dialog is shown, then reopen on close.
      await invoke("browser_dock_hide").catch(() => undefined);
      recording = false;
      recordPending = false;
      recordCount = 0;
      recordElapsed = 0;
      teachDraft = draft;
      teachOpen = true;
    } catch (cause) {
      browserDock.error = String(cause);
      notify(t("demo.stopFailed"), "error");
      recording = false;
      recordPending = false;
      recordCount = 0;
      recordElapsed = 0;
    } finally {
      recordBusy = false;
    }
  }

  async function cancelRecording() {
    if (!isTauri() || recordBusy) return;
    recordBusy = true;
    stopRecordPoll();
    try {
      await invoke("browser_dock_record_cancel");
    } catch (cause) {
      browserDock.error = String(cause);
    } finally {
      recording = false;
      recordPending = false;
      recordCount = 0;
      recordElapsed = 0;
      recordBusy = false;
    }
  }

  function closeTeach() {
    teachOpen = false;
    teachDraft = null;
    if (browserDock.open && isTauri()) {
      void openDock(browserDock.currentUrl || browserDock.url);
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

  // Switching agents/offices changes the persistent browser profile. The old
  // webview owns the old data directory, so reopen the current page in the new
  // one rather than leaving the two scopes sharing cookies.
  let lastProfile = browserDock.profile;
  $effect(() => {
    const profile = browserDock.profile;
    if (profile === lastProfile) return;
    lastProfile = profile;
    if (browserDock.open) {
      void openDock(browserDock.currentUrl || browserDock.url);
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
    reloadNamedProfiles = () => void loadNamedProfiles();
    window.addEventListener(
      "ravenbot:browser-profiles-changed",
      reloadNamedProfiles,
    );
    void loadNamedProfiles();
    if (!isTauri()) return;

    cookiePolicyListener = () => {
      void refreshCookiePolicy();
    };
    window.addEventListener(
      "ravenbot:browser-cookie-policy-changed",
      cookiePolicyListener,
    );

    void (async () => {
      try {
        unlisteners.push(
          await listen("browser-dock:close", () => {
            close();
          }),
        );
        unlisteners.push(
          await listen<{ url?: string; tab_id?: string }>(
            "browser-dock:request",
            (event) => {
              const url = event.payload?.url;
              const fromPagePopup = Boolean(event.payload?.tab_id);
              if (!url) {
                browserDock.show();
              } else if (fromPagePopup && browserDock.open) {
                void newTab(url);
              } else {
                browserDock.show(url);
              }
            },
          ),
        );
        unlisteners.push(
          await listen<{ url?: string; event?: string; tab_id?: string }>(
            "browser-dock:page-load",
            (event) => {
              const tabId = event.payload?.tab_id;
              const url = event.payload?.url ?? "";
              const phase = event.payload?.event ?? "finished";
              browserDock.pageLoad(url, phase, tabId);
              const active =
                !tabId || tabId === browserDock.activeTabId;
              if (active && url) address = url;
              if (active && phase === "started") browserDock.loading = true;
              if (phase === "finished" && tabId && url) {
                scheduleCookieReject(tabId, url);
              }
              if (active && phase === "finished") {
                // The title signal can arrive before or after load depending
                // on the engine. One small DOM read makes it deterministic and
                // also exercises the same evaluator the agent tools use.
                void invoke<string>("browser_dock_eval", { script: "document.title" })
                  .then((title) => {
                    if (typeof title === "string" && title.trim()) {
                      browserDock.setTitle(
                        title.trim(),
                        tabId ?? browserDock.activeTabId,
                      );
                    }
                  })
                  .catch(() => undefined);
              }
            },
          ),
        );
        unlisteners.push(
          await listen<{ title?: string; tab_id?: string }>(
            "browser-dock:title",
            (event) => {
              browserDock.setTitle(
                event.payload?.title ?? "",
                event.payload?.tab_id,
              );
            },
          ),
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
    if (reloadNamedProfiles) {
      window.removeEventListener(
        "ravenbot:browser-profiles-changed",
        reloadNamedProfiles,
      );
    }
    if (frame) cancelAnimationFrame(frame);
    stopRecordPoll();
    if ((recording || recordPending) && isTauri()) {
      void invoke("browser_dock_record_cancel").catch(() => undefined);
    }
    if (cookiePolicyListener) {
      window.removeEventListener(
        "ravenbot:browser-cookie-policy-changed",
        cookiePolicyListener,
      );
    }
    clearCookieTimers();
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

      <div class="browser-dock__zoom" role="group" aria-label={t("a11y.pageZoom")}>
        <button
          type="button"
          class="browser-dock__button"
          title="Zoom out"
          aria-label={t("a11y.zoomOut")}
          onclick={() => void setZoom(zoom - 0.1)}
        >
          <ZoomOut class="size-3.5" />
        </button>
        <button
          type="button"
          class="browser-dock__zoom-value"
          title="Reset zoom to 100%"
          aria-label={`Page zoom ${Math.round(zoom * 100)} percent`}
          onclick={() => void setZoom(1)}
        >
          {Math.round(zoom * 100)}%
        </button>
        <button
          type="button"
          class="browser-dock__button"
          title="Zoom in"
          aria-label={t("a11y.zoomIn")}
          onclick={() => void setZoom(zoom + 0.1)}
        >
          <ZoomIn class="size-3.5" />
        </button>
      </div>

      <button
        type="button"
        class="browser-dock__button browser-dock__teach"
        class:is-recording={recording || recordPending}
        title={t("demo.teachHint")}
        aria-label={t("demo.teach")}
        aria-pressed={recording || recordPending}
        aria-busy={recordBusy}
        disabled={recording || recordPending || recordBusy || !browserDock.activeTabId}
        onclick={startRecording}
      >
        {#if recordBusy && !recording && !recordPending}
          <Loader2 class="size-4 animate-spin" />
        {:else}
          <GraduationCap class="size-4" />
        {/if}
      </button>

      {#if recording || recordPending}
        <div class="browser-dock__record" role="group" aria-label={t("demo.recordingTitle")}>
          <span
            class="browser-dock__record-dot"
            class:is-stopped={!recording}
            aria-hidden="true"
          ></span>
          <span class="browser-dock__record-time" title={t("demo.elapsed", { time: elapsedText(recordElapsed) })}>
            {elapsedText(recordElapsed)}
          </span>
          <span class="browser-dock__record-count" aria-label={t("demo.status", { count: recordCount, time: elapsedText(recordElapsed) })}>
            {t("demo.actionCount", { count: recordCount })}
          </span>
          <button
            type="button"
            class="browser-dock__button browser-dock__record-stop"
            title={recording ? t("demo.stop") : t("demo.reviewRecording")}
            aria-label={recording ? t("demo.stop") : t("demo.reviewRecording")}
            aria-busy={recordBusy}
            disabled={recordBusy}
            onclick={stopRecording}
          >
            {#if recordBusy}
              <Loader2 class="size-3.5 animate-spin" />
            {:else}
              <Square class="size-3.5" />
            {/if}
          </button>
          <button
            type="button"
            class="browser-dock__button"
            title={t("demo.cancel")}
            aria-label={t("demo.cancel")}
            disabled={recordBusy}
            onclick={cancelRecording}
          >
            <X class="size-3.5" />
          </button>
        </div>
      {/if}

      <button
        type="button"
        class="browser-dock__button"
        title={t("browser.cookieRefuse")}
        aria-label={t("browser.cookieRefuse")}
        onclick={() => void refuseCookies()}
      >
        <Cookie class="size-4" />
      </button>

      <button
        type="button"
        class="browser-dock__button"
        title={t("browser.secureInput")}
        aria-label={t("browser.secureInput")}
        onclick={openSecure}
        disabled={!browserDock.open}
      >
        <Lock class="size-4" />
      </button>

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

    {#if browserDock.tabs.length > 0}
      <!--
        A group, not a tablist. Each tab owns a close button, and a tablist's
        required children are tabs, not wrappers containing two buttons. The
        invalid roles failed axe; `aria-pressed` still carries the active state.
      -->
      <div class="browser-dock__tabs" role="group" aria-label={t("browser.tabs")}>
        {#each browserDock.tabs as tab (tab.id)}
          <div
            class="browser-dock__tab"
            role="presentation"
            class:is-active={tab.id === browserDock.activeTabId}
            draggable="true"
            ondragstart={(event) => startTabDrag(event, tab.id)}
            ondragover={(event) => event.preventDefault()}
            ondrop={(event) => dropTab(event, tab.id)}
          >
            <button
              type="button"
              aria-pressed={tab.id === browserDock.activeTabId}
              class="browser-dock__tab-label"
              title={tab.title || tab.url}
              onclick={() => void switchTab(tab.id)}
            >{tabLabel(tab)}</button>
            <button
              type="button"
              class="browser-dock__tab-close"
              title={t("browser.closeTab")}
              aria-label={t("browser.closeTab")}
              onclick={(event) => {
                event.stopPropagation();
                void closeTab(tab.id);
              }}
            >
              <X class="size-3" />
            </button>
          </div>
        {/each}
        <button
          type="button"
          class="browser-dock__tab-new"
          title={t("browser.newTab")}
          aria-label={t("browser.newTab")}
          onclick={() => void newTab()}
        >
          <Plus class="size-3.5" />
        </button>
      </div>
    {/if}

    <div class="browser-dock__caption" title={browserDock.title || address}>
      <select
        class="browser-dock__profile"
        value={selectedProfile}
        onchange={chooseProfile}
        aria-label={t("browser.profile")}
        title={t("browser.profile")}
      >
        <option value="__auto__">{t("browser.profileAuto")}</option>
        {#each browserDock.namedProfiles as profile (profile.id)}
          <option value={profile.id}>{profile.name}</option>
        {/each}
      </select>
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

  {#if secureOpen}
    <div
      class="downloads-overlay"
      role="dialog"
      aria-modal="true"
      aria-label={t("browser.secureTitle")}
      tabindex="-1"
      onkeydown={(event) => {
        if (event.key === "Escape") closeSecure();
      }}
      onclick={(event) => {
        if (event.target === event.currentTarget) closeSecure();
      }}
    >
      <form
        class="downloads-dialog__panel browser-secret"
        onsubmit={(event) => {
          event.preventDefault();
          void fillSecure();
        }}
      >
        <header class="downloads-dialog__head">
          <div>
            <h2><ShieldCheck class="size-4 inline mr-1" />{t("browser.secureTitle")}</h2>
            <code>{secureOrigin || t("browser.title")}</code>
          </div>
          <button type="button" onclick={closeSecure} aria-label={t("ui.close")} title={t("ui.close")}>
            <X class="size-3.5" />
          </button>
        </header>
        <div class="browser-secret__body">
          <p>{t("browser.secureDesc")}</p>
          <div class="browser-secret__modes">
            <button
              type="button"
              class="browser-secret__mode"
              data-active={secureMode === "password" ? "" : undefined}
              onclick={() => (secureMode = "password")}
            >{t("browser.securePassword")}</button>
            <button
              type="button"
              class="browser-secret__mode"
              data-active={secureMode === "otp" ? "" : undefined}
              onclick={() => (secureMode = "otp")}
            >{t("browser.secureCode")}</button>
          </div>
          <input
            class="browser-secret__input"
            type="password"
            autocomplete="off"
            spellcheck="false"
            bind:value={secureValue}
            placeholder={secureMode === "otp" ? t("browser.secureCode") : t("browser.securePassword")}
            aria-label={secureMode === "otp" ? t("browser.secureCode") : t("browser.securePassword")}
            disabled={secureBusy}
          />
          {#if secureError}<p class="browser-secret__error" role="alert">{secureError}</p>{/if}
          <div class="browser-secret__actions">
            <button type="button" class="browser-secret__cancel" onclick={closeSecure}>{t("ui.cancel")}</button>
            {#if secureMode === "password"}
              <button
                type="button"
                class="browser-secret__cancel"
                onclick={() => void openSaveToVault()}
                disabled={!secureValue || secureBusy}
              >{t("browser.saveToVault")}</button>
            {/if}
            <button type="submit" class="browser-secret__submit" disabled={!secureValue || secureBusy}>
              {secureBusy ? t("ui.saving") : t("browser.secureFill")}
            </button>
          </div>
          {#if saveToVaultOpen && secureMode === "password"}
            <BrowserVaultSave
              origin={secureOrigin}
              initialName={saveName}
              initialUsername={saveUsername}
              secret={secureValue}
              onCancel={() => (saveToVaultOpen = false)}
              onSaved={savedToVault}
            />
          {/if}
        </div>
      </form>
    </div>
  {/if}

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

  {#if teachOpen && teachDraft}
    <LearnFromRun
      initialDraft={teachDraft}
      titleKey="learning.teachTitle"
      onClose={closeTeach}
    />
  {/if}
{/if}
