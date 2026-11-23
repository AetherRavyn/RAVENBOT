// @vitest-environment jsdom
import { describe, it, expect, beforeEach } from "vitest";
import { BrowserDockStore } from "$lib/browserDock.svelte";

describe("BrowserDockStore", () => {
  let dock: BrowserDockStore;

  beforeEach(() => {
    localStorage.clear();
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

  it("keeps URL and title state separate per tab", () => {
    dock.setTabs(
      [
        { id: "a", title: "Alpha", url: "https://alpha.example" },
        { id: "b", title: "Beta", url: "https://beta.example" },
      ],
      "a",
    );
    expect(dock.activeTabId).toBe("a");
    expect(dock.currentUrl).toBe("https://alpha.example");

    dock.pageLoad("https://alpha.example/login", "started", "a");
    dock.pageLoad("https://beta.example/login", "finished", "b");
    expect(dock.currentUrl).toBe("https://alpha.example/login");
    expect(dock.tabs.find((tab) => tab.id === "b")?.url).toBe(
      "https://beta.example/login",
    );

    dock.setTitle("Beta login", "b");
    expect(dock.title).toBe("Alpha");
    dock.setActiveTab("b");
    expect(dock.currentUrl).toBe("https://beta.example/login");
    expect(dock.title).toBe("Beta login");
  });

  it("reorders tabs locally and returns the new id order", () => {
    dock.setTabs(
      [
        { id: "a", title: "A", url: "https://a.example" },
        { id: "b", title: "B", url: "https://b.example" },
        { id: "c", title: "C", url: "https://c.example" },
      ],
      "b",
    );
    expect(dock.reorderTabs("c", "a")).toEqual(["c", "a", "b"]);
    expect(dock.tabs.map((tab) => tab.id)).toEqual(["c", "a", "b"]);
    expect(dock.activeTabId).toBe("b");
    expect(dock.reorderTabs("a", "a")).toBeNull();
    expect(dock.reorderTabs("missing", "a")).toBeNull();
  });

  it("applies named profiles per scope and falls back when one is removed", () => {
    dock.setScope("agent-1", "agent-a");
    expect(dock.profile).toBe("agent-a");
    dock.setNamedProfiles([
      {
        id: "named-work",
        name: "Work",
        created_at: "",
        updated_at: "",
      },
    ]);
    dock.setProfileOverride("agent-1", "named-work");
    expect(dock.profile).toBe("named-work");
    dock.setScope("office-1", "office-o");
    expect(dock.profile).toBe("office-o");
    dock.setProfileOverride("office-1", "named-work");
    expect(dock.profile).toBe("named-work");
    dock.setScope("agent-1", "agent-a");
    expect(dock.profile).toBe("named-work");

    dock.setNamedProfiles([]);
    expect(dock.profile).toBe("agent-a");
  });

  it("releases take-over when the panel is hidden", () => {
    dock.show("https://example.com");
    dock.takenOver = true;
    dock.hide();
    expect(dock.takenOver).toBe(false);
  });
});
