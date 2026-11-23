// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import BrowserDock from "$lib/components/workspace/BrowserDock.svelte";
import { browserDock } from "$lib/browserDock.svelte";
import { agentActivity } from "$lib/agentActivity.svelte";
import { setLocale } from "$lib/i18n";
import { a11yViolations } from "$lib/test/a11y";

describe("BrowserDock activity", () => {
  beforeEach(() => {
    cleanup();
    setLocale("en");
    agentActivity.clear();
    browserDock.hide();
    browserDock.setTabs([], "");
    browserDock.setNamedProfiles([]);
    browserDock.profileOverrides = {};
    localStorage.clear();
    browserDock.setScope("home", "shared");
  });

  afterEach(() => {
    cleanup();
    browserDock.hide();
    browserDock.setTabs([], "");
    agentActivity.clear();
  });

  it("renders the native tabs and their active state", async () => {
    browserDock.show("https://alpha.example");
    browserDock.setTabs(
      [
        { id: "a", title: "Alpha", url: "https://alpha.example" },
        { id: "b", title: "", url: "https://beta.example" },
      ],
      "a",
    );
    const { container } = render(BrowserDock, { props: { bots: [] } });
    await waitFor(() => expect(document.body.textContent).toContain("Alpha"));
    expect(document.body.textContent).toContain("beta.example");
    expect(await a11yViolations(container), JSON.stringify(await a11yViolations(container))).toEqual([]);
    expect(
      document.querySelector('.browser-dock__tabs[role="group"]')?.getAttribute("aria-label"),
    ).toBe("Browser tabs");
    expect(
      document.querySelector(".browser-dock__tab-label")?.getAttribute("aria-pressed"),
    ).toBe("true");
    expect(document.querySelectorAll(".browser-dock__tab").length).toBe(2);
    expect(document.querySelector(".browser-dock__zoom")?.textContent).toContain("80%");
  });

  it("selects a named profile for the current scope", async () => {
    browserDock.setScope("home", "shared");
    browserDock.setNamedProfiles([
      {
        id: "named-work",
        name: "Work",
        created_at: "",
        updated_at: "",
      },
    ]);
    browserDock.show("https://example.com");
    const { getByLabelText } = render(BrowserDock, { props: { bots: [] } });
    const select = getByLabelText("Browser profile") as HTMLSelectElement;
    await fireEvent.change(select, { target: { value: "named-work" } });
    expect(browserDock.profile).toBe("named-work");
    expect(browserDock.profileOverrides.home).toBe("named-work");
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
