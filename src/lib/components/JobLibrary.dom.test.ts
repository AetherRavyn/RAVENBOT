// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import JobLibrary from "./JobLibrary.svelte";
import { setLocale } from "$lib/i18n";
import { a11yViolations } from "$lib/test/a11y";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);

const jobs = [
  {
    id: "morning-brief",
    name: "Morning brief",
    summary: "The few things that matter today, in one screen.",
    group: "personal",
    cadence: "recurring",
    schedule: "30 7 * * 1-5",
    oversight: "review",
    role_title: "Chief of staff",
    agent_prompt: "You are a chief of staff.",
    routine_instruction: "Write a morning brief.",
    deliverable: "A one-screen brief.",
  },
  {
    id: "blog-draft",
    name: "Blog post draft",
    summary: "Turn a brief into a sourced, edited first draft.",
    group: "marketing",
    cadence: "one_off",
    schedule: "@manual",
    oversight: "review",
    role_title: "Content writer",
    agent_prompt: "You are a careful content writer.",
    routine_instruction: "Write a complete first draft.",
    deliverable: "A Markdown draft.",
  },
];

beforeEach(() => {
  cleanup();
  setLocale("en");
  if (!window.matchMedia) {
    window.matchMedia = ((query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: () => {},
      removeListener: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => false,
    })) as unknown as typeof window.matchMedia;
  }
  if (!globalThis.ResizeObserver) {
    globalThis.ResizeObserver = class {
      observe() {}
      unobserve() {}
      disconnect() {}
    } as unknown as typeof ResizeObserver;
  }
  invokeMock.mockImplementation(async (command: string, args?: any) => {
    switch (command) {
      case "list_job_library":
        return jobs;
      case "install_job": {
        const job = jobs.find((row) => row.id === args?.jobId)!;
        return {
          job_id: job.id,
          bot: {
            id: "bot-new",
            name: args?.name ?? job.role_title,
            description: job.summary,
            config: { job_id: job.id },
          },
          routine: { id: "routine-new", name: job.name },
          thread_id: "thread-new",
          duplicate: false,
        };
      }
      default:
        return null;
    }
  });
});

afterEach(cleanup);

describe("JobLibrary", () => {
  it("loads the catalog, searches it and filters by group", async () => {
    const { getByLabelText, getByRole, getByText, queryByText } = render(JobLibrary, {
      props: { bots: [] },
    });
    await waitFor(() => expect(getByText("Morning brief")).toBeTruthy());
    expect(getByText("Blog post draft")).toBeTruthy();

    await fireEvent.input(getByLabelText("Search jobs, roles, deliverables…"), {
      target: { value: "markdown" },
    });
    await waitFor(() => expect(queryByText("Morning brief")).toBeNull());
    expect(getByText("Blog post draft")).toBeTruthy();

    await fireEvent.input(getByLabelText("Search jobs, roles, deliverables…"), {
      target: { value: "" },
    });
    await fireEvent.click(getByRole("button", { name: "Marketing" }));
    await waitFor(() => expect(queryByText("Morning brief")).toBeNull());
    expect(getByText("Blog post draft")).toBeTruthy();
  });

  it("installs a recurring job with the chosen oversight", async () => {
    const { getByText, getByLabelText } = render(JobLibrary, {
      props: { bots: [] },
    });
    await waitFor(() => expect(getByText("Morning brief")).toBeTruthy());
    await fireEvent.click(getByText("Morning brief"));

    await waitFor(() => expect(getByText("The agent")).toBeTruthy());
    expect(getByText("Chief of staff")).toBeTruthy();

    expect((getByLabelText("Agent name") as HTMLInputElement).value).toBe("Chief of staff");
    await fireEvent.click(getByText("Approval"));
    await fireEvent.click(getByText("Install job"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("install_job", {
        jobId: "morning-brief",
        name: "Chief of staff",
        schedule: "30 7 * * 1-5",
        oversight: "approval",
      }),
    );
    await waitFor(() => expect(getByText("Installed")).toBeTruthy());
    expect(getByText("Review routine")).toBeTruthy();
    expect(
      getByText("The routine is paused. Review it in Routines and enable it when you are ready."),
    ).toBeTruthy();
  });

  it("installs a one-off job as a manual routine", async () => {
    const { getByText } = render(JobLibrary, { props: { bots: [] } });
    await waitFor(() => expect(getByText("Blog post draft")).toBeTruthy());
    await fireEvent.click(getByText("Blog post draft"));
    await waitFor(() => expect(getByText("Install job")).toBeTruthy());

    await fireEvent.click(getByText("Install job"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("install_job", {
        jobId: "blog-draft",
        name: "Content writer",
        schedule: null,
        oversight: "review",
      }),
    );
  });

  it("shows an installed badge for an agent created from the template", async () => {
    const { getByText, getAllByText } = render(JobLibrary, {
      props: {
        bots: [{ id: "bot-1", config: { job_id: "morning-brief" } }] as any,
      },
    });
    await waitFor(() => expect(getByText("Morning brief")).toBeTruthy());
    expect(getAllByText("Installed").length).toBeGreaterThan(0);
  });

  it("has no serious accessibility violations", async () => {
    const { container, getByText } = render(JobLibrary, { props: { bots: [] } });
    await waitFor(() => expect(getByText("Morning brief")).toBeTruthy());
    expect(await a11yViolations(container)).toEqual([]);
  });

  it("shows a retryable error when the catalog cannot load", async () => {
    invokeMock.mockRejectedValueOnce(new Error("backend offline"));
    const { getByText } = render(JobLibrary, { props: { bots: [] } });
    await waitFor(() => expect(getByText("Could not load the job library.")).toBeTruthy());
    expect(getByText(/backend offline/)).toBeTruthy();
    expect(getByText("Retry")).toBeTruthy();
  });
});
