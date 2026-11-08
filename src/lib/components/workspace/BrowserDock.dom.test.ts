// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, render, waitFor } from "@testing-library/svelte";
import BrowserDock from "$lib/components/workspace/BrowserDock.svelte";
import { browserDock } from "$lib/browserDock.svelte";
import { agentActivity } from "$lib/agentActivity.svelte";
import { setLocale } from "$lib/i18n";

describe("BrowserDock activity", () => {
  beforeEach(() => {
    cleanup();
    setLocale("en");
    agentActivity.clear();
    browserDock.hide();
  });

  afterEach(() => {
    cleanup();
    browserDock.hide();
    agentActivity.clear();
  });

  it("names the agent and the last browser action", async () => {
    agentActivity.record({
      surface: "browser",
      bot_id: "bot-1",
      action: "click",
      detail: "click #submit",
      ok: true,
      at: new Date().toISOString(),
    });
    browserDock.show("https://example.com");
    render(BrowserDock, {
      props: {
        bots: [{ id: "bot-1", name: "Billing Agent" }],
      },
    });
    await waitFor(() =>
      expect(document.body.textContent).toContain("Billing Agent"),
    );
    expect(document.body.textContent).toContain("click #submit");
    expect(document.querySelector(".browser-dock__activity")).not.toBeNull();
  });
});
