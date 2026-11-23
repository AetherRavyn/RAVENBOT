// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import OfficeSettings from "$lib/components/OfficeSettings.svelte";
import { setLocale } from "$lib/i18n";
import { a11yViolations } from "$lib/test/a11y";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);

const room = {
  id: "office-1",
  name: "Billing Office",
  description: "Invoices and reconciliations",
  office_template: "custom",
  avatar_url: null,
  avatar_style: "micah",
  goal: "Close the month accurately",
  policy: "Ask before sending email",
  terms: "Local only",
  budget: 2,
  project_folders: [],
};

const bots = [
  { id: "bot-1", name: "Billing Agent" },
  { id: "bot-2", name: "Research Agent" },
];

const goalDetail = {
  goal: {
    id: "goal-1",
    office_id: "office-1",
    thread_id: null,
    objective: "Close the month accurately",
    contract: {
      objective: "Close the month accurately",
      outcome: "The dashboard builds and every chart renders.",
      constraints: [],
      boundaries: [],
      stop_when: "The review says done and the gates pass.",
      max_turns: 3,
      max_tokens: 0,
      max_wall_seconds: 0,
      gates: [],
    },
    status: "active",
    turns_used: 2,
    tokens_used: 0,
    last_verdict: "continue",
    last_reason: "one milestone left",
    created_at: "2026-11-21T10:00:00Z",
    updated_at: "2026-11-21T10:01:00Z",
    completed_at: null,
  },
  gates: [{ command: "npm test" }, { command: "npm run build" }],
  gate_runs: [],
};

let lastGoalUpdate: any = null;

describe("OfficeSettings accessibility", () => {
  beforeEach(() => {
    cleanup();
    setLocale("en");
    invokeMock.mockReset();
    lastGoalUpdate = null;
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command === "list_chatroom_members") {
        return [{ bot_id: "bot-1", rank: "Lead", specialty: "Billing", bot: bots[0] }];
      }
      if (command === "list_mcp_servers") return [];
      if (command === "list_bot_mcp_servers") return [];
      if (command === "list_office_memories" || command === "list_bot_memories") return [];
      if (command === "get_office_alert_settings") {
        return { enabled: false, email: "", webhook_url: "" };
      }
      if (command === "get_office_goal") return structuredClone(goalDetail);
      if (command === "update_chatroom") return room;
      if (command === "update_office_goal") {
        lastGoalUpdate = args;
        return structuredClone(goalDetail);
      }
      if (command === "pause_office_goal" || command === "resume_office_goal") {
        const detail = structuredClone(goalDetail);
        detail.goal.status =
          command === "pause_office_goal" ? "paused" : "active";
        return detail;
      }
      return null;
    });
  });

  afterEach(cleanup);

  it("renders the office settings dialog without axe violations", async () => {
    const { rerender } = render(OfficeSettings, {
      props: {
        room,
        bots,
        open: true,
        onClose: vi.fn(),
        onUpdated: vi.fn(),
      },
    });

    await waitFor(() =>
      expect(document.body.textContent).toContain("Office settings"),
    );
    const violations = await a11yViolations(document.body);
    expect(violations, JSON.stringify(violations)).toEqual([]);

    // Close before cleanup so the tabs unmount under a live root context.
    await rerender({
      room,
      bots,
      open: false,
      onClose: vi.fn(),
      onUpdated: vi.fn(),
    });
    await waitFor(() =>
      expect(document.body.textContent).not.toContain("Office settings"),
    );
  });

  it("edits the completion contract with its verification gates", async () => {
    const { getByText } = render(OfficeSettings, {
      props: {
        room,
        bots,
        open: true,
        onClose: vi.fn(),
        onUpdated: vi.fn(),
      },
    });
    await waitFor(() =>
      expect(document.body.textContent).toContain("Office settings"),
    );

    const goalTab = getByText("Goal").closest("button")!;
    await fireEvent.pointerDown(goalTab, { button: 0, pointerType: "mouse" });
    await fireEvent.pointerUp(goalTab, { button: 0, pointerType: "mouse" });
    await waitFor(() =>
      expect(document.body.textContent).toContain("Completion contract"),
    );
    const outcome = document.querySelector("#goal-contract-outcome") as HTMLTextAreaElement;
    const gates = document.querySelector("#goal-contract-gates") as HTMLTextAreaElement;
    expect(outcome.value).toBe("The dashboard builds and every chart renders.");
    expect(gates.value).toBe("npm test\nnpm run build");
    expect(document.body.textContent).toContain("Last verdict: continue");
    expect(document.body.textContent).toContain("2 reviewed");

    await fireEvent.input(gates, {
      target: { value: "npm test\nnpm run build\nnpm run check" },
    });
    await fireEvent.click(getByText("Save changes").closest("button")!);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "update_office_goal",
        expect.objectContaining({
          officeId: "office-1",
          maxTurns: 3,
          gates: [
            { command: "npm test" },
            { command: "npm run build" },
            { command: "npm run check" },
          ],
        }),
      ),
    );
    expect(lastGoalUpdate?.outcome).toBe("The dashboard builds and every chart renders.");

    await fireEvent.click(getByText("Pause goal").closest("button")!);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("pause_office_goal", {
        officeId: "office-1",
      }),
    );
    await waitFor(() => expect(document.body.textContent).toContain("paused"));

    await fireEvent.click(getByText("Resume goal").closest("button")!);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("resume_office_goal", {
        officeId: "office-1",
      }),
    );
  });
});
