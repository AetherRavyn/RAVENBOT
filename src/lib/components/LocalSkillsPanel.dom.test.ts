// @vitest-environment jsdom
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, cleanup, fireEvent, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import LocalSkillsPanel from "$lib/components/LocalSkillsPanel.svelte";
import { setLocale } from "$lib/i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);

const summary = {
  id: "weekly-review",
  name: "Weekly Review",
  description: "Turn notes into decisions.",
  example_prompt: "Run my weekly review.",
  latest_version: "1.0.1",
  versions: [
    { version: "1.0.0", hash: "a", created_at: "2026-01-01T00:00:00Z", note: "First" },
    { version: "1.0.1", hash: "b", created_at: "2026-01-02T00:00:00Z", note: "Second" },
  ],
  installed_version: "1.0.0",
  installed_modified: false,
  has_demonstration: true,
};

const view = {
  id: "weekly-review",
  name: "Weekly Review",
  description: "Turn notes into decisions.",
  example_prompt: "Run my weekly review.",
  version: "1.0.0",
  hash: "a",
  source: "installed",
  installed_version: "1.0.0",
  installed_modified: false,
  content: "---\nname: Weekly Review\n---\n# Weekly Review\n\n1. Read the notes.",
  files: ["scripts/check.sh", "demo.json"],
  has_demonstration: true,
};

const replayReport = {
  ok: true,
  completed: 3,
  total: 3,
  repaired: 1,
  failed_index: null,
  page_url: "https://example.com/app",
  attempted: [],
  error: null,
  repair_hint: null,
  repairs: [
    {
      index: 1,
      selector: "#submit",
      used: ".primary",
      matched_by: "fallback",
    },
  ],
  warnings: [],
  needs_input: false,
};

