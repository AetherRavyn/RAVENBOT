// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import AuditLogPanel from "./AuditLogPanel.svelte";
import { takePendingThreadFocus } from "$lib/chat/messageFocus";
import { setLocale } from "$lib/i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn(), open: vi.fn() }));
const invokeMock = vi.mocked(invoke);
const saveMock = vi.mocked(saveDialog);
const openMock = vi.mocked(openDialog);

const bots = [{ id: "b1", name: "Aster" }];

const modelEntry = {
  id: "a1",
  bot_id: "b1",
  bot_name: "Aster",
  run_id: "r1",
  thread_id: "t1",
  thread_title: "Release review",
  kind: "ModelCall",
  timestamp: "2026-11-09T08:00:00.000+00:00",
  tool_name: null,
  path: null,
  url: null,
  method: null,
  provider: "openai",
  model: "gpt-5",
  tokens_in: 120,
  tokens_out: 30,
  cost: 0.004,
  size: null,
  permission: null,
  reason: null,
};

const memoryEntry = {
  ...modelEntry,
  id: "a3",
  bot_id: "00000000-0000-0000-0000-000000000000",
  bot_name: null,
  run_id: null,
  thread_id: null,
  thread_title: null,
  kind: "MemoryChanged",
  timestamp: "2026-11-09T07:58:00.000+00:00",
  tool_name: null,
  provider: null,
  model: null,
  tokens_in: null,
  tokens_out: null,
  cost: null,
  memory_scope: "office",
  memory_action: "updated",
  memory_id: "memory-1234",
  category: "rule",
  office_id: "office-1",
};

const embeddingsEntry = {
  ...modelEntry,
  id: "a4",
  bot_id: "00000000-0000-0000-0000-000000000000",
  bot_name: null,
  run_id: null,
  thread_id: null,
  thread_title: null,
  kind: "MemoryEmbeddingsUpdated",
  timestamp: "2026-11-09T07:57:00.000+00:00",
  tool_name: null,
  provider: "ollama",
  model: "nomic-embed-text",
  url: "http://127.0.0.1:11434",
  tokens_in: null,
  tokens_out: null,
  cost: null,
  size: 42,
};

const toolEntry = {
  ...modelEntry,
  id: "a2",
  run_id: null,
  thread_id: null,
  thread_title: null,
  kind: "ToolCall",
  timestamp: "2026-11-09T07:59:00.000+00:00",
  tool_name: "read_file",
  provider: null,
  model: null,
  tokens_in: null,
  tokens_out: null,
  cost: null,
  actor_name: "alice",
  actor_role: "operator",
};

const approvalEntry = {
  ...toolEntry,
  id: "a5",
  kind: "ApprovalDecision",
  timestamp: "2026-11-09T07:59:30.000+00:00",
  tool_name: "shell_exec",
  permission: "denied",
  reason: "desktop: too risky",
  actor_name: null,
  actor_role: null,
};

const membershipEntry = {
  ...toolEntry,
  id: "a6",
  bot_id: "00000000-0000-0000-0000-000000000000",
  bot_name: null,
  run_id: null,
  thread_id: null,
  thread_title: null,
  kind: "WorkspaceMembershipChanged",
  timestamp: "2026-11-09T07:58:30.000+00:00",
  tool_name: null,
  permission: "added",
  reason: "alice → bob (operator) in acme",
  actor_name: null,
  actor_role: null,
};

function page(entries: unknown[], total = entries.length) {
  return { entries, total, limit: 50, offset: 0 };
}

beforeEach(() => {
  cleanup();
  setLocale("en");
  takePendingThreadFocus("b1");
  invokeMock.mockReset();
  saveMock.mockReset();
  openMock.mockReset();
  saveMock.mockResolvedValue("/tmp/audit.json");
  openMock.mockResolvedValue("/tmp/verify.json");
  invokeMock.mockImplementation(async (command: string) => {
    if (command === "list_audit_entries")
      return page(
        [
          modelEntry,
          toolEntry,
          approvalEntry,
          membershipEntry,
          memoryEntry,
          embeddingsEntry,
        ],
        120,
      );
    if (command === "export_audit_log") {
      return { path: "/tmp/audit.json", count: 2, sha256: "abc123" };
    }
    if (command === "verify_audit_export") {
      return {
        valid: true,
        trusted: true,
        count: 2,
        exported_at: "2026-11-09T08:00:00.000+00:00",
      };
    }
    if (command === "prune_audit_log") return 4;
    if (command === "get_audit_retention") return 30;
    if (command === "set_audit_retention") return { days: 90, deleted: 3 };
    throw new Error(`unexpected command ${command}`);
  });
});
afterEach(cleanup);

