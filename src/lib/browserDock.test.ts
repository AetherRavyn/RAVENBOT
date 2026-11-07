import { describe, it, expect, beforeEach } from "vitest";
import { BrowserDockStore } from "$lib/browserDock.svelte";

describe("BrowserDockStore", () => {
  let dock: BrowserDockStore;

  beforeEach(() => {
    dock = new BrowserDockStore();
  });

  it("opens on a URL and bumps the request nonce", () => {
    dock.show("https://example.com");
    expect(dock.open).toBe(true);
    expect(dock.url).toBe("https://example.com");
    expect(dock.nonce).toBe(1);
    dock.show("https://example.com/other");
    expect(dock.nonce).toBe(2);
  });

  it("tracks the page the webview actually reached", () => {
    dock.show("https://example.com");
    dock.pageLoad("https://example.com/login", "started");
    expect(dock.currentUrl).toBe("https://example.com/login");
    expect(dock.loading).toBe(true);
    dock.pageLoad("https://example.com/login", "finished");
    expect(dock.loading).toBe(false);
  });

  it("hides without discarding the current page", () => {
    dock.show("https://example.com");
    dock.pageLoad("https://example.com/page", "finished");
    dock.hide();
    expect(dock.open).toBe(false);
    expect(dock.currentUrl).toBe("https://example.com/page");
    expect(dock.url).toBe("https://example.com/page");
  });

  it("releases take-over when the panel is hidden", () => {
    dock.show("https://example.com");
    dock.takenOver = true;
    dock.hide();
    expect(dock.takenOver).toBe(false);
  });
});
