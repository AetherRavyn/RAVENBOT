// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import WorkflowLibrary from "./WorkflowLibrary.svelte";
import { setLocale } from "$lib/i18n";
import { a11yViolations } from "$lib/test/a11y";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = vi.mocked(invoke);

const members = [
  { bot_id: "bot-1", bot: { id: "bot-1", name: "Scout" } },
  { bot_id: "bot-2", bot: { id: "bot-2", name: "Builder" } },
];

const tasks = [
  {
    id: "0",
    botId: "bot-1",
    label: "Audit",
    dependsOn: [],
    condition: null,
    dataFrom: null,
    retries: 0,
    outputSchema: "",
    loopIterations: 1,
    loopUntil: null,
    loopGroup: "",
    loopGroupTerminal: false,
    loopGroupIterations: 2,
    loopGroupStop: null,
    nodeType: "agent",
  },
  {
    id: "1",
    botId: "bot-2",
    label: "Fix",
    dependsOn: [0],
    condition: null,
    dataFrom: null,
    retries: 0,
    outputSchema: "",
    loopIterations: 1,
    loopUntil: null,
    loopGroup: "",
    loopGroupTerminal: false,
    loopGroupIterations: 2,
    loopGroupStop: null,
    nodeType: "agent",
  },
];

let webhookEnabled = false;

const loadCondition = {
  source: 0,
  operator: "contains",
  value: "approved",
};

beforeEach(() => {
  cleanup();
  setLocale("en");
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (command: string) => {
    if (command === "save_workflow") return { id: "wf-2", version: 2 };
    if (command === "list_workflows") {
      return [
        {
          id: "wf-1",
          name: "Release audit",
          goal: "Audit the release",
          tasks: [
            { bot_id: "bot-1", label: "Audit", depends_on: [] },
            {
              bot_id: "bot-2",
              label: "Fix",
              depends_on: [0],
              condition: loadCondition,
              data_from: [0],
              retries: 2,
              output_schema: '{"type":"object"}',
              loop_iterations: 3,
              loop_until: { operator: "contains", value: "APPROVED" },
            },
          ],
          office_id: "office-1",
          version: 1,
          created_at: "2026-11-09T10:00:00Z",
          updated_at: "2026-11-09T11:00:00Z",
        },
      ];
    }
    if (command === "list_workflow_versions") {
      return [
        {
          id: "wf-1-v2",
          workflow_id: "wf-1",
          version: 2,
          name: "Release audit v2",
          goal: "Audit the release again",
          tasks: [{ bot_id: "bot-1", label: "Audit", depends_on: [] }],
          created_at: "2026-11-09T11:00:00Z",
        },
        {
          id: "wf-1-v1",
          workflow_id: "wf-1",
          version: 1,
          name: "Release audit",
          goal: "Audit the release",
          tasks: [{ bot_id: "bot-1", label: "Audit", depends_on: [] }],
          created_at: "2026-11-09T10:00:00Z",
        },
      ];
    }
    if (command === "list_workflow_runs") {
      return [
        {
          id: "run-1",
          workflow_id: "wf-1",
          workflow_version: 1,
          thread_id: "thread-1",
          goal: "Audit the release",
          state: "completed",
          started_at: "2026-11-09T11:30:00Z",
          finished_at: "2026-11-09T11:35:00Z",
          detail: null,
        },
      ];
    }
    if (command === "get_workflow_schedule") {
      return {
        workflow_id: "wf-1",
        interval_hours: 0,
        enabled: false,
        next_run_at: null,
        last_run_at: null,
        last_error: null,
        updated_at: "2026-11-09T10:00:00Z",
      };
    }
    if (command === "set_workflow_schedule") {
      return {
        workflow_id: "wf-1",
        interval_hours: 24,
        enabled: true,
        next_run_at: "2026-11-10T10:00:00Z",
        last_run_at: null,
        last_error: null,
        updated_at: "2026-11-09T10:00:00Z",
      };
    }
    if (command === "get_workflow_webhook_status") {
      return {
        enabled: webhookEnabled,
        base_url: "http://127.0.0.1:8800",
      };
    }
    if (command === "enable_workflow_webhook") {
      webhookEnabled = true;
      return {
        url: "http://127.0.0.1:8800/workflows/secret-token",
        secret: "secret-token",
        header: "Bearer secret-token",
      };
    }
    if (command === "disable_workflow_webhook") {
      webhookEnabled = false;
      return null;
    }
    if (command === "delete_workflow") return null;
    throw new Error(`unexpected command ${command}`);
  });
});
afterEach(cleanup);

