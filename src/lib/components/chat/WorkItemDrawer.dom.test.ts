// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import WorkItemDrawer from "./WorkItemDrawer.svelte";
import { setLocale } from "$lib/i18n";
import { a11yViolations } from "$lib/test/a11y";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = vi.mocked(invoke);

function makeDetail(): any {
  return {
    item: {
      id: "n1",
      title: "Render charts",
      instruction: "Render the dashboard charts from the collected data.",
      status: "failed",
      owner_bot_id: "b1",
      owner_account: null,
      claimed_at: null,
      thread_id: "t1",
      run_id: "r1",
      attempts: 1,
      max_attempts: 2,
      created_at: "2026-11-21T10:00:00Z",
      started_at: "2026-11-21T10:01:00Z",
      updated_at: "2026-11-21T10:02:00Z",
      completed_at: "2026-11-21T10:02:00Z",
      result: null,
      error: "Provider down",
      workspace: "/tmp/office",
      metadata: { graph_id: "g1", cycle: 1 },
    },
    comments: [
      {
        id: "c1",
        author: "reviewer",
        body: "Try the cached data.",
        created_at: "2026-11-21T10:03:00Z",
      },
    ],
    artifacts: [
      {
        id: "a1",
        path: "dist/chart.svg",
        sha256: "abcdef1234567890",
        mime: "image/svg+xml",
        size: 1234,
        created_at: "2026-11-21T10:02:00Z",
      },
    ],
    events: [
      {
        id: "e1",
        kind: "claimed",
        payload: { owner: "graph-executor" },
        created_at: "2026-11-21T10:01:00Z",
      },
      {
        id: "e2",
        kind: "failed",
        payload: { error: "Provider down" },
        created_at: "2026-11-21T10:02:00Z",
      },
    ],
  };
}

let current: any = makeDetail();
let claimConflict: string | null = null;

beforeEach(() => {
  cleanup();
  setLocale("en");
  invokeMock.mockReset();
  current = makeDetail();
  claimConflict = null;
  invokeMock.mockImplementation(async (command: string, args?: any) => {
    if (command === "get_current_account") {
      return {
        username: "alice",
        workspace: "alice",
        role: "owner",
        workspace_owner: true,
      };
    }
    if (command === "get_work_item") {
      return structuredClone(current);
    }
    if (command === "claim_work_item") {
      if (claimConflict) {
        throw `This task is already claimed by @${claimConflict}.`;
      }
      current.item.owner_account = "alice";
      current.item.claimed_at ??= "2026-11-21T10:05:00Z";
      current.events.push({
        id: `e${current.events.length + 1}`,
        kind: "claimed",
        payload: { account: "alice", previous_owner: null },
        created_at: current.item.claimed_at,
      });
      return structuredClone(current.item);
    }
    if (command === "release_work_item") {
      if (current.item.owner_account !== "alice") {
        throw "This task is not claimed.";
      }
      current.item.owner_account = null;
      current.item.claimed_at = null;
      current.events.push({
        id: `e${current.events.length + 1}`,
        kind: "released",
        payload: { account: "alice", previous_owner: "alice" },
        created_at: "2026-11-21T10:06:00Z",
      });
      return structuredClone(current.item);
    }
    if (command === "add_work_comment") {
      const comment = {
        id: `c${current.comments.length + 1}`,
        author: String(args?.author ?? "user"),
        body: String(args?.body ?? ""),
        created_at: "2026-11-21T10:04:00Z",
      };
      current.comments.push(comment);
      current.events.push({
        id: `e${current.events.length + 1}`,
        kind: "comment",
        payload: { author: comment.author, body: comment.body },
        created_at: comment.created_at,
      });
      return comment;
    }
    if (command === "retry_work_item") {
      current.item.status = "pending";
      current.item.error = null;
      return null;
    }
    if (command === "request_work_review") {
      current.item.status = "review";
      return null;
    }
    if (command === "approve_work_review") {
      current.item.status = "done";
      return null;
    }
    if (command === "block_work_item") {
      current.item.status = "blocked";
      current.item.error = String(args?.reason ?? "");
      return null;
    }
    if (command === "unblock_work_item" || command === "request_work_changes") {
      current.item.status = "pending";
      current.item.error = null;
      return null;
    }
    return null;
  });
});

afterEach(cleanup);

function props(onClose = vi.fn()) {
  return {
    itemId: "n1",
    label: "Render charts",
    botName: "Ada",
    onClose,
  };
}

