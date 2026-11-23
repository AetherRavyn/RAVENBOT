// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import SidePanel from "$lib/components/workspace/SidePanel.svelte";
import { sidePanel } from "$lib/workspace/sidePanel.svelte";
import { browserDock } from "$lib/browserDock.svelte";
import { agentActivity } from "$lib/agentActivity.svelte";
import { setLocale } from "$lib/i18n";

describe("SidePanel overlay chrome", () => {
  beforeEach(() => {
    cleanup();
    setLocale("en");
    sidePanel.close("home");
    browserDock.hide();
    browserDock.setTabs([], "");
    browserDock.setNamedProfiles([]);
    browserDock.profileOverrides = {};
    agentActivity.clear();
    localStorage.clear();
    browserDock.setScope("home", "shared");
  });

  afterEach(() => {
    cleanup();
    sidePanel.close("home");
    browserDock.hide();
    agentActivity.clear();
  });

  it("renders the browser chrome above the page in overlay mode", async () => {
    sidePanel.select("browser", "home");
    const { container } = render(SidePanel, { props: { bots: [], overlay: true } });

    const panel = container.querySelector(".side-panel");
    expect(panel).toBeTruthy();
    expect(panel?.classList.contains("side-panel--overlay")).toBe(true);
    expect(panel?.hasAttribute("hidden")).toBe(false);

    // The chrome is what the native page must sit below; if it is missing in
    // the DOM the user sees a page with no address bar.
    expect(container.querySelector(".side-panel__bar")).toBeTruthy();
    expect(container.querySelector(".browser-dock__header")).toBeTruthy();
    expect(container.querySelector(".browser-dock__address")).toBeTruthy();
    expect(container.querySelector(".browser-dock__zoom")?.textContent).toContain("80%");
    expect(container.querySelectorAll(".side-panel__tab").length).toBe(5);
  });
});