describe("WorkflowLibrary", () => {
  it("saves the current plan as a workflow", async () => {
    const { getByText, getByLabelText } = render(WorkflowLibrary, {
      props: {
        officeId: "office-1",
        members,
        goal: "Audit the release",
        tasks,
      },
    });
    await fireEvent.click(getByText("Save as workflow"));
    await fireEvent.input(getByLabelText("Workflow name"), {
      target: { value: "Release audit" },
    });
    await fireEvent.click(getByText("Save workflow"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("save_workflow", {
        id: null,
        name: "Release audit",
        goal: "Audit the release",
        tasks: [
          {
            botId: "bot-1",
            label: "Audit",
            dependsOn: [],
            condition: null,
            dataFrom: null,
            retries: 0,
            outputSchema: null,
            loopIterations: 1,
            loopUntil: null,
            loopGroup: null,
            loopGroupTerminal: false,
            loopGroupIterations: 2,
            loopGroupStop: null,
            nodeType: "agent",
          },
          {
            botId: "bot-2",
            label: "Fix",
            dependsOn: [0],
            condition: null,
            dataFrom: null,
            retries: 0,
            outputSchema: null,
            loopIterations: 1,
            loopUntil: null,
            loopGroup: null,
            loopGroupTerminal: false,
            loopGroupIterations: 2,
            loopGroupStop: null,
            nodeType: "agent",
          },
        ],
        officeId: "office-1",
      }),
    );
    await waitFor(() => expect(getByText("Workflow saved as v2.")).toBeTruthy());
  });

  it("saves a task's JSON Schema with the workflow", async () => {
    const schema = '{"type":"object","required":["summary"]}';
    const { getByText, getByLabelText } = render(WorkflowLibrary, {
      props: {
        officeId: "office-1",
        members,
        goal: "Audit the release",
        tasks: [
          {
            id: "0",
            botId: "bot-1",
            label: "Audit",
            dependsOn: [],
            condition: null,
            dataFrom: null,
            retries: 0,
            outputSchema: schema,
            loopIterations: 3,
            loopUntil: { operator: "contains", value: "APPROVED" },
            loopGroup: "",
            loopGroupTerminal: false,
            loopGroupIterations: 2,
            loopGroupStop: null,
            nodeType: "agent",
          },
        ],
      },
    });
    await fireEvent.click(getByText("Save as workflow"));
    await fireEvent.input(getByLabelText("Workflow name"), {
      target: { value: "Typed audit" },
    });
    await fireEvent.click(getByText("Save workflow"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("save_workflow", {
        id: null,
        name: "Typed audit",
        goal: "Audit the release",
        tasks: [
          {
            botId: "bot-1",
            label: "Audit",
            dependsOn: [],
            condition: null,
            dataFrom: null,
            retries: 0,
            outputSchema: schema,
            loopIterations: 3,
            loopUntil: { operator: "contains", value: "APPROVED" },
            loopGroup: null,
            loopGroupTerminal: false,
            loopGroupIterations: 2,
            loopGroupStop: null,
            nodeType: "agent",
          },
        ],
        officeId: "office-1",
      }),
    );
  });

  it("loads a saved workflow into the plan canvas", async () => {
    const onLoad = vi.fn();
    const { getByText, container } = render(WorkflowLibrary, {
      props: {
        officeId: "office-1",
        members,
        goal: "",
        tasks: [],
        onLoad,
      },
    });
    await fireEvent.click(getByText("Workflows"));
    await waitFor(() => expect(getByText("Release audit")).toBeTruthy());
    expect(await a11yViolations(container)).toEqual([]);
    await fireEvent.click(getByText("Load"));

    expect(onLoad).toHaveBeenCalledWith(
      "Audit the release",
      [
        {
          id: "0",
          botId: "bot-1",
          label: "Audit",
          dependsOn: [],
          condition: null,
          dataFrom: null,
          retries: 0,
          outputSchema: "",
          loopIterations: 1,
          loopUntil: null,
          loopGroup: "",
          loopGroupTerminal: false,
          loopGroupIterations: 2,
          loopGroupStop: null,
          nodeType: "agent",
        },
        {
          id: "1",
          botId: "bot-2",
          label: "Fix",
          dependsOn: [0],
          condition: loadCondition,
          dataFrom: [0],
          retries: 2,
          outputSchema: '{"type":"object"}',
          loopIterations: 3,
          loopUntil: { operator: "contains", value: "APPROVED" },
          loopGroup: "",
          loopGroupTerminal: false,
          loopGroupIterations: 2,
          loopGroupStop: null,
          nodeType: "agent",
        },
      ],
      { id: "wf-1", version: 1, name: "Release audit" },
    );
    expect(getByText("Workflow loaded into the plan.")).toBeTruthy();
  });

  it("runs a saved workflow with a confirmation and workflow meta", async () => {
    const onRun = vi.fn();
    vi.spyOn(window, "confirm").mockReturnValue(true);
    const { getByLabelText, getByText } = render(WorkflowLibrary, {
      props: { officeId: "office-1", members, goal: "", tasks: [], onRun },
    });
    await fireEvent.click(getByText("Workflows"));
    await waitFor(() => expect(getByText("Release audit")).toBeTruthy());
    await fireEvent.click(getByLabelText("Run"));

    expect(onRun).toHaveBeenCalledWith(
      "Audit the release",
      [
        {
          id: "0",
          botId: "bot-1",
          label: "Audit",
          dependsOn: [],
          condition: null,
          dataFrom: null,
          retries: 0,
          outputSchema: "",
          loopIterations: 1,
          loopUntil: null,
          loopGroup: "",
          loopGroupTerminal: false,
          loopGroupIterations: 2,
          loopGroupStop: null,
          nodeType: "agent",
        },
        {
          id: "1",
          botId: "bot-2",
          label: "Fix",
          dependsOn: [0],
          condition: loadCondition,
          dataFrom: [0],
          retries: 2,
          outputSchema: '{"type":"object"}',
          loopIterations: 3,
          loopUntil: { operator: "contains", value: "APPROVED" },
          loopGroup: "",
          loopGroupTerminal: false,
          loopGroupIterations: 2,
          loopGroupStop: null,
          nodeType: "agent",
        },
      ],
      { id: "wf-1", version: 1, name: "Release audit" },
    );
  });

  it("enables and disables a workflow webhook trigger", async () => {
    const { getByLabelText, getByText } = render(WorkflowLibrary, {
      props: { officeId: "office-1", members, goal: "", tasks: [] },
    });
    await fireEvent.click(getByText("Workflows"));
    await waitFor(() => expect(getByText("Release audit")).toBeTruthy());

    await fireEvent.click(getByLabelText("Webhook trigger"));
    await waitFor(() =>
      expect(getByText("Webhook trigger is off.")).toBeTruthy(),
    );

    await fireEvent.click(getByText("Enable trigger"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("enable_workflow_webhook", {
        workflowId: "wf-1",
      }),
    );
    await waitFor(() =>
      expect(getByText("Webhook trigger enabled.")).toBeTruthy(),
    );
    expect(
      (getByLabelText("Webhook trigger URL") as HTMLInputElement).value,
    ).toBe("http://127.0.0.1:8800/workflows/secret-token");

    await fireEvent.click(getByText("Disable trigger"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("disable_workflow_webhook", {
        workflowId: "wf-1",
      }),
    );
    await waitFor(() =>
      expect(getByText("Webhook trigger is off.")).toBeTruthy(),
    );
  });

  it("saves a daily schedule for a workflow", async () => {
    const { getByLabelText, getByText } = render(WorkflowLibrary, {
      props: { officeId: "office-1", members, goal: "", tasks: [] },
    });
    await fireEvent.click(getByText("Workflows"));
    await waitFor(() => expect(getByText("Release audit")).toBeTruthy());
    await fireEvent.click(getByLabelText("Schedule"));
    await waitFor(() => expect(getByText("Save schedule")).toBeTruthy());
    const select = document.querySelector("select") as HTMLSelectElement;
    await fireEvent.change(select, { target: { value: "24" } });
    await fireEvent.click(getByText("Save schedule"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_workflow_schedule", {
        workflowId: "wf-1",
        intervalHours: 24,
      }),
    );
    await waitFor(() =>
      expect(getByText("Workflow schedule saved.")).toBeTruthy(),
    );
  });

  it("loads a historical version as that version", async () => {
    const onLoad = vi.fn();
    const { getAllByText, getByLabelText, getByText } = render(WorkflowLibrary, {
      props: { officeId: "office-1", members, goal: "", tasks: [], onLoad },
    });
    await fireEvent.click(getByText("Workflows"));
    await waitFor(() => expect(getByText("Release audit")).toBeTruthy());
    await fireEvent.click(getByLabelText("Versions"));
    await waitFor(() => expect(getByText("Release audit v2")).toBeTruthy());
    const loads = getAllByText("Load");
    await fireEvent.click(loads[loads.length - 1]);
    expect(onLoad).toHaveBeenCalledWith(
      "Audit the release",
      [
        {
          id: "0",
          botId: "bot-1",
          label: "Audit",
          dependsOn: [],
          condition: null,
          dataFrom: null,
          retries: 0,
          outputSchema: "",
          loopIterations: 1,
          loopUntil: null,
          loopGroup: "",
          loopGroupTerminal: false,
          loopGroupIterations: 2,
          loopGroupStop: null,
          nodeType: "agent",
        },
      ],
      { id: "wf-1", version: 1, name: "Release audit" },
    );
  });
});
