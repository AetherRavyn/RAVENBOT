// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import NotificationSettings from "./NotificationSettings.svelte";
import {
  notificationPreference,
  sendTestNotification,
  setNotificationPreference,
} from "$lib/notifications";
import { setLocale } from "$lib/i18n";
import { a11yViolations } from "$lib/test/a11y";

vi.mock("$lib/notifications", () => ({
  notificationPreference: vi.fn(() => true),
  setNotificationPreference: vi.fn((enabled: boolean) => enabled),
  sendTestNotification: vi.fn(async () => "Test notification sent."),
}));

const preferenceMock = vi.mocked(notificationPreference);
const setMock = vi.mocked(setNotificationPreference);
const testMock = vi.mocked(sendTestNotification);

beforeEach(() => {
  cleanup();
  setLocale("en");
  preferenceMock.mockReturnValue(true);
  setMock.mockImplementation((enabled: boolean) => enabled);
  testMock.mockResolvedValue("Test notification sent.");
});
afterEach(cleanup);

describe("NotificationSettings", () => {
  it("toggles the preference and sends a test notification", async () => {
    const { getByText, getByRole, container } = render(NotificationSettings);
    const toggle = getByRole("checkbox") as HTMLInputElement;
    expect(toggle.checked).toBe(true);
    expect(await a11yViolations(container)).toEqual([]);

    await fireEvent.click(getByText("Send test notification"));
    await waitFor(() =>
      expect(getByText("Test notification sent.")).toBeTruthy(),
    );
    expect(testMock).toHaveBeenCalled();

    await fireEvent.click(toggle);
    expect(setMock).toHaveBeenCalledWith(false);
  });
});
