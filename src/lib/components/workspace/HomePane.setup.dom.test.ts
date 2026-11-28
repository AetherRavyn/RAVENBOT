// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import HomePane from "./HomePane.svelte";
import { workspace } from "$lib/workspace.svelte";
import { setLocale } from "$lib/i18n";
import { THEMES } from "$lib/theme";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);
let providers: string[] = [];
let agent: any = null;

function mockSetup() {
  invokeMock.mockImplementation(async (command: string) => {
    if (command === "get_configured_providers") return providers;
    if (command === "get_default_agent") return agent;
    if (command === "probe_ollama") return { models: [] };
    if (command === "get_kill_switch_status") return { state: "Inactive", reason: "" };
    return null;
  });
}

describe("HomePane setup checklist", () => {
  beforeEach(() => {
    cleanup();
    setLocale("en");
    providers = [];
    agent = null;
    workspace.chatrooms = [];
    workspace.bots = [];
    workspace.showSettings = false;
    mockSetup();
  });

  afterEach(() => {
    cleanup();
    workspace.chatrooms = [];
    workspace.bots = [];
    workspace.showSettings = false;
    vi.restoreAllMocks();
  });

  it("shows the checklist after probing and opens provider settings", async () => {
    const { container } = render(HomePane, { props: { theme: THEMES[0] } });
    await waitFor(() => expect(container.textContent).toContain("Set up RAVENBOT"));
    expect(container.textContent).toContain("0 of 3 complete");

    await fireEvent.click(screen.getByRole("button", { name: /Open provider settings/ }));
    expect(workspace.showSettings).toBe(true);
  });

  it("opens the first-office creator when the office step is next", async () => {
    providers = ["openai"];
    agent = { id: "raven", config: { model_provider: "openai" } };
    const { container } = render(HomePane, { props: { theme: THEMES[0] } });
    await waitFor(() => expect(container.textContent).toContain("Create your first office"));

    await fireEvent.click(screen.getByRole("button", { name: /Create office/ }));
    await waitFor(() => expect(document.body.textContent).toContain("New Office"));
  });

  it("reloads the facts when Settings closes and hides when complete", async () => {
    const { container } = render(HomePane, { props: { theme: THEMES[0] } });
    await waitFor(() => expect(container.textContent).toContain("Set up RAVENBOT"));

    providers = ["openai"];
    agent = { id: "raven", config: { model_provider: "openai" } };
    workspace.chatrooms = [{ id: "office-1", name: "Office" }];
    window.dispatchEvent(new CustomEvent("settings-closed"));

    await waitFor(() => expect(container.textContent).not.toContain("Set up RAVENBOT"));
  });
});
