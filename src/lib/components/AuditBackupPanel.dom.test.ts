// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import AuditBackupPanel from "./AuditBackupPanel.svelte";
import { setLocale } from "$lib/i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
const invokeMock = vi.mocked(invoke);
const openMock = vi.mocked(openDialog);

beforeEach(() => {
  cleanup();
  setLocale("en");
  invokeMock.mockReset();
  openMock.mockReset();
  openMock.mockResolvedValue("/tmp/backups");
  invokeMock.mockImplementation(async (command: string) => {
    if (command === "get_audit_export_schedule") {
      return {
        directory: null,
        interval_hours: 0,
        last_at: null,
        last_error: null,
      };
    }
    if (command === "set_audit_export_schedule") {
      return {
        directory: "/tmp/backups",
        interval_hours: 24,
        last_at: "2026-11-09T12:00:00.000Z",
        last_error: null,
        exported: {
          path: "/tmp/backups/ravenbot-audit-20261109T120000Z.json",
          count: 3,
        },
      };
    }
    throw new Error(`unexpected command ${command}`);
  });
});
afterEach(cleanup);

describe("AuditBackupPanel", () => {
  it("chooses a folder, saves a daily schedule, and reports the first backup", async () => {
    const { getByText, getByLabelText } = render(AuditBackupPanel);
    await waitFor(() => expect(getByText(/No backup yet/)).toBeTruthy());

    await fireEvent.click(getByText("Choose folder"));
    await waitFor(() =>
      expect((getByLabelText("Backup folder") as HTMLInputElement).value).toBe(
        "/tmp/backups",
      ),
    );
    await fireEvent.change(getByLabelText("Frequency"), {
      target: { value: "24" },
    });
    await fireEvent.click(getByText("Save schedule"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_audit_export_schedule", {
        directory: "/tmp/backups",
        intervalHours: 24,
      }),
    );
    await waitFor(() =>
      expect(
        getByText(
          "Wrote 3 entries to /tmp/backups/ravenbot-audit-20261109T120000Z.json.",
        ),
      ).toBeTruthy(),
    );
  });

  it("shows the last backup error returned by the backend", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_audit_export_schedule") {
        return {
          directory: "/tmp/backups",
          interval_hours: 24,
          last_at: "2026-11-09T12:00:00.000Z",
          last_error: "The audit export folder is not writable",
        };
      }
      throw new Error(`unexpected command ${command}`);
    });
    const { getByText } = render(AuditBackupPanel);
    await waitFor(() =>
      expect(
        getByText(
          "Backup failed: The audit export folder is not writable",
        ),
      ).toBeTruthy(),
    );
  });
});
