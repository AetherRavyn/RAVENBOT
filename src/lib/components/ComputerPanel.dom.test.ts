// @vitest-environment jsdom
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, cleanup, fireEvent, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import ComputerPanel from "$lib/components/ComputerPanel.svelte";
import { agentActivity } from "$lib/agentActivity.svelte";
import { setLocale } from "$lib/i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);

const capture = {
  width: 1280,
  height: 720,
  origin_x: 0,
  origin_y: 0,
  data_url: "data:image/png;base64,",
  timestamp: "2026-11-05T01:00:00Z",
};

const windows = [
  {
    id: 7,
    title: "Billing Ledger",
    app_name: "Code",
    width: 1200,
    height: 800,
    minimized: false,
    focused: true,
  },
];

function mockCommands() {
  invokeMock.mockImplementation(async (command: string, args?: any) => {
    switch (command) {
      case "capture_screen":
        return capture;
      case "computer_capabilities":
        return {
          os: "linux",
          session: "x11",
          preview: true,
          host_control: true,
          host_control_backend: "pointer=xdotool typing=xdotool keys=xdotool",
          host_control_policy: "opt_in_required",
          docker: {
            available: true,
            version: "27.0",
            image: "ravenbot/desktop:latest",
            image_present: true,
          },
        };
      case "bot_desktop_status":
        return null;
      case "computer_takeover":
        return false;
      case "set_computer_takeover":
        return Boolean(args?.paused);
      case "list_desktop_windows":
        return windows;
      case "focus_desktop_window":
        return "Focused Billing Ledger";
      case "capture_desktop_window":
        return { ...capture, width: 1200, height: 800 };
      default:
        throw new Error(`unexpected command ${command}`);
    }
  });
}

function button(text: string): HTMLButtonElement | undefined {
  return Array.from(document.querySelectorAll("button")).find(
    (candidate) => candidate.textContent?.trim() === text,
  ) as HTMLButtonElement | undefined;
}

describe("ComputerPanel", () => {
  beforeEach(() => {
    cleanup();
    setLocale("en");
    agentActivity.clear();
    agentActivity.computerPaused = false;
    invokeMock.mockReset();
    mockCommands();
  });

  afterEach(() => {
    cleanup();
  });

  it("shows a live frame and lets the user take over", async () => {
    render(ComputerPanel, {
      props: { botName: "Billing Agent", botId: "bot-1", onClose: () => {} },
    });
    await waitFor(() => expect(document.querySelector("img")).not.toBeNull());

    await fireEvent.click(button("Take over")!);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_computer_takeover", { paused: true }),
    );
    await waitFor(() =>
      expect(document.body.textContent).toContain("You have control"),
    );
  });

  it("lists windows and can focus or capture one", async () => {
    render(ComputerPanel, {
      props: { botName: "Billing Agent", botId: "bot-1", onClose: () => {} },
    });
    await waitFor(() => expect(button("Windows")).toBeTruthy());
    await fireEvent.click(button("Windows")!);
    await waitFor(() => expect(document.body.textContent).toContain("Billing Ledger"));

    await fireEvent.click(button("Focus")!);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("focus_desktop_window", { windowId: 7 }),
    );

    await fireEvent.click(button("Capture")!);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("capture_desktop_window", { windowId: 7 }),
    );
  });

  it("draws a marker over the last pointer action", async () => {
    agentActivity.record({
      surface: "computer",
      bot_id: "bot-1",
      action: "click",
      detail: "click (640, 360)",
      ok: true,
      at: new Date().toISOString(),
      x: 640,
      y: 360,
    });
    render(ComputerPanel, {
      props: { botName: "Billing Agent", botId: "bot-1", onClose: () => {} },
    });
    await waitFor(() => expect(document.querySelector("img")).not.toBeNull());
    expect(document.querySelector(".computer-click-marker")).not.toBeNull();
    expect(document.body.textContent).toContain("click (640, 360)");
  });
});
