// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import AttentionIsland from "$lib/components/AttentionIsland.svelte";
import { avatarProfile } from "$lib/avatar";
import { setLocale } from "$lib/i18n";
import { a11yViolations } from "$lib/test/a11y";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ hide: vi.fn(async () => {}) }),
}));

const invokeMock = vi.mocked(invoke);

describe("AttentionIsland", () => {
  beforeEach(() => {
    cleanup();
    setLocale("en");
    invokeMock.mockReset();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_attention_summary") {
        return {
          approvals: 1,
          questions: 1,
          active_runs: 2,
          items: [
            {
              kind: "approval",
              label: "Run a command",
              detail: "shell_exec",
              thread_id: "thread-1",
              risk: "high",
              bot_id: "bot-1",
              bot_name: "Nova",
              avatar_style: "raven-native",
              state: "pending",
              created_at: "2026-01-01T10:05:00Z",
            },
            {
              kind: "question",
              label: "Which branch?",
              detail: "Choose the release branch",
              thread_id: "thread-2",
              bot_id: "bot-2",
              bot_name: "Atlas",
              avatar_style: "raven-native",
              state: "pending",
              created_at: "2026-01-01T10:00:00Z",
            },
          ],
        };
      }
      if (command === "open_main_window") return null;
      throw new Error(`unexpected command ${command}`);
    });
  });

  afterEach(cleanup);

  it("reports waiting work, renders the newest agent's avatar and passes axe", async () => {
    const { container, getByLabelText } = render(AttentionIsland);
    await waitFor(() =>
      expect(container.textContent).toContain("2 waiting"),
    );
    expect(container.textContent).toContain("Nova");
    expect(container.textContent).toContain("Run a command");
    expect(container.textContent).toContain("2 running");

    // The latest item decides the face, and the face comes from the same
    // resolver the roster uses — not a second avatar implementation.
    const profile = avatarProfile("Nova");
    const avatar = container.querySelector("[data-island-avatar] .raven-avatar");
    expect(avatar?.getAttribute("data-shape")).toBe(profile.shape);
    expect(avatar?.getAttribute("data-colour")).toBe(profile.colour);
    expect(avatar?.getAttribute("data-mood")).toBe("waiting");
    expect(container.querySelector("[data-island-dot='danger']")).not.toBeNull();
    expect(container.querySelector("[data-island-state='pending']")).not.toBeNull();
    expect(await a11yViolations(container)).toEqual([]);

    await fireEvent.click(getByLabelText("Open RAVENBOT"));
    expect(invokeMock).toHaveBeenCalledWith("open_main_window");
  });
});
