// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import AuditExportPanel from "./AuditExportPanel.svelte";
import { setLocale } from "$lib/i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = vi.mocked(invoke);

const baseStatus = {
  enabled: true,
  endpoint: "https://otel.example.com/v1/logs",
  service_name: "ravenbot",
  interval_secs: 60,
  batch_size: 200,
  headers_configured: true,
  audit_cursor: 41,
  run_cursor: { updated_at: "2026-11-27T10:00:00Z", id: "run-1" },
  last_attempt_at: "2026-11-27T10:00:01Z",
  last_success_at: "2026-11-27T10:00:01Z",
  last_error: null,
  last_audit_count: 12,
  last_run_count: 3,
};

const successReport = {
  attempted: true,
  audit_count: 3,
  run_count: 2,
  delivered: true,
  endpoint: "https://otel.example.com/v1/logs",
  status_code: 200,
  error: null,
  next_retry_in_secs: null,
};

beforeEach(() => {
  cleanup();
  setLocale("en");
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (command: string) => {
    if (command === "get_otel_export_status") return { ...baseStatus };
    throw new Error(`unexpected command ${command}`);
  });
});
afterEach(cleanup);

describe("AuditExportPanel", () => {
  it("saves the config and never renders saved header values back", async () => {
    const { getByText, getByLabelText, container } = render(AuditExportPanel);
    await waitFor(() => expect(getByText("Headers configured")).toBeTruthy());

    const endpoint = getByLabelText("Collector endpoint") as HTMLInputElement;
    expect(endpoint.value).toBe("https://otel.example.com/v1/logs");
    const headers = getByLabelText("Request headers (JSON)") as HTMLTextAreaElement;
    expect(headers.value).toBe("");

    await fireEvent.input(endpoint, {
      target: { value: "https://collector.example.com" },
    });
    await fireEvent.input(getByLabelText("Service name"), {
      target: { value: "acme-production" },
    });
    await fireEvent.input(getByLabelText("Interval (seconds)"), {
      target: { value: "30" },
    });
    await fireEvent.input(getByLabelText("Batch size"), {
      target: { value: "100" },
    });

    invokeMock.mockImplementation(async (command: string, args: any) => {
      if (command === "set_otel_export_config") {
        expect(args).toEqual({
          enabled: true,
          endpoint: "https://collector.example.com",
          serviceName: "acme-production",
          intervalSecs: 30,
          batchSize: 100,
          headers: null,
        });
        return {
          ...baseStatus,
          endpoint: "https://collector.example.com/v1/logs",
          service_name: "acme-production",
          interval_secs: 30,
          batch_size: 100,
        };
      }
      if (command === "get_otel_export_status") return { ...baseStatus };
      throw new Error(`unexpected command ${command}`);
    });

    await fireEvent.click(getByText("Save"));
    await waitFor(() => expect(getByText("Export settings saved.")).toBeTruthy());
    expect((getByLabelText("Request headers (JSON)") as HTMLTextAreaElement).value).toBe(
      "",
    );
    expect(container.textContent).not.toContain("Bearer");
  });

  it("stores pasted headers write-only and clears them only on save", async () => {
    const { getByText, getByLabelText, container } = render(AuditExportPanel);
    await waitFor(() => expect(getByText("Headers configured")).toBeTruthy());

    const headers = getByLabelText("Request headers (JSON)") as HTMLTextAreaElement;
    await fireEvent.input(headers, {
      target: { value: '{"Authorization":"Bearer top-secret-value"}' },
    });
    expect(headers.value).toBe('{"Authorization":"Bearer top-secret-value"}');

    invokeMock.mockImplementation(async (command: string, args: any) => {
      if (command === "set_otel_export_config") {
        expect(args.headers).toBe('{"Authorization":"Bearer top-secret-value"}');
        return { ...baseStatus, headers_configured: true };
      }
      throw new Error(`unexpected command ${command}`);
    });
    await fireEvent.click(getByText("Save"));
    await waitFor(() => expect(getByText("Export settings saved.")).toBeTruthy());
    expect((getByLabelText("Request headers (JSON)") as HTMLTextAreaElement).value).toBe(
      "",
    );
    expect(container.textContent).not.toContain("top-secret-value");
    expect(container.textContent).not.toContain("Bearer top-secret-value");

    await fireEvent.click(getByText("Clear headers"));
    expect(getByText("Headers will be removed when you save.")).toBeTruthy();

    invokeMock.mockImplementation(async (command: string, args: any) => {
      if (command === "set_otel_export_config") {
        expect(args.headers).toBe("");
        return { ...baseStatus, headers_configured: false };
      }
      throw new Error(`unexpected command ${command}`);
    });
    await fireEvent.click(getByText("Save"));
    await waitFor(() =>
      expect(container.textContent).not.toContain("Headers configured"),
    );
  });

  it("reports a successful test connection and a successful export", async () => {
    const { getByText } = render(AuditExportPanel);
    await waitFor(() => expect(getByText("Headers configured")).toBeTruthy());

    invokeMock.mockImplementation(async (command: string) => {
      if (command === "test_otel_export") return { ...successReport };
      if (command === "export_otel_now") return { ...successReport };
      if (command === "get_otel_export_status") return { ...baseStatus };
      throw new Error(`unexpected command ${command}`);
    });

    await fireEvent.click(getByText("Test connection"));
    await waitFor(() =>
      expect(
        getByText(
          "Test log delivered to https://otel.example.com/v1/logs.",
        ),
      ).toBeTruthy(),
    );

    await fireEvent.click(getByText("Export now"));
    await waitFor(() => expect(getByText("Exported 5 events.")).toBeTruthy());
  });

  it("shows a collector failure as an alert for test and export", async () => {
    const { getByText, getAllByRole } = render(AuditExportPanel);
    await waitFor(() => expect(getByText("Headers configured")).toBeTruthy());

    invokeMock.mockImplementation(async (command: string) => {
      if (command === "test_otel_export") {
        return {
          ...successReport,
          delivered: false,
          status_code: 500,
          error: "Collector returned HTTP 500: nope",
        };
      }
      if (command === "export_otel_now") {
        return {
          ...successReport,
          attempted: true,
          delivered: false,
          status_code: 500,
          error: "Collector returned HTTP 500: nope",
        };
      }
      if (command === "get_otel_export_status") {
        return { ...baseStatus, last_error: "Collector returned HTTP 500: nope" };
      }
      throw new Error(`unexpected command ${command}`);
    });

    await fireEvent.click(getByText("Test connection"));
    await waitFor(() =>
      expect(
        getByText("Test failed: Collector returned HTTP 500: nope"),
      ).toBeTruthy(),
    );

    await fireEvent.click(getByText("Export now"));
    await waitFor(() =>
      expect(
        getByText("Export failed: Collector returned HTTP 500: nope"),
      ).toBeTruthy(),
    );
    expect(getAllByRole("alert").length).toBeGreaterThan(0);
  });

  it("reports an empty batch without calling it a failure", async () => {
    const { getByText } = render(AuditExportPanel);
    await waitFor(() => expect(getByText("Headers configured")).toBeTruthy());
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "export_otel_now") {
        return {
          ...successReport,
          attempted: false,
          audit_count: 0,
          run_count: 0,
          delivered: false,
        };
      }
      if (command === "get_otel_export_status") return { ...baseStatus };
      throw new Error(`unexpected command ${command}`);
    });

    await fireEvent.click(getByText("Export now"));
    await waitFor(() => expect(getByText("Nothing new to export.")).toBeTruthy());
  });
});
