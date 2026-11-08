/**
 * The browser dock's UI state.
 *
 * The page itself is a native child webview owned by Rust; this store holds
 * only what the chrome needs to render. That split matters: the dock keeps its
 * history and logins when the panel is hidden, so the URL the user sees is
 * reported by the page, not assumed from what the address bar last sent.
 */

const DEFAULT_URL = "https://duckduckgo.com/";

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
  title = $state("");
  loading = $state(false);
  /** Set when the native dock could not be created or navigated. */
  error = $state<string | null>(null);
  /** The user has the page; agent DOM actions are refused until released. */
  takenOver = $state(false);
  downloads = $state<BrowserDownload[]>([]);
  downloadsOpen = $state(false);

  show(url?: string): void {
    const next = url?.trim();
    if (next) this.url = next;
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

  pageLoad(url: string, event: string): void {
    this.currentUrl = url;
    this.loading = event === "started";
    if (url) this.url = url;
  }

  setTitle(title: string): void {
    this.title = title;
  }
}

export const browserDock = new BrowserDockStore();
