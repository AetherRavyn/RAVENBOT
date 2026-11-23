// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import OfficeAlertSettings from "./OfficeAlertSettings.svelte";
import { setLocale } from "$lib/i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = vi.mocked(invoke);

beforeEach(() => {
  cleanup();
  setLocale("en");
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (command: string) => {
    if (command === "get_office_alert_policy") {
      return { enabled: true, warning_percent: 90, critical_percent: 100 };
    }
    if (command === "set_office_alert_policy") {
      return { enabled: true, warning_percent: 50, critical_percent: 80 };
    }
    throw new Error(`unexpected command ${command}`);
  });
});
afterEach(cleanup);

describe("OfficeAlertSettings", () => {
  it("loads and saves custom warning and critical thresholds", async () => {
    const { getByLabelText, getByText } = render(OfficeAlertSettings, {
      props: { chatroomId: "office-1" },
    });
    await waitFor(() =>
      expect(getByLabelText("Warning at (% of budget)")).toBeTruthy(),
    );
    const warning = getByLabelText("Warning at (% of budget)") as HTMLInputElement;
    await waitFor(() => expect(warning.value).toBe("90"));

    await fireEvent.input(warning, { target: { value: "50" } });
    await fireEvent.input(getByLabelText("Critical at (% of budget)"), {
      target: { value: "80" },
    });
    await fireEvent.click(getByText("Save alert policy"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_office_alert_policy", {
        chatroomId: "office-1",
        enabled: true,
        warningPercent: 50,
        criticalPercent: 80,
      }),
    );
    await waitFor(() =>
      expect(getByText("Budget alert policy saved.")).toBeTruthy(),
    );
  });
});