describe("WorkItemDrawer", () => {
  it("shows the durable record and retries a failed task", async () => {
    const { container, getByText } = render(WorkItemDrawer, { props: props() });

    await waitFor(() => expect(container.textContent).toContain("Provider down"));
    expect(container.textContent).toContain("Render charts");
    expect(container.textContent).toContain("Try the cached data.");
    expect(container.textContent).toContain("dist/chart.svg");
    expect(container.textContent).toContain("1 of 2 attempts");
    expect(container.textContent).toContain("claimed");

    await fireEvent.click(getByText("Retry task").closest("button")!);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("retry_work_item", { id: "n1" }),
    );
    await waitFor(() => expect(container.textContent).toContain("Pending"));
  });

  it("claims an unclaimed task for the signed-in account and refreshes the record", async () => {
    const { container, getByText } = render(WorkItemDrawer, { props: props() });
    await waitFor(() => expect(container.textContent).toContain("Unclaimed"));
    expect(container.textContent).not.toContain("Release claim");

    const loadsBefore = invokeMock.mock.calls.filter(
      ([command]) => command === "get_work_item",
    ).length;
    await fireEvent.click(getByText("Claim task").closest("button")!);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("claim_work_item", { id: "n1" }),
    );
    await waitFor(() => expect(container.textContent).toContain("@alice"));
    await waitFor(() => expect(container.textContent).toContain("Release claim"));
    const loadsAfter = invokeMock.mock.calls.filter(
      ([command]) => command === "get_work_item",
    ).length;
    expect(loadsAfter).toBeGreaterThan(loadsBefore);
  });

  it("offers release only to the account that owns the claim", async () => {
    current.item.owner_account = "alice";
    current.item.claimed_at = "2026-11-21T10:05:00Z";
    const { container, getByText } = render(WorkItemDrawer, { props: props() });
    await waitFor(() => expect(container.textContent).toContain("@alice"));
    await waitFor(() => expect(container.textContent).toContain("Release claim"));
    expect(container.textContent).not.toContain("Claim task");

    await fireEvent.click(getByText("Release claim").closest("button")!);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("release_work_item", { id: "n1" }),
    );
    await waitFor(() => expect(container.textContent).toContain("Unclaimed"));
  });

  it("shows another person's claim as read-only, with no claim or release button", async () => {
    current.item.owner_account = "bob";
    current.item.claimed_at = "2026-11-21T10:05:00Z";
    const { container, queryByText } = render(WorkItemDrawer, { props: props() });
    await waitFor(() => expect(container.textContent).toContain("@bob"));
    expect(container.textContent).toContain("Claimed by @bob");
    expect(queryByText("Claim task")).toBeNull();
    expect(queryByText("Release claim")).toBeNull();
  });

  it("renders a claim conflict as an inline alert and keeps the record", async () => {
    claimConflict = "bob";
    const { container, getByText } = render(WorkItemDrawer, { props: props() });
    await waitFor(() => expect(container.textContent).toContain("Unclaimed"));

    await fireEvent.click(getByText("Claim task").closest("button")!);
    await waitFor(() =>
      expect(container.querySelector('[role="alert"]')?.textContent).toContain(
        "This task is already claimed by @bob.",
      ),
    );
    expect(container.textContent).toContain("Unclaimed");
    expect(container.textContent).not.toContain("Release claim");
  });

  it("adds a human comment to the record", async () => {    const { container, getByLabelText, getByText } = render(WorkItemDrawer, {
      props: props(),
    });
    await waitFor(() => expect(container.textContent).toContain("Try the cached data."));

    const box = getByLabelText("Add a note for the team…") as HTMLTextAreaElement;
    await fireEvent.input(box, { target: { value: "Use the blue palette." } });
    await fireEvent.click(getByText("Add comment").closest("button")!);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("add_work_comment", {
        id: "n1",
        body: "Use the blue palette.",
        author: "You",
      }),
    );
    await waitFor(() => expect(container.textContent).toContain("Use the blue palette."));
  });

  it("offers review and block actions for the matching states", async () => {
    current.item.status = "done";
    current.item.error = null;
    const { container, getAllByText, getByText } = render(WorkItemDrawer, {
      props: props(),
    });
    await waitFor(() => expect(container.textContent).toContain("Try the cached data."));
    expect(container.textContent).toContain("Done");

    await fireEvent.click(getByText("Request review").closest("button")!);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("request_work_review", { id: "n1" }),
    );
    await waitFor(() => expect(container.textContent).toContain("In review"));

    await fireEvent.click(getAllByText("Request changes")[0].closest("button")!);
    const box = container.querySelector("#work-changes") as HTMLTextAreaElement;
    expect(box).toBeTruthy();
    await fireEvent.input(box, { target: { value: "Match the brand palette." } });
    await fireEvent.click(box.parentElement!.querySelector("button")!);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("request_work_changes", {
        id: "n1",
        comment: "Match the brand palette.",
      }),
    );
  });

  it("closes on Escape and reports a missing ledger row", async () => {
    const onClose = vi.fn();
    const { unmount } = render(WorkItemDrawer, { props: props(onClose) });
    await fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalled();
    unmount();

    invokeMock.mockRejectedValue("The work item was not found.");
    const { findByText } = render(WorkItemDrawer, { props: props() });
    expect(
      await findByText(/No ledger record for this task yet/),
    ).toBeTruthy();
  });

  it("has no axe violations", async () => {
    const { container } = render(WorkItemDrawer, { props: props() });
    await waitFor(() => expect(container.textContent).toContain("Provider down"));
    expect(await a11yViolations(container)).toEqual([]);
  });
});
