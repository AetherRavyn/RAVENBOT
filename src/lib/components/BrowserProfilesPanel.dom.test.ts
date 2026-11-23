// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import BrowserProfilesPanel from "./BrowserProfilesPanel.svelte";
import { setLocale } from "$lib/i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = vi.mocked(invoke);

const profile = {
  id: "named-work",
  name: "Work",
  created_at: "2026-11-09T10:00:00Z",
  updated_at: "2026-11-09T10:00:00Z",
  cookie_policy: "inherit",
};

beforeEach(() => {
  cleanup();
  setLocale("en");
  invokeMock.mockReset();
  HTMLElement.prototype.hasPointerCapture = vi.fn(() => false);
  HTMLElement.prototype.releasePointerCapture = vi.fn();
  HTMLElement.prototype.setPointerCapture = vi.fn();
  invokeMock.mockImplementation(async (command: string, args?: any) => {
    if (command === "list_browser_profiles") return [profile];
    if (command === "get_browser_cookie_policy") return false;
    if (command === "set_browser_cookie_policy") {
      return Boolean(args?.enabled);
    }
    if (command === "create_browser_profile") {
      return {
        ...profile,
        id: "named-personal",
        name: "Personal",
      };
    }
    if (command === "rename_browser_profile") return profile;
    if (command === "delete_browser_profile") return null;
    if (command === "set_browser_profile_cookie_policy") {
      return (args as any)?.policy;
    }
    throw new Error(`unexpected command ${command}`);
  });
});
afterEach(cleanup);

describe("BrowserProfilesPanel", () => {
  it("overrides the cookie policy for a named profile", async () => {
    const { getByText } = render(BrowserProfilesPanel);
    await waitFor(() => expect(getByText("Work")).toBeTruthy());

    const trigger = document.querySelector('[data-slot="select-trigger"]') as HTMLElement;
    expect(trigger).toBeTruthy();
    await fireEvent.pointerDown(trigger, { button: 0, pointerType: "mouse" });
    await fireEvent.pointerUp(trigger, { button: 0, pointerType: "mouse" });
    await fireEvent.click(trigger);
    await waitFor(() =>
      expect(document.querySelector('[role="option"]')).toBeTruthy(),
    );
    const option = Array.from(
      document.querySelectorAll('[role="option"]'),
    ).find((item) => item.textContent?.includes("Always reject"));
    expect(option).toBeTruthy();
    await fireEvent.pointerDown(option!, { button: 0, pointerType: "mouse" });
    await fireEvent.pointerUp(option!, { button: 0, pointerType: "mouse" });
    await fireEvent.click(option!);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "set_browser_profile_cookie_policy",
        { id: "named-work", policy: "always" },
      ),
    );
  });

  it("toggles automatic cookie refusal", async () => {
    const { getByRole } = render(BrowserProfilesPanel);
    await waitFor(() =>
      expect(getByRole("switch", { name: "Automatic cookie refusal" })).toBeTruthy(),
    );
    const toggle = getByRole("switch", {
      name: "Automatic cookie refusal",
    });
    expect(toggle.getAttribute("aria-checked")).toBe("false");

    await fireEvent.click(toggle);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_browser_cookie_policy", {
        enabled: true,
      }),
    );
    await waitFor(() =>
      expect(toggle.getAttribute("aria-checked")).toBe("true"),
    );
  });

  it("lists, creates, and deletes named profiles", async () => {
    const { getByText, getByLabelText } = render(BrowserProfilesPanel);
    await waitFor(() => expect(getByText("Work")).toBeTruthy());

    await fireEvent.input(getByLabelText("Profile name"), {
      target: { value: "Personal" },
    });
    await fireEvent.click(getByText("Create profile"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("create_browser_profile", {
        name: "Personal",
      }),
    );
    await waitFor(() =>
      expect(getByText("Browser profile created.")).toBeTruthy(),
    );

    vi.spyOn(window, "confirm").mockReturnValue(true);
    await fireEvent.click(getByLabelText("Delete profile"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("delete_browser_profile", {
        id: "named-work",
      }),
    );
    await waitFor(() =>
      expect(getByText("Browser profile deleted.")).toBeTruthy(),
    );
  });
});
