// @vitest-environment jsdom
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, cleanup, fireEvent, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import TeamChannelsView from "$lib/components/workspace/TeamChannelsView.svelte";
import { setLocale } from "$lib/i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => undefined),
}));

const invokeMock = vi.mocked(invoke);

const channel = {
  id: "channel-1",
  name: "Platform",
  description: "Ship the platform",
  instructions: "",
  working_folder: null,
  color: null,
  position: 0,
  bot_ids: ["bot-1", "bot-2"],
};

const message = {
  id: "m1",
  channel_id: "channel-1",
  author_kind: "agent",
  author_id: "bot-1",
  author_name: "Lead",
  content: "The migration is ready.",
  reply_to_id: null,
  created_at: "2026-01-01T10:00:00Z",
};

const task = {
  id: "t1",
  channel_id: "channel-1",
  title: "Review the migration",
  description: "Check the indexes",
  owner_bot_id: "bot-1",
  state: "pending",
  created_by: "You",
  source_thread_id: null,
  result: null,
  created_at: "2026-01-01T10:00:00Z",
  updated_at: "2026-01-01T10:00:00Z",
  archived_at: null,
};

describe("TeamChannelsView", () => {
  beforeEach(() => {
    cleanup();
    setLocale("en");
    invokeMock.mockReset();
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command === "list_channels") return [channel];
      if (command === "list_bots")
        return [
          { id: "bot-1", name: "Lead" },
          { id: "bot-2", name: "Backend" },
        ];
      if (command === "list_channel_messages") return [message];
      if (command === "list_channel_tasks") return [task];
      if (command === "post_channel_message") {
        return { ...message, id: "m2", author_kind: "human", author_name: "You", content: args.content };
      }
      if (command === "start_channel_task") return { task_id: "t1" };
      if (command === "channel_task_action") return { ...task, state: "archived" };
      if (command === "create_channel_task") return task;
      throw new Error(`unexpected command ${command}`);
    });
  });

  afterEach(() => {
    cleanup();
  });

  it("renders the shared timeline and the task board", async () => {
    const { container } = render(TeamChannelsView);
    await waitFor(() =>
      expect(container.querySelector(".team-channels__message")?.textContent).toContain(
        "The migration is ready",
      ),
    );
    expect(container.querySelector(".team-channels__task")?.textContent).toContain(
      "Review the migration",
    );
    expect(container.querySelector(".team-channels__task-owner")?.textContent).toContain("Lead");
  });

  it("starts a task through the runtime command", async () => {
    const { container } = render(TeamChannelsView);
    await waitFor(() => expect(container.querySelector(".team-channels__task-actions button")).not.toBeNull());
    const start = Array.from(container.querySelectorAll(".team-channels__task-actions button")).find(
      (button) => button.textContent?.includes("Start"),
    );
    await fireEvent.click(start!);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("start_channel_task", { taskId: "t1" }),
    );
  });

  it("archives a task from the card header", async () => {
    const { container } = render(TeamChannelsView);
    await waitFor(() => expect(container.querySelector(".team-channels__task-archive")).not.toBeNull());
    await fireEvent.click(container.querySelector(".team-channels__task-archive")!);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("channel_task_action", {
        taskId: "t1",
        action: "archive",
        ownerBotId: null,
        note: null,
      }),
    );
  });

  it("posts a human message into the channel", async () => {
    const { container } = render(TeamChannelsView);
    await waitFor(() =>
      expect(container.querySelector(".team-channels__composer textarea")).not.toBeNull(),
    );
    const input = container.querySelector(".team-channels__composer textarea") as HTMLTextAreaElement;
    await fireEvent.input(input, { target: { value: "Thanks" } });
    const send = container.querySelector(".team-channels__send") as HTMLButtonElement;
    await fireEvent.click(send);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("post_channel_message", {
        channelId: "channel-1",
        content: "Thanks",
        authorName: "You",
      }),
    );
  });
});
