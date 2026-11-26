// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import LearnFromRun from "$lib/components/LearnFromRun.svelte";
import { setLocale } from "$lib/i18n";
import { a11yViolations } from "$lib/test/a11y";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);

const draft = {
  id: "learned-fix-the-build-12345678",
  name: "Fix the build",
  description: "A reviewed workflow learned from a completed Atlas run: fix the build.",
  example_prompt: "fix the build",
  content:
    "---\nname: Fix the build\ndescription: Learned from a run\nexample-prompt: fix the build\n---\n\n## Steps\n\n1. Run cargo test.\n",
  steps: [
    {
      tool: "shell_exec",
      action: null,
      summary: "Run cargo test -p ravenbot",
      arguments: { command: "cargo test -p ravenbot" },
    },
    {
      tool: "browser_dom",
      action: "snapshot → click",
      summary: "Inspect the page snapshot, then Click `#submit`",
      arguments: { actions: [] },
    },
  ],
  tools: ["shell_exec", "browser_dom"],
  warnings: [
    "This skill uses high-risk tools: shell_exec, browser_dom (click). Review every step.",
  ],
  source_run_id: "12345678-aaaa-bbbb-cccc-1234567890ab",
  source_bot_name: "Atlas",
};

const savedView = {
  id: draft.id,
  name: draft.name,
  version: "0.1.0",
  provenance: `learned:run:${draft.source_run_id}`,
};

describe("LearnFromRun", () => {
  beforeEach(() => {
    cleanup();
    setLocale("en");
    invokeMock.mockReset();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "propose_skill_from_run") return structuredClone(draft);
      if (command === "save_learned_skill") return savedView;
      throw new Error(`unexpected command ${command}`);
    });
  });

  afterEach(cleanup);

  it("shows the proposed steps, tools, warnings, and editable document", async () => {
    const onClose = vi.fn();
    const { getByText, getAllByText, getByLabelText, container } = render(
      LearnFromRun,
      { runId: draft.source_run_id, onClose },
    );

    await waitFor(() => expect(getByText("Fix the build")).toBeTruthy());
    expect(getByText("Run cargo test -p ravenbot")).toBeTruthy();
    expect(getAllByText("browser_dom").length).toBeGreaterThanOrEqual(1);
    expect(
      getByText(/This skill uses high-risk tools/),
    ).toBeTruthy();
    expect(container.querySelectorAll(".learned-step")).toHaveLength(2);
    expect(container.querySelectorAll(".learned-tool")).toHaveLength(2);
    expect(container.querySelectorAll(".learned-warning")).toHaveLength(1);

    const document = getByLabelText(/SKILL\.md/) as HTMLTextAreaElement;
    expect(document.value).toContain("Run cargo test");

    expect(await a11yViolations(container)).toEqual([]);
  });

  it("saves the edited document and installs it by default", async () => {
    const onClose = vi.fn();
    const { getByLabelText, getByRole } = render(LearnFromRun, {
      runId: draft.source_run_id,
      onClose,
    });

    await waitFor(() => expect(getByRole("button", { name: "Save & install" })).toBeTruthy());
    const idField = getByLabelText("Skill id") as HTMLInputElement;
    await fireEvent.input(idField, { target: { value: "learned-fix-the-build-retry" } });
    const editor = getByLabelText(/SKILL\.md/) as HTMLTextAreaElement;
    await fireEvent.input(editor, {
      target: { value: `${draft.content}\n3. Edited by the reviewer.\n` },
    });
    await fireEvent.click(getByRole("button", { name: "Save & install" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("save_learned_skill", {
        draft: expect.objectContaining({
          id: "learned-fix-the-build-retry",
          content: expect.stringContaining("Edited by the reviewer."),
        }),
        install: true,
      }),
    );
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
  });

  it("can save without installing", async () => {
    const onClose = vi.fn();
    const { getByRole } = render(LearnFromRun, {
      runId: draft.source_run_id,
      onClose,
    });

    await waitFor(() =>
      expect(getByRole("button", { name: "Save without installing" })).toBeTruthy(),
    );
    await fireEvent.click(getByRole("button", { name: "Save without installing" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("save_learned_skill", {
        draft: expect.objectContaining({ id: draft.id }),
        install: false,
      }),
    );
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
  });

  it("shows an unlearnable-run error inline and lets the user retry", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "propose_skill_from_run") {
        throw new Error(
          "This run has no assistant message with a tool trace to learn from.",
        );
      }
      throw new Error(`unexpected command ${command}`);
    });
    const { getByText, getByRole } = render(LearnFromRun, {
      runId: draft.source_run_id,
      onClose: vi.fn(),
    });

    await waitFor(() =>
      expect(getByText(/no assistant message with a tool trace/)).toBeTruthy(),
    );
    expect(getByRole("button", { name: "Try again" })).toBeTruthy();
    expect(invokeMock).toHaveBeenCalledWith("propose_skill_from_run", {
      runId: draft.source_run_id,
    });
  });

  it("shows a save error without closing the dialog", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "propose_skill_from_run") return structuredClone(draft);
      if (command === "save_learned_skill") {
        throw new Error("A skill with id 'learned-fix-the-build-12345678' already exists.");
      }
      throw new Error(`unexpected command ${command}`);
    });
    const onClose = vi.fn();
    const { getByRole, container } = render(LearnFromRun, {
      runId: draft.source_run_id,
      onClose,
    });

    await waitFor(() => expect(getByRole("button", { name: "Save & install" })).toBeTruthy());
    await fireEvent.click(getByRole("button", { name: "Save & install" }));

    await waitFor(() =>
      expect(container.querySelector(".learned-save-error")?.textContent).toContain(
        "already exists",
      ),
    );
    expect(onClose).not.toHaveBeenCalled();
  });

  it("closes on Escape", async () => {
    const onClose = vi.fn();
    const { getByRole } = render(LearnFromRun, {
      runId: draft.source_run_id,
      onClose,
    });
    await waitFor(() => expect(getByRole("dialog")).toBeTruthy());

    await fireEvent.keyDown(getByRole("dialog"), { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
