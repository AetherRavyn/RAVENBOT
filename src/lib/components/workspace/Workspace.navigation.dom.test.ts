// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));

const BOTS = [
  {
    id: "bot-1",
    name: "Raven",
    description: "",
    avatar_url: "",
    avatar_style: "micah",
    is_orchestrator: true,
    config: { model_provider: "ollama", model_id: "llama3.1" },
  },
];
const ROOMS = [
  {
    id: "room-1",
    name: "Ops",
    description: "",
    office_template: "custom",
    avatar_url: "",
    avatar_style: "micah",
    goal: "",
    policy: "",
    terms: "",
    budget: 1,
    project_folders: [],
  },
];

const { default: Workspace } = await import("./Workspace.svelte");
const { workspace } = await import("$lib/workspace.svelte");
const { sidePanel } = await import("$lib/workspace/sidePanel.svelte");
const { browserDock } = await import("$lib/browserDock.svelte");
const { setLocale } = await import("$lib/i18n");

beforeEach(() => {
  cleanup();
  setLocale("en");
  if (!window.matchMedia) {
    window.matchMedia = ((query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: () => {},
      removeListener: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => false,
    })) as typeof window.matchMedia;
  }
  if (!globalThis.ResizeObserver) {
    globalThis.ResizeObserver = class {
      observe() {}
      unobserve() {}
      disconnect() {}
    } as unknown as typeof ResizeObserver;
  }
  workspace.dest = "home";
  workspace.loading = false;
  workspace.bots = BOTS;
  workspace.chatrooms = ROOMS;
  workspace.selectedBotId = null;
  workspace.selectedRoomId = null;
  workspace.showSettings = false;
  sidePanel.open = false;
  sidePanel.active = "browser";
  browserDock.hide();
  vi.mocked(invoke).mockImplementation(async (command: string) => {
    switch (command) {
      case "list_bots":
        return BOTS;
      case "list_chatrooms":
        return ROOMS;
      case "get_status":
        return { kill_switch_active: false };
      case "get_configured_providers":
        return ["ollama"];
      case "get_default_agent":
        return BOTS[0];
      case "probe_ollama":
        return { models: [] };
      case "list_attention_items":
        return [];
      case "computer_takeover":
        return false;
      case "get_kill_switch_status":
        return { state: "Inactive" };
      case "default_workspace_for":
        return "";
      case "get_effective_browser_cookie_policy":
        return false;
      case "list_browser_profiles":
        return [];
      case "list_browser_downloads":
        return [];
      case "browser_downloads_dir":
        return "/tmp";
      case "get_office_active":
      case "get_office_autonomy":
        return false;
      case "get_office_phase":
        return null;
      case "list_pending_approvals":
      case "list_pending_questions":
      case "list_messages":
      case "list_messages_page":
      case "list_chatroom_members":
      case "list_mcp_servers":
      case "list_bot_mcp_servers":
      case "list_office_memories":
      case "list_projects":
      case "file_change_totals":
      case "list_recent_file_changes":
      case "list_channels":
      case "list_routines":
      case "list_bot_todos":
      case "list_all_skills":
      case "list_browser_tabs":
      case "list_threads":
      case "list_local_skills":
      case "list_plugin_catalog":
      case "list_plugins":
      case "get_plugin_catalog":
        return [];
      case "get_chatroom_thread":
        return null;
      case "get_office_board":
        return { goal: "", nodes: [], liveNodeThreads: [] };
      case "get_office_queue":
        return [];
      case "get_office_queue_paused":
        return false;
      case "get_model_catalog":
        return [];
      case "get_default_model":
        return { provider: "ollama", model: "llama3.1" };
      case "get_service_status":
        return { supported: false, installed: false, enabled: false, running: false, linger: false, path: "", detail: "" };
      case "get_remote_status":
        return { enabled: false, running: false, url: "", port: 0, allow_lan: false, devices: [], audit: [] };
      case "get_autostart_enabled":
      case "get_allow_private_tool_network":
        return false;
      case "get_tool_prices":
        return {};
      case "get_current_account":
        return { username: "owner", workspace: "owner", role: "owner", workspace_owner: true };
      case "list_custom_providers":
        return [];
      case "get_ollama_url":
        return "http://localhost:11434";
      default:
        return null;
    }
  });
});

afterEach(cleanup);

describe("workspace navigation", () => {
  it("opens every rail destination without an effect update loop", async () => {
    const { getByLabelText } = render(Workspace);
    await waitFor(() => expect(getByLabelText("Offices")).toBeTruthy());
    const destinations = [
      ["Offices", "offices"],
      ["Projects", "projects"],
      ["Channels", "channels"],
      ["MCPs & Connectors", "connectors"],
      ["Routines", "routines"],
      ["Inbox", "inbox"],
    ] as const;
    for (const [label, dest] of destinations) {
      await fireEvent.click(getByLabelText(label));
      expect(workspace.dest).toBe(dest);
      await new Promise((resolve) => setTimeout(resolve, 80));
    }
  });

  it("opens an office conversation and an agent from the rail/sidebar", async () => {
    const { getByLabelText } = render(Workspace);
    await waitFor(() => expect(getByLabelText("Offices")).toBeTruthy());
    workspace.selectRoom("room-1");
    await new Promise((resolve) => setTimeout(resolve, 100));
    workspace.selectBot("bot-1");
    await new Promise((resolve) => setTimeout(resolve, 100));
    expect(workspace.selectedBotId).toBe("bot-1");
  });

  it("opens the skills, marketplace, and settings modals without an effect loop", async () => {
    const { getAllByLabelText } = render(Workspace);
    for (const label of ["Skills", "Marketplace", "Settings"]) {
      await fireEvent.click(getAllByLabelText(label)[0]);
      await new Promise((resolve) => setTimeout(resolve, 180));
    }
    expect(workspace.showSettings).toBe(true);
  });

  it("keeps the open browser panel stable while switching office and agent", async () => {
    render(Workspace);
    sidePanel.select("browser");
    await new Promise((resolve) => setTimeout(resolve, 100));
    workspace.selectRoom("room-1");
    await new Promise((resolve) => setTimeout(resolve, 100));
    workspace.selectBot("bot-1");
    await new Promise((resolve) => setTimeout(resolve, 100));
    expect(sidePanel.open).toBe(true);
  });

  it("keeps the workspace root selectable and out of application mode", () => {
    const { container } = render(Workspace);
    expect(container.querySelector('[role="application"]')).toBeNull();
    const root = container.firstElementChild as HTMLElement | null;
    expect(root).not.toBeNull();
    expect(root?.classList.contains("select-none")).toBe(false);
  });
});
