// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from "vitest";
import { sidePanel } from "./sidePanel.svelte";
import { workspace } from "$lib/workspace.svelte";
import { browserDock } from "$lib/browserDock.svelte";

beforeEach(() => {
  sidePanel.open = false;
  sidePanel.active = "browser";
  workspace.selectedBotId = null;
  workspace.selectedRoomId = null;
  browserDock.hide();
});

describe("side panel", () => {
  it("remembers the active tab per selection", () => {
    workspace.selectedBotId = "bot-1";
    sidePanel.select("files");
    expect(sidePanel.open).toBe(true);
    expect(sidePanel.active).toBe("files");

    workspace.selectedBotId = "bot-2";
    sidePanel.restore();
    expect(sidePanel.active).toBe("browser");

    workspace.selectedBotId = "bot-1";
    sidePanel.restore();
    expect(sidePanel.active).toBe("files");
  });

  it("toggles the same tab closed and opens a different tab", () => {
    workspace.selectedBotId = "bot-1";
    sidePanel.toggleForSelection("browser");
    expect(sidePanel.open).toBe(true);
    sidePanel.toggleForSelection("browser");
    expect(sidePanel.open).toBe(false);

    sidePanel.toggleForSelection("files");
    expect(sidePanel.open).toBe(true);
    expect(sidePanel.active).toBe("files");
    expect(browserDock.open).toBe(false);
  });

  it("brings the browser tab forward when an agent opens it", () => {
    workspace.selectedBotId = "bot-1";
    sidePanel.select("files");
    browserDock.show("https://example.com");
    sidePanel.revealBrowser();
    expect(sidePanel.open).toBe(true);
    expect(sidePanel.active).toBe("browser");
  });
});