describe("AuditLogPanel", () => {
  it("shows the fleet-wide audit trail without tool arguments", async () => {
    const { getAllByText, getByText, queryByText } = render(AuditLogPanel, {
      props: { bots },
    });
    await waitFor(() => expect(getByText("Release review")).toBeTruthy());
    const text = document.body.textContent || "";
    expect(text).toContain("openai/gpt-5");
    expect(text).toContain("120 in");
    expect(text).toContain("$0.0040");
    expect(getByText("read_file")).toBeTruthy();
    expect(getByText("by alice (operator)")).toBeTruthy();
    expect(getByText("shell_exec")).toBeTruthy();
    expect(getByText("denied")).toBeTruthy();
    expect(getByText("desktop: too risky")).toBeTruthy();
    expect(getByText("alice → bob (operator) in acme")).toBeTruthy();
    expect(getByText("Workspace")).toBeTruthy();
    expect(getByText("Release review")).toBeTruthy();
    expect(getAllByText("Memory change")).toHaveLength(2);
    expect(getByText("Office memory")).toBeTruthy();
    expect(getByText(/Office · updated/)).toBeTruthy();
    expect(getByText("rule")).toBeTruthy();
    expect(getByText("memory-1")).toBeTruthy();
    expect(getAllByText("Embeddings updated")).toHaveLength(2);
    expect(getByText("ollama/nomic-embed-text")).toBeTruthy();
    expect(getByText("http://127.0.0.1:11434")).toBeTruthy();
    expect(getByText("42 memories reindexed")).toBeTruthy();
    expect(queryByText(/api_key|sk-secret/)).toBeNull();

    expect(invokeMock).toHaveBeenCalledWith(
      "list_audit_entries",
      expect.objectContaining({
        botId: null,
        kinds: [],
        search: null,
        limit: 50,
        offset: 0,
      }),
    );
  });

  it("reloads from the first page when the event filter changes", async () => {
    const { getByLabelText } = render(AuditLogPanel, { props: { bots } });
    await waitFor(() => expect(invokeMock).toHaveBeenCalled());
    await fireEvent.change(getByLabelText("All events"), {
      target: { value: "ToolCall" },
    });
    await waitFor(() =>
      expect(invokeMock).toHaveBeenLastCalledWith(
        "list_audit_entries",
        expect.objectContaining({ kinds: ["ToolCall"], offset: 0 }),
      ),
    );
  });

  it("pages forward and opens the owning thread", async () => {
    const onClose = vi.fn();
    const { getByText, getByLabelText } = render(AuditLogPanel, {
      props: { bots, onClose },
    });
    await waitFor(() => expect(getByText("Release review")).toBeTruthy());

    await fireEvent.click(getByLabelText("Next page"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenLastCalledWith(
        "list_audit_entries",
        expect.objectContaining({ offset: 50 }),
      ),
    );

    await fireEvent.click(getByText("Release review"));
    expect(takePendingThreadFocus("b1")).toEqual({
      botId: "b1",
      threadId: "t1",
    });
    expect(onClose).toHaveBeenCalled();
  });

  it("exports the current filters to a signed file", async () => {
    const { getByText } = render(AuditLogPanel, { props: { bots } });
    await waitFor(() => expect(getByText("Release review")).toBeTruthy());

    await fireEvent.click(getByText("Export"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "export_audit_log",
        expect.objectContaining({
          path: "/tmp/audit.json",
          botId: null,
          kinds: [],
          search: null,
        }),
      ),
    );
    await waitFor(() =>
      expect(getByText(/Exported 2 entries with an Ed25519 signature/)).toBeTruthy(),
    );
    expect(getByText("SHA-256 abc123")).toBeTruthy();
  });

  it("verifies a signed export and reports its entry count", async () => {
    const { getByText } = render(AuditLogPanel, { props: { bots } });
    await waitFor(() => expect(getByText("Release review")).toBeTruthy());

    await fireEvent.click(getByText("Verify"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("verify_audit_export", {
        path: "/tmp/verify.json",
      }),
    );
    await waitFor(() =>
      expect(
        getByText(/Signature valid: 2 entries.*this installation/),
      ).toBeTruthy(),
    );
  });

  it("saves an automatic retention policy and reports immediate deletion", async () => {
    const { getByText, getByLabelText } = render(AuditLogPanel, {
      props: { bots },
    });
    await waitFor(() => expect(getByText("Release review")).toBeTruthy());

    await fireEvent.click(getByText("Retention"));
    const select = getByLabelText("Automatic cleanup") as HTMLSelectElement;
    await waitFor(() => expect(select.value).toBe("30"));
    await fireEvent.change(select, { target: { value: "90" } });
    await fireEvent.click(getByText("Save policy"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_audit_retention", {
        days: 90,
      }),
    );
    await waitFor(() =>
      expect(
        getByText(/Automatic cleanup keeps 90 days\..*3 older entries removed/),
      ).toBeTruthy(),
    );
  });

  it("requires an explicit retention action and reloads after pruning", async () => {
    const { getByText, getByLabelText } = render(AuditLogPanel, {
      props: { bots },
    });
    await waitFor(() => expect(getByText("Release review")).toBeTruthy());
    expect(invokeMock).not.toHaveBeenCalledWith(
      "prune_audit_log",
      expect.anything(),
    );

    await fireEvent.click(getByText("Retention"));
    await fireEvent.change(getByLabelText("Delete entries older than"), {
      target: { value: "180" },
    });
    await fireEvent.click(getByText("Delete permanently"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("prune_audit_log", {
        olderThanDays: 180,
      }),
    );
    await waitFor(() => expect(getByText("Deleted 4 audit entries.")).toBeTruthy());
    const listCalls = invokeMock.mock.calls.filter(
      ([command]) => command === "list_audit_entries",
    );
    expect(listCalls.length).toBeGreaterThan(1);
  });
});