describe("LocalSkillsPanel", () => {
  beforeEach(() => {
    cleanup();
    setLocale("en");
    invokeMock.mockReset();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "local_skills_list") return { skills: [summary] };
      if (command === "local_skills_read") return view;
      if (command === "local_skills_install") return { ...view, version: "1.0.1" };
      if (command === "local_skills_marketplace_search")
        return [
          {
            id: "anthropics/docx",
            name: "docx",
            description: "Create Word documents",
            url: "https://github.com/anthropics/skills",
            category: "Documents",
          },
        ];
      if (command === "local_skills_marketplace_install") return { skill: view };
      if (command === "replay_demonstration") return structuredClone(replayReport);
      throw new Error(`unexpected command ${command}`);
    });
  });

  afterEach(() => {
    cleanup();
  });

  it("lists skills and previews the installed document", async () => {
    const { container } = render(LocalSkillsPanel);
    await waitFor(() =>
      expect(container.querySelector(".local-skills__item-name")?.textContent).toContain(
        "Weekly Review",
      ),
    );
    await waitFor(() =>
      expect(container.querySelector(".local-skills__source")?.textContent).toContain("1.0.0"),
    );
    expect(container.querySelector(".local-skills__document")?.textContent).toContain(
      "Read the notes",
    );
    expect(container.querySelector(".local-skills__document")?.textContent).not.toContain(
      "name: Weekly Review",
    );
  });

  it("installs an exact version from the version list", async () => {
    const { container } = render(LocalSkillsPanel);
    await waitFor(() => expect(container.querySelectorAll(".local-skills__version").length).toBe(2));
    const buttons = Array.from(container.querySelectorAll(".local-skills__version button"));
    const install = buttons.find((button) => button.textContent?.includes("Install"));
    await fireEvent.click(install!);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("local_skills_install", {
        id: "weekly-review",
        version: "1.0.1",
        force: false,
      }),
    );
  });

  it("fills the composer with the example prompt instead of sending", async () => {
    const seen: string[] = [];
    const listener = (event: Event) => {
      seen.push((event as CustomEvent<{ prompt: string }>).detail.prompt);
    };
    window.addEventListener("local-skill-try", listener);
    const { container } = render(LocalSkillsPanel);
    await waitFor(() => expect(container.querySelector(".local-skills__try button")).not.toBeNull());
    await fireEvent.click(container.querySelector(".local-skills__try button")!);
    window.removeEventListener("local-skill-try", listener);
    expect(seen).toEqual(["Run my weekly review."]);
  });

  it("searches the marketplace and installs a catalog skill", async () => {
    const { container } = render(LocalSkillsPanel);
    await waitFor(() =>
      expect(container.querySelectorAll(".local-skills__tabs button").length).toBe(2),
    );
    await fireEvent.click(container.querySelectorAll(".local-skills__tabs button")[1]);
    await waitFor(() =>
      expect(container.querySelector(".local-skills__market-item")).not.toBeNull(),
    );
    const install = container.querySelector(".local-skills__market-item button");
    await fireEvent.click(install!);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("local_skills_marketplace_install", {
        target: "anthropics/docx",
        update: false,
      }),
    );
  });

  it("offers a backed-up replace when the installed copy was edited", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "local_skills_list") return { skills: [{ ...summary, installed_modified: true }] };
      if (command === "local_skills_read") return { ...view, installed_modified: true };
      if (command === "local_skills_install") {
        throw "The installed copy of 'weekly-review' has local modifications. Refusing to overwrite it; install again with force=true to replace it after a backup.";
      }
      throw new Error(`unexpected command ${command}`);
    });
    const { container } = render(LocalSkillsPanel);
    await waitFor(() => expect(container.querySelector(".local-skills__warning")).not.toBeNull());
    const buttons = Array.from(container.querySelectorAll(".local-skills__version button"));
    const install = buttons.find((button) => button.textContent?.includes("Install"));
    await fireEvent.click(install!);
    await waitFor(() =>
      expect(container.querySelector(".local-skills__install-error")?.textContent).toContain(
        "local modifications",
      ),
    );
    expect(container.querySelector(".local-skills__install-error button")?.textContent).toContain(
      "Replace",
    );
  });

  it("shows Replay only for skills with a demonstration and confirms before acting", async () => {
    const { container, getByRole, getByText } = render(LocalSkillsPanel);
    await waitFor(() => expect(container.querySelector(".local-skills__replay")).not.toBeNull());
    const replay = getByRole("button", { name: "Replay" });
    await fireEvent.click(replay);
    expect(getByText(/Replay will click and fill/)).toBeTruthy();
    expect(invokeMock).not.toHaveBeenCalledWith(
      "replay_demonstration",
      expect.anything(),
    );

    await fireEvent.click(getByRole("button", { name: "Start replay" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("replay_demonstration", {
        skillId: "weekly-review",
      }),
    );
    await waitFor(() => expect(getByText("Replayed 3 of 3 steps.")).toBeTruthy());
    expect(getByText("1 selector(s) repaired by a fallback.")).toBeTruthy();
    expect(container.querySelector(".local-skills__replay-report")?.getAttribute("data-ok")).toBe(
      "true",
    );
  });

  it("renders a failed replay with its page and repair hint", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "local_skills_list") return { skills: [summary] };
      if (command === "local_skills_read") return view;
      if (command === "replay_demonstration") {
        return {
          ...replayReport,
          ok: false,
          completed: 1,
          failed_index: 1,
          error: "No element matched the recorded selector.",
          repair_hint:
            "Open https://example.com/app and find the control labelled \"Search\". RAVENBOT did not change the skill.",
          repaired: 0,
          repairs: [],
        };
      }
      throw new Error(`unexpected command ${command}`);
    });
    const { container, getByRole, getByText } = render(LocalSkillsPanel);
    await waitFor(() => expect(container.querySelector(".local-skills__replay")).not.toBeNull());
    await fireEvent.click(getByRole("button", { name: "Replay" }));
    await fireEvent.click(getByRole("button", { name: "Start replay" }));
    await waitFor(() =>
      expect(getByText("Replay stopped at step 2 of 3.")).toBeTruthy(),
    );
    expect(getByText(/No element matched the recorded selector/)).toBeTruthy();
    expect(getByText(/did not change the skill/)).toBeTruthy();
    expect(container.querySelector(".local-skills__replay-report")?.getAttribute("data-ok")).toBe(
      "false",
    );
  });

  it("does not offer Replay for a skill without demo.json", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "local_skills_list") {
        return { skills: [{ ...summary, has_demonstration: false }] };
      }
      if (command === "local_skills_read") {
        return { ...view, has_demonstration: false, files: ["scripts/check.sh"] };
      }
      throw new Error(`unexpected command ${command}`);
    });
    const { container, queryByRole } = render(LocalSkillsPanel);
    await waitFor(() =>
      expect(container.querySelector(".local-skills__preview-head")).not.toBeNull(),
    );
    expect(queryByRole("button", { name: "Replay" })).toBeNull();
    expect(container.querySelector(".local-skills__replay")).toBeNull();
  });

  it("shows the explicit dock-closed error from replay", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "local_skills_list") return { skills: [summary] };
      if (command === "local_skills_read") return view;
      if (command === "replay_demonstration") {
        throw "Open the browser dock first, then replay the demonstration.";
      }
      throw new Error(`unexpected command ${command}`);
    });
    const { getByRole, container } = render(LocalSkillsPanel);
    await waitFor(() => expect(container.querySelector(".local-skills__replay")).not.toBeNull());
    await fireEvent.click(getByRole("button", { name: "Replay" }));
    await fireEvent.click(getByRole("button", { name: "Start replay" }));
    await waitFor(() =>
      expect(container.querySelector(".local-skills__error")?.textContent).toContain(
        "Open the browser dock first",
      ),
    );
  });
});
