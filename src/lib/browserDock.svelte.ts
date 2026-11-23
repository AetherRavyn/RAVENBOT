/**
 * The browser dock's UI state.
 *
 * The page itself is a native child webview owned by Rust; this store holds
 * only what the chrome needs to render. That split matters: the dock keeps its
 * history and logins when the panel is hidden, so the URL the user sees is
 * reported by the page, not assumed from what the address bar last sent.
 */

const DEFAULT_URL = "https://duckduckgo.com/";
const PROFILE_OVERRIDES_KEY = "raven.browserProfileOverrides";

function loadProfileOverrides(): Record<string, string> {
  if (typeof localStorage === "undefined") return {};
  try {
    const raw = JSON.parse(localStorage.getItem(PROFILE_OVERRIDES_KEY) || "{}");
    if (!raw || typeof raw !== "object" || Array.isArray(raw)) return {};
    const result: Record<string, string> = {};
    for (const [scope, profile] of Object.entries(raw)) {
      if (typeof scope === "string" && typeof profile === "string" && profile) {
        result[scope] = profile;
      }
    }
    return result;
  } catch {
    return {};
  }
}

function saveProfileOverrides(overrides: Record<string, string>): void {
  try {
    localStorage.setItem(PROFILE_OVERRIDES_KEY, JSON.stringify(overrides));
  } catch {
    /* storage unavailable; overrides remain session-only */
  }
}

export interface BrowserProfile {
  id: string;
  name: string;
  created_at: string;
  updated_at: string;
  /** "inherit", "always", or "never". */
  cookie_policy?: string;
}

export interface BrowserTab {
  id: string;
  title: string;
  url: string;
}

export interface BrowserDownload {
  id: string;
  name: string;
  path: string;
  url: string;
  state: "downloading" | "done" | "failed" | string;
  size?: number | null;
  started_at: string;
  finished_at?: string | null;
}

export class BrowserDockStore {
  open = $state(false);
  /** Increments on every explicit show, so a second request navigates. */
  nonce = $state(0);
  /** The last URL the user or an agent asked for. */
  url = $state(DEFAULT_URL);
  /** The URL the page actually reports, which redirects and link clicks change. */
  currentUrl = $state("");
  /** Every open native tab, in display order. */
  tabs = $state<BrowserTab[]>([]);
  activeTabId = $state("");
  title = $state("");
  loading = $state(false);
  /** Persistent browser data directory: shared, agent-<id>, office-<id>, or named-<id>. */
  profile = $state("shared");
  /** Current workspace scope and its automatic agent/office profile. */
  scope = $state("home");
  autoProfile = $state("shared");
  /** Named profile overrides keyed by workspace scope. */
  profileOverrides = $state<Record<string, string>>(loadProfileOverrides());
  namedProfiles = $state<BrowserProfile[]>([]);
  /** Set when the native dock could not be created or navigated. */
  error = $state<string | null>(null);
  /** The user has the page; agent DOM actions are refused until released. */
  takenOver = $state(false);
  /** A plain panel open should restore this profile's saved tabs. */
  restoreTabs = $state(false);
  downloads = $state<BrowserDownload[]>([]);
  downloadsOpen = $state(false);

  show(url?: string): void {
    const next = url?.trim();
    if (next) this.url = next;
    this.restoreTabs = !next;
    this.error = null;
    this.open = true;
    this.nonce += 1;
  }

  hide(): void {
    this.open = false;
    this.loading = false;
    this.takenOver = false;
    this.downloadsOpen = false;
  }

  setTabs(tabs: BrowserTab[], active: string): void {
    this.tabs = tabs.map((tab) => ({ ...tab }));
    this.activeTabId = active || this.tabs[0]?.id || "";
    const current = this.tabs.find((tab) => tab.id === this.activeTabId);
    if (current) {
      this.currentUrl = current.url || this.currentUrl;
      this.title = current.title;
    } else if (this.tabs.length === 0) {
      this.currentUrl = "";
      this.title = "";
    }
  }

  /** Reorder tabs locally. Returns the new id order, or null when invalid. */
  reorderTabs(sourceId: string, targetId: string): string[] | null {
    const ids = this.tabs.map((tab) => tab.id);
    const from = ids.indexOf(sourceId);
    const to = ids.indexOf(targetId);
    if (from < 0 || to < 0 || from === to) return null;
    const next = [...ids];
    const [moved] = next.splice(from, 1);
    next.splice(to, 0, moved);
    const byId = new Map(this.tabs.map((tab) => [tab.id, tab]));
    this.tabs = next
      .map((id) => byId.get(id))
      .filter((tab): tab is BrowserTab => Boolean(tab));
    return next;
  }

  setActiveTab(id: string): void {
    const tab = this.tabs.find((candidate) => candidate.id === id);
    if (!tab) return;
    this.activeTabId = id;
    this.currentUrl = tab.url;
    this.title = tab.title;
  }

  setProfile(profile: string): void {
    const next = profile.trim() || "shared";
    if (this.profile !== next) this.profile = next;
  }

  /** Follow the selected agent/office unless this scope has a named override. */
  setScope(scope: string, autoProfile: string): void {
    this.scope = scope || "home";
    this.autoProfile = autoProfile || "shared";
    this.applyProfile();
  }

  /** `null` restores the automatic agent/office profile for this scope. */
  setProfileOverride(scope: string, profile: string | null): void {
    const next = { ...this.profileOverrides };
    if (profile) next[scope] = profile;
    else delete next[scope];
    this.profileOverrides = next;
    saveProfileOverrides(next);
    this.applyProfile();
  }

  setNamedProfiles(profiles: BrowserProfile[]): void {
    this.namedProfiles = profiles.map((profile) => ({ ...profile }));
    const ids = new Set(this.namedProfiles.map((profile) => profile.id));
    let changed = false;
    const next = { ...this.profileOverrides };
    for (const [scope, profile] of Object.entries(next)) {
      if (profile.startsWith("named-") && !ids.has(profile)) {
        delete next[scope];
        changed = true;
      }
    }
    if (changed) {
      this.profileOverrides = next;
      saveProfileOverrides(next);
    }
    this.applyProfile();
  }

  private applyProfile(): void {
    this.setProfile(this.profileOverrides[this.scope] || this.autoProfile);
  }

  setDownloads(downloads: BrowserDownload[]): void {
    this.downloads = downloads;
  }

  upsertDownload(download: BrowserDownload): void {
    const index = this.downloads.findIndex((item) => item.id === download.id);
    if (index >= 0) {
      this.downloads[index] = download;
    } else {
      this.downloads = [download, ...this.downloads];
    }
    this.downloads = this.downloads.slice(0, 100);
  }

  toggle(): void {
    if (this.open) this.hide();
    else this.show();
  }

  pageLoad(url: string, event: string, tabId?: string | null): void {
    if (tabId) {
      this.tabs = this.tabs.map((tab) =>
        tab.id === tabId ? { ...tab, url } : tab,
      );
      if (tabId !== this.activeTabId) return;
    }
    this.currentUrl = url;
    this.loading = event === "started";
    if (url) this.url = url;
  }

  setTitle(title: string, tabId?: string | null): void {
    if (tabId) {
      this.tabs = this.tabs.map((tab) =>
        tab.id === tabId ? { ...tab, title } : tab,
      );
      if (tabId !== this.activeTabId) return;
    }
    this.title = title;
  }
}

export const browserDock = new BrowserDockStore();
