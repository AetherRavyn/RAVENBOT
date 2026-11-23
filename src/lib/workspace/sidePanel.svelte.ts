/**
 * The right-hand workspace panel.
 *
 * The browser, the files browser, and the attention inbox are three views of
 * the same selected agent/office, so they share one panel rather than three
 * independent overlays. The active tab is remembered per scope: switching from
 * an agent to an office and back returns to the tab that scope was using.
 */
import { invoke } from "@tauri-apps/api/core";
import { browserDock } from "$lib/browserDock.svelte";
import { workspace } from "$lib/workspace.svelte";

export type SidePanelTab = "browser" | "files" | "inbox" | "artifacts" | "vault";

const TABS: readonly SidePanelTab[] = ["browser", "files", "inbox", "artifacts", "vault"];

function isTab(value: string | undefined): value is SidePanelTab {
  return Boolean(value && (TABS as readonly string[]).includes(value));
}

class SidePanelStore {
  open = $state(false);
  active = $state<SidePanelTab>("browser");

  private remembered = new Map<string, SidePanelTab>();

  scope(): string {
    return workspace.selectedRoomId ?? workspace.selectedBotId ?? "home";
  }

  private remember(scope: string, tab: SidePanelTab): void {
    this.remembered.set(scope, tab);
  }

  select(tab: SidePanelTab, scope = this.scope()): void {
    this.open = true;
    this.active = tab;
    this.remember(scope, tab);
    if (tab === "browser") {
      if (!browserDock.open) browserDock.show();
    } else {
      browserDock.hide();
      // The native page is a separate child webview; hiding the component
      // state alone does not always hide its window on Linux.
      void invoke("browser_dock_hide").catch(() => undefined);
    }
  }

  toggle(tab: SidePanelTab, scope = this.scope()): void {
    if (this.open && this.active === tab) this.close(scope);
    else this.select(tab, scope);
  }

  toggleForSelection(tab: SidePanelTab): void {
    this.toggle(tab, this.scope());
  }

  close(scope = this.scope()): void {
    this.remember(scope, this.active);
    browserDock.hide();
    // Belt-and-braces: the child webview does not inherit the aside's
    // `hidden` attribute, so hide its window explicitly as well.
    void invoke("browser_dock_hide").catch(() => undefined);
    this.open = false;
  }

  /** Restore the tab this scope used before; do not force a browser reload. */
  restore(scope = this.scope()): void {
    const remembered = this.remembered.get(scope);
    this.active = isTab(remembered) ? remembered : "browser";
    if (!this.open) return;
    if (this.active === "browser") {
      if (!browserDock.open) browserDock.show();
    } else {
      browserDock.hide();
    }
  }

  /** An agent opened the browser outside the panel; bring the tab forward. */
  revealBrowser(): void {
    if (!browserDock.open) return;
    this.open = true;
    this.active = "browser";
    this.remember(this.scope(), "browser");
  }
}

export const sidePanel = new SidePanelStore();
