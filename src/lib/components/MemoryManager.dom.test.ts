// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import MemoryManager from "./MemoryManager.svelte";
import { setLocale } from "$lib/i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = vi.mocked(invoke);

let officeRows: any[];
let agentRows: any[];

function officeRow(overrides: Record<string, unknown> = {}) {
  return {
    id: "memory-1",
    content: "Sign every release",
    category: "rule",
    importance: 0.9,
    access_count: 2,
    last_accessed: "2026-11-09T10:00:00Z",
    created_at: "2026-11-08T10:00:00Z",
    source: "user",
    source_thread_id: null,
    source_run_id: null,
    updated_at: "2026-11-09T10:00:00Z",
    ...overrides,
  };
}

beforeEach(() => {
  cleanup();
  setLocale("en");
  officeRows = [officeRow()];
  agentRows = [
    officeRow({
      id: "fact-1",
      content: "Prefer small releases",
      category: null,
      importance: 0.6,
      source: "agent",
    }),
  ];
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (command: string, args: any) => {
    if (command === "list_office_memories") return officeRows;
    if (command === "add_office_memory") {
      const added = officeRow({
        id: "memory-2",
        content: args.content,
        category: args.category,
      });
      officeRows = [...officeRows, added];
      return added;
    }
    if (command === "update_office_memory") {
      officeRows = officeRows.map((row) =>
        row.id === args.memoryId
          ? { ...row, content: args.content, category: args.category }
          : row,
      );
      return officeRows.find((row) => row.id === args.memoryId);
    }
    if (command === "delete_office_memory") {
      officeRows = officeRows.filter((row) => row.id !== args.memoryId);
      return true;
    }
    if (command === "list_bot_memories") return agentRows;
    if (command === "add_bot_memory") {
      const added = officeRow({
        id: "fact-2",
        content: args.content,
        category: null,
      });
      agentRows = [...agentRows, added];
      return added;
    }
    if (command === "update_bot_memory") {
      agentRows = agentRows.map((row) =>
        row.id === args.memoryId ? { ...row, content: args.content } : row,
      );
      return agentRows.find((row) => row.id === args.memoryId);
    }
    if (command === "delete_bot_memory") {
      agentRows = agentRows.filter((row) => row.id !== args.memoryId);
      return true;
    }
    if (command === "find_memory_duplicates") {
      return [
        {
          keep: {
            id: "memory-1",
            content: "Sign every release",
            importance: 0.9,
            access_count: 2,
            created_at: "2026-11-08T10:00:00Z",
            similarity: 1,
          },
          duplicates: [
            {
              id: "memory-2",
              content: "Sign all releases",
              importance: 0.5,
              access_count: 1,
              created_at: "2026-11-09T10:00:00Z",
              similarity: 0.97,
            },
          ],
        },
      ];
    }
    if (command === "remove_memory_duplicates") {
      officeRows = officeRows.filter(
        (row) => row.id !== "memory-2",
      );
      return 1;
    }
    if (command === "summarize_memories") {
      return {
        content: "Combined memory",
        importance: 0.9,
        source_ids: args.memoryIds,
      };
    }
    if (command === "apply_memory_summary") {
      officeRows = [
        officeRow({
          id: "memory-summary",
          content: args.content,
          importance: args.importance,
          source: "summary",
        }),
      ];
      return { replaced: args.sourceIds.length };
    }
    throw new Error(`unexpected command ${command}`);
  });
});
afterEach(cleanup);

