/**
 * Open a URL outside the app.
 *
 * The Tauri CSP allows a specific list of provider hosts, and a `target="_blank"`
 * on a webview that is not allowed to navigate is a dead click. Routing through
 * the opener plugin also keeps the app window from ever navigating away, which
 * would strand the user on a blank page with no title bar.
 */
import { openUrl } from "@tauri-apps/plugin-opener";

/**
 * Open `url` externally, falling back to a normal navigation.
 *
 * The fallback matters in the browser preview, where the plugin is absent and
 * a dead button is worse than a new tab.
 */
export async function openExternal(url: string): Promise<void> {
  try {
    await openUrl(url);
  } catch {
    window.open(url, "_blank", "noopener,noreferrer");
  }
}
