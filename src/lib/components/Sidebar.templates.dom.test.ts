// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import Sidebar from "./Sidebar.svelte";
import { AGENT_TEMPLATES } from "$lib/agentTemplates";
import { drafts, threadDraftKey } from "$lib/chat/drafts";
import { setLocale } from "$lib/i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);
const created = {
  id: "bot-new",
  name: "Research Scout",
  description: "Finds primary sources.",
  specialty: "Research Scout",
  config: { model_provider: "ollama" },
  skills: ["web_search"],
};

describe("Sidebar agent templates", () => {
  beforeEach(() => {
    cleanup();
    setLocale("en");
    window.localStorage.clear();
    invokeMock.mockReset();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "list_bot_contacts") return [];
      if (command === "get_unread_counts") return {};
      if (command === "create_bot") return created;
      return null;
    });
  });

  afterEach(() => {
    cleanup();
    window.localStorage.clear();
    drafts.clear(threadDraftKey(created.id, null));
    vi.restoreAllMocks();
  });

  it("prefills a template, creates a real agent and seeds a review draft", async () => {
    const onBotCreated = vi.fn();
    render(Sidebar, {
      props: {
        bots: [],
        selectedBotId: null,
        onSelectBot: vi.fn(),
        onBotCreated,
        onBotUpdated: vi.fn(),
        onBotDeleted: vi.fn(),
        openSettings: vi.fn(),
      },
    });

    await fireEvent.click(screen.getByRole("button", { name: "Create new AI agent" }));
    await fireEvent.click(screen.getByRole("button", { name: /Research Scout/ }));

    expect((screen.getByLabelText("Agent Name") as HTMLInputElement).value).toBe("Research Scout");
    expect((screen.getByLabelText("Specialization & Mission") as HTMLTextAreaElement).value).toContain(
      "Finds primary sources",
    );

    await fireEvent.click(screen.getByRole("button", { name: "Create Agent" }));
    await waitFor(() => expect(onBotCreated).toHaveBeenCalledWith(created));

    const template = AGENT_TEMPLATES.find((item) => item.id === "research")!;
    expect(invokeMock).toHaveBeenCalledWith(
      "create_bot",
      expect.objectContaining({
        name: "Research Scout",
        prompt: template.prompt,
        skills: template.skills,
        specialty: "Research Scout",
      }),
    );
    expect(drafts.load(threadDraftKey(created.id, null))).toBe(
      "Research the current state of the topic in this workspace and return a short brief with sources.",
    );
  });
});
