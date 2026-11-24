// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import RoutinesPanel from "./RoutinesPanel.svelte";
import { setLocale } from "$lib/i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);

const bot = { id: "bot-1", name: "Raven", description: "", avatar_url: "" };

const routine = {
  id: "routine-1",
  bot_id: "bot-1",
  name: "Nightly deploy",
  description: "",
  schedule: "0 9 * * *",
  instruction: "Check the deploy",
  is_enabled: true,
  last_run_at: null,
  created_at: "2026-11-20T00:00:00Z",
  updated_at: "2026-11-20T00:00:00Z",
};

const server = {
  id: "github",
  name: "GitHub",
  transport: "stdio",
  enabled: true,
};

const trigger = {
  id: "trigger-1",
  routine_id: "routine-1",
  server_id: "github",
  event: "notifications/message",
  resource_uri: "",
  match_text: "",
  cooldown_secs: 60,
  enabled: true,
  last_fired_at: null,
  last_error: "",
  created_at: "2026-11-20T00:00:00Z",
  updated_at: "2026-11-20T00:00:00Z",
};

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
    })) as unknown as typeof window.matchMedia;
  }
  if (!globalThis.ResizeObserver) {
    globalThis.ResizeObserver = class {
      observe() {}
      unobserve() {}
      disconnect() {}
    } as unknown as typeof ResizeObserver;
  }

  let triggers: any[] = [];
  invokeMock.mockImplementation(async (command: string) => {
    switch (command) {
      case "get_scheduler_status":
        return { running: true };
      case "list_routines":
        return [routine];
      case "get_routine_webhook_status":
        return { enabled: false };
      case "list_mcp_servers":
        return [server];
      case "list_mcp_event_triggers":
        return triggers;
      case "get_mcp_event_status":
        return [{ server_id: "github", state: "running", detail: "ready" }];
      case "create_mcp_event_trigger":
        triggers = [trigger];
        return trigger;
      default:
        return null;
    }
  });
});

afterEach(cleanup);

describe("RoutinesPanel MCP events", () => {
  it("opens the trigger panel and creates a stdio notification trigger", async () => {
    const { getByLabelText, getByText, getAllByText } = render(RoutinesPanel, {
      props: { bot },
    });
    await waitFor(() => expect(getByText("Nightly deploy")).toBeTruthy());

    await fireEvent.click(getByLabelText("MCP event triggers"));
    await waitFor(() => expect(getAllByText("MCP event triggers").length).toBeGreaterThan(0));
    await waitFor(() => expect(getByText("GitHub")).toBeTruthy());

    await fireEvent.click(getByText("Add trigger"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "create_mcp_event_trigger",
        expect.objectContaining({
          routineId: "routine-1",
          serverId: "github",
          event: "notifications/message",
          cooldownSecs: 60,
        }),
      ),
    );
    await waitFor(() => expect(getAllByText("notifications/message").length).toBeGreaterThan(0));
  });
});