describe("MemoryManager", () => {
  it("loads, adds, edits, and deletes office memories", async () => {
    const {
      getAllByLabelText,
      getByLabelText,
      getByText,
      queryByLabelText,
      queryByText,
    } = render(MemoryManager, {
      props: {
        scope: "office",
        targetId: "office-1",
        categories: [
          { id: "general", label: "General" },
          { id: "rule", label: "Rules" },
        ],
      },
    });

    await waitFor(() => expect(getByText("Sign every release")).toBeTruthy());
    expect(getByText("Added manually")).toBeTruthy();
    expect(invokeMock).toHaveBeenCalledWith("list_office_memories", {
      chatroomId: "office-1",
    });

    await fireEvent.input(getByLabelText("New memory content"), {
      target: { value: "Archive every release" },
    });
    await fireEvent.click(getByText("Add to Office Brain"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("add_office_memory", {
        chatroomId: "office-1",
        content: "Archive every release",
        category: "general",
        createdBy: null,
      }),
    );
    await waitFor(() => expect(getByText("Archive every release")).toBeTruthy());

    await fireEvent.click(getAllByLabelText("Edit memory")[0]);
    const editor = getByLabelText("Edit memory content") as HTMLTextAreaElement;
    await fireEvent.input(editor, {
      target: { value: "Sign and archive every release" },
    });
    await fireEvent.click(getByText("Save"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("update_office_memory", {
        chatroomId: "office-1",
        memoryId: "memory-1",
        content: "Sign and archive every release",
        category: "rule",
        importance: 0.9,
      }),
    );
    await waitFor(() =>
      expect(getByText("Sign and archive every release")).toBeTruthy(),
    );
    await waitFor(() =>
      expect(queryByLabelText("Edit memory content")).toBeNull(),
    );

    await fireEvent.click(getAllByLabelText("Delete")[0]);
    await waitFor(() =>
      expect(getByText("Delete this memory? This cannot be undone.")).toBeTruthy(),
    );
    await fireEvent.click(getByText("Delete"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("delete_office_memory", {
        chatroomId: "office-1",
        memoryId: "memory-1",
      }),
    );
    await waitFor(() =>
      expect(queryByText("Sign and archive every release")).toBeNull(),
    );
    expect(getByText("Archive every release")).toBeTruthy();
  });

  it("finds, previews, and removes near-duplicate office memories", async () => {
    officeRows = [
      officeRow(),
      officeRow({
        id: "memory-2",
        content: "Sign all releases",
        importance: 0.5,
      }),
    ];
    const { getByText, queryByText } = render(MemoryManager, {
      props: {
        scope: "office",
        targetId: "office-1",
        categories: [{ id: "rule", label: "Rules" }],
      },
    });
    await waitFor(() => expect(getByText("Sign all releases")).toBeTruthy());

    await fireEvent.click(getByText("Find duplicates"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("find_memory_duplicates", {
        scope: "office",
        targetId: "office-1",
        threshold: 0.92,
      }),
    );
    await waitFor(() =>
      expect(getByText("Groups: 1 · removable: 1")).toBeTruthy(),
    );
    expect(getByText("97% similar")).toBeTruthy();
    expect(getByText("Keep")).toBeTruthy();

    await fireEvent.click(getByText("Remove duplicates (1)"));
    await waitFor(() =>
      expect(
        getByText(
          "Remove these duplicates permanently? This cannot be undone.",
        ),
      ).toBeTruthy(),
    );
    await fireEvent.click(getByText("Confirm removal"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("remove_memory_duplicates", {
        scope: "office",
        targetId: "office-1",
        keepId: "memory-1",
        duplicateIds: ["memory-2"],
        threshold: 0.92,
      }),
    );
    await waitFor(() =>
      expect(getByText("Removed 1 duplicate memories.")).toBeTruthy(),
    );
    await waitFor(() => expect(queryByText("Sign all releases")).toBeNull());
    expect(getByText("Sign every release")).toBeTruthy();
  });

  it("summarizes selected office memories after previewing the result", async () => {
    officeRows = [
      officeRow(),
      officeRow({
        id: "memory-2",
        content: "Sign all releases",
        importance: 0.5,
      }),
    ];
    const { getAllByLabelText, getByLabelText, getByText, queryByText } =
      render(MemoryManager, {
        props: {
          scope: "office",
          targetId: "office-1",
          categories: [{ id: "rule", label: "Rules" }],
        },
      });
    await waitFor(() => expect(getByText("Sign all releases")).toBeTruthy());

    const boxes = getAllByLabelText("Select memory");
    expect(boxes).toHaveLength(2);
    await fireEvent.click(boxes[0]);
    await fireEvent.click(boxes[1]);
    await waitFor(() => expect(getByText("2 selected")).toBeTruthy());

    await fireEvent.click(getByText("Summarize selected"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("summarize_memories", {
        scope: "office",
        targetId: "office-1",
        memoryIds: ["memory-1", "memory-2"],
      }),
    );
    const editor = getByLabelText("Summary content") as HTMLTextAreaElement;
    await waitFor(() => expect(editor.value).toBe("Combined memory"));

    await fireEvent.input(editor, {
      target: { value: "Reviewed combined memory" },
    });
    await fireEvent.click(getByText("Replace 2 memories"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("apply_memory_summary", {
        scope: "office",
        targetId: "office-1",
        sourceIds: ["memory-1", "memory-2"],
        content: "Reviewed combined memory",
        importance: 0.9,
      }),
    );
    await waitFor(() =>
      expect(getByText("Summarized 2 memories.")).toBeTruthy(),
    );
    await waitFor(() =>
      expect(getByText("Reviewed combined memory")).toBeTruthy(),
    );
    expect(getByText("Summarized memory")).toBeTruthy();
    expect(queryByText("Sign all releases")).toBeNull();
  });

  it("uses the agent memory commands in agent scope", async () => {
    const { getByLabelText, getByText, queryByLabelText } = render(
      MemoryManager,
      {
        props: { scope: "agent", targetId: "bot-1" },
      },
    );

    await waitFor(() =>
      expect(getByText("Prefer small releases")).toBeTruthy(),
    );
    expect(getByText("Saved by an agent")).toBeTruthy();
    expect(invokeMock).toHaveBeenCalledWith("list_bot_memories", {
      botId: "bot-1",
    });

    await fireEvent.click(getByLabelText("Edit memory"));
    const editor = getByLabelText("Edit memory content") as HTMLTextAreaElement;
    await fireEvent.input(editor, {
      target: { value: "Prefer small, signed releases" },
    });
    await fireEvent.click(getByText("Save"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("update_bot_memory", {
        botId: "bot-1",
        memoryId: "fact-1",
        content: "Prefer small, signed releases",
        importance: 0.6,
      }),
    );

    await waitFor(() =>
      expect(queryByLabelText("Edit memory content")).toBeNull(),
    );
    await fireEvent.click(getByLabelText("Delete"));
    await fireEvent.click(getByText("Delete"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("delete_bot_memory", {
        botId: "bot-1",
        memoryId: "fact-1",
      }),
    );
  });
});
