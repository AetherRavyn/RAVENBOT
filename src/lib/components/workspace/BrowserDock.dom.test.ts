// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import BrowserDock from "$lib/components/workspace/BrowserDock.svelte";
import { browserDock } from "$lib/browserDock.svelte";
import { agentActivity } from "$lib/agentActivity.svelte";
import { setLocale } from "$lib/i18n";
import { a11yViolations } from "$lib/test/a11y";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

const invokeMock = vi.mocked(invoke);

describe("BrowserDock activity", () => {
  beforeEach(() => {
    cleanup();
    setLocale("en");
    invokeMock.mockReset();
    agentActivity.clear();
    browserDock.hide();
    browserDock.setTabs([], "");
    browserDock.setNamedProfiles([]);
    browserDock.profileOverrides = {};
    localStorage.clear();
    browserDock.setScope("home", "shared");
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  });

  afterEach(() => {
    cleanup();
    browserDock.hide();
    browserDock.setTabs([], "");
    agentActivity.clear();
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
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

  it("records a demonstration and opens the Teach review dialog", async () => {
    const draft = {
      id: "demo-example-com-20261126t100000",
      name: "Demonstrated example.com workflow",
      description: "A reviewed browser workflow demonstrated on example.com.",
      example_prompt: "Replay the browser workflow demonstrated on example.com.",
      content:
        "---\nname: Demonstrated example.com workflow\ndescription: reviewed\nexample-prompt: replay\n---\n\n## Steps\n\n1. **browser_dom** — Click Search\n",
      steps: [
        {
          tool: "browser_dom",
          action: "click",
          summary: "Click Search",
          arguments: { action: "click", selector: "#go" },
        },
      ],
      tools: ["browser_dom"],
      warnings: ["This draft records a demonstration."],
      source_run_id: "demo:2026-11-26T10:00:00Z",
      source_bot_name: "you",
      demonstration: {
        version: 1,
        start_url: "https://example.com",
        recorded_at: "2026-11-26T10:00:00Z",
        actions: [],
      },
    };
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "browser_dock_record_start") {
        return {
          recording: true,
          count: 0,
          tab_id: "a",
          elapsed_ms: 0,
          auto_stopped: false,
        };
      }
      if (command === "browser_dock_record_status") {
        return {
          recording: true,
          count: 2,
          tab_id: "a",
          elapsed_ms: 1200,
          auto_stopped: false,
        };
      }
      if (command === "browser_dock_record_stop") return draft;
      if (command === "browser_dock_hide") return undefined;
      return undefined;
    });
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    browserDock.show("https://example.com");
    browserDock.setTabs(
      [{ id: "a", title: "Example", url: "https://example.com" }],
      "a",
    );
    const { container, getByRole, getByText, queryByRole } = render(BrowserDock, {
      props: { bots: [] },
    });

    const teach = await waitFor(() => getByRole("button", { name: "Teach" }));
    expect(teach.getAttribute("aria-pressed")).toBe("false");
    await fireEvent.click(teach);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("browser_dock_record_start"),
    );
    await waitFor(() =>
      expect(getByRole("group", { name: "Recording a demonstration" })).toBeTruthy(),
    );
    expect(getByRole("button", { name: "Stop recording" })).toBeTruthy();
    expect(queryByRole("button", { name: "Teach" })?.getAttribute("aria-pressed")).toBe(
      "true",
    );
    expect(await a11yViolations(container)).toEqual([]);

    await fireEvent.click(getByRole("button", { name: "Stop recording" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("browser_dock_record_stop"),
    );
    await waitFor(() => expect(getByText("Teach this workflow")).toBeTruthy());
    expect(getByText("Click Search")).toBeTruthy();
    expect(invokeMock).toHaveBeenCalledWith("browser_dock_hide");
  });
});
