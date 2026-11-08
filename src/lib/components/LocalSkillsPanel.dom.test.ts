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
  files: ["scripts/check.sh"],
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
});
