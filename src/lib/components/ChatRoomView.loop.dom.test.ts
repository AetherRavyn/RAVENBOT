// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));

const room = {
  id: "office-1",
  name: "ARGUS",
  description: "A dashboard office",
  office_template: "custom",
  avatar_url: "",
  avatar_style: "micah",
  goal: "Build the dashboard",
  policy: "Free APIs only",
  terms: "",
  budget: 10,
  project_folders: [],
};
const bots = Array.from({ length: 7 }, (_, index) => ({
  id: `bot-${index + 1}`,
  name: `Agent ${index + 1}`,
  description: "",
  avatar_url: "",
  avatar_style: "micah",
  is_orchestrator: index === 0,
  status: "idle",
  config: { model_provider: "ollama", model_id: "llama3.1" },
}));
const members = bots.map((bot, index) => ({
  bot_id: bot.id,
  rank: ["CEO", "Planner", "Architect", "Developer", "QA Engineer", "Quality Assurance", "DevOps"][index],
  specialty: "Office work",
  bot,
}));
const messages = [
  {
    id: "m1",
    thread_id: "thread-1",
    role: "user",
    content: { type: "Text", text: "Build the ARGUS dashboard" },
    created_at: "2026-11-21T04:00:00Z",
  },
  {
    id: "m2",
    thread_id: "thread-1",
    role: "assistant",
    content: { type: "Text", text: "The team is on it." },
    // A persisted context estimate is what makes `hydrate()` run from the
    // transcript effect. Without untrack inside the store, the merge reads
    // `byThread` and the effect re-triggers itself forever.
    context_used_tokens: 4200,
    context_window_tokens: 128000,
    context_compacted: false,
    context_system_tokens: 1200,
    context_tool_tokens: 300,
    context_memory_tokens: 200,
    context_message_tokens: 2500,
    created_at: "2026-11-21T04:00:05Z",
  },
];

const { default: ChatRoomView } = await import("./ChatRoomView.svelte");
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
  vi.mocked(invoke).mockImplementation(async (command: string) => {
    switch (command) {
      case "list_chatroom_members":
        return members;
      case "get_office_autonomy":
        return true;
      case "list_bot_todos":
      case "get_office_queue":
      case "list_pending_approvals":
      case "list_pending_questions":
      case "list_mcp_servers":
      case "list_bot_mcp_servers":
      case "list_all_skills":
      case "list_recent_file_changes":
      case "list_channels":
      case "list_office_memories":
      case "search_messages":
        return [];
      case "get_office_queue_paused":
        return false;
      case "get_chatroom_thread":
        return "thread-1";
      case "list_messages_page":
        return { messages, has_more: false };
      case "get_office_board":
        return { goal: "", nodes: [], liveNodeThreads: [] };
      case "get_office_active":
        return false;
      case "get_office_phase":
        return null;
      case "get_default_model":
        return { provider: "ollama", model: "llama3.1" };
      case "get_office_alert_settings":
        return { enabled: false, email: "", webhook_url: "" };
      default:
        return null;
    }
  });
});

afterEach(cleanup);

describe("office conversation", () => {
  it("mounts and opens the office settings without an effect loop", async () => {
    const { getByLabelText } = render(ChatRoomView, { props: { room, bots } });
    await waitFor(() => expect(getByLabelText("Office settings")).toBeTruthy());
    await new Promise((resolve) => setTimeout(resolve, 150));
    await fireEvent.click(getByLabelText("Office settings"));
    await new Promise((resolve) => setTimeout(resolve, 150));
    expect(document.body.textContent).toContain("Office settings");
    const save = Array.from(document.querySelectorAll("button")).find((button) =>
      button.textContent?.includes("Save changes"),
    );
    if (save) {
      await fireEvent.click(save);
      await new Promise((resolve) => setTimeout(resolve, 300));
    }
    expect(document.body.textContent).toContain("Office settings");
  });

  it("switches offices with a draft without an effect loop", async () => {
    const { rerender } = render(ChatRoomView, { props: { room, bots } });
    await waitFor(() => expect(document.body.textContent).toContain("ARGUS"));
    const composer = document.querySelector("textarea");
    if (composer) {
      await fireEvent.input(composer, { target: { value: "unfinished draft" } });
    }
    await new Promise((resolve) => setTimeout(resolve, 80));
    await rerender({
      room: { ...room, id: "office-2", name: "ARGUS II" },
      bots,
    });
    await new Promise((resolve) => setTimeout(resolve, 150));
    expect(document.body.textContent).toContain("ARGUS II");
  });
});
