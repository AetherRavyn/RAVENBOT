// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import AwaitingReplies from "./AwaitingReplies.svelte";
import { setLocale } from "$lib/i18n";
import type { Handoff } from "$lib/handoffs.svelte";

const working: Handoff = {
  key: "h1",
  core: "c",
  threadId: "office",
  childThreadId: "child-1",
  fromBotId: "lead",
  toBotId: "sam",
  toBotName: "Sam",
  instruction: "Check the logs",
  startedAt: Date.now(),
  done: false,
  error: null,
  response: null,
  expanded: false,
};

beforeEach(() => {
  cleanup();
  setLocale("en");
});
afterEach(cleanup);

describe("AwaitingReplies", () => {
  it("names the agent and opens its child thread", async () => {
    const onOpen = vi.fn();
    const { getByText } = render(AwaitingReplies, {
      props: { items: [working], nameFor: () => "Sam", onOpen },
    });
    expect(getByText("Sam")).toBeTruthy();
    expect(getByText("Check the logs")).toBeTruthy();
    await fireEvent.click(getByText("Open thread"));
    expect(onOpen).toHaveBeenCalledWith(working);
  });

  it("renders nothing when no handoff is in flight", () => {
    const { container } = render(AwaitingReplies, {
      props: { items: [], nameFor: () => "Sam" },
    });
    expect(container.querySelector(".awaiting-replies")).toBeNull();
  });
});
