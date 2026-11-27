// @vitest-environment jsdom
/**
 * The plan graph's avatars follow the node, not the fleet.
 *
 * A DAG can show a finished node and a failed node for two different agents at
 * the same time. The face on each node therefore comes from that node's own run
 * state; reading a single fleet mood here would paint the same expression on
 * every node and hide the failure that the graph exists to show.
 */
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import PlanDag from "./PlanDag.svelte";
import type { DagTask } from "$lib/chat/dag";

afterEach(cleanup);

const tasks: DagTask[] = [{ label: "Check the release", botId: "b1", dependsOn: [] }];

const members = [
  {
    bot_id: "b1",
    rank: "Chief",
    bot: {
      name: "Nova",
      avatar_url: "openbot://cat?color=%23ff6a1a",
      avatar_style: "openbot:cat",
    },
  },
];

function renderNode(state: string | undefined) {
  const { container } = render(PlanDag, {
    props: { tasks, members, stateFor: () => state },
  });
  const avatar = container.querySelector(".raven-avatar[data-source='openbot']");
  expect(avatar, "the OpenBot avatar should render for this member").not.toBeNull();
  return avatar!;
}

describe("PlanDag — avatar mood", () => {
  it("shows a running node as working", () => {
    const avatar = renderNode("running");
    expect(avatar.getAttribute("data-mood")).toBe("working");
    expect(avatar.getAttribute("data-openbot-state")).toBe("working");
  });

  it("shows a failed node as blocked", () => {
    const avatar = renderNode("failed");
    expect(avatar.getAttribute("data-mood")).toBe("failed");
    expect(avatar.getAttribute("data-openbot-state")).toBe("blocked");
  });

  it("shows a finished node as done", () => {
    const avatar = renderNode("done");
    expect(avatar.getAttribute("data-mood")).toBe("responded");
    expect(avatar.getAttribute("data-openbot-state")).toBe("done");
  });

  it("leaves pending and skipped nodes idle", () => {
    for (const state of ["pending", "skipped", undefined]) {
      const avatar = renderNode(state);
      expect(avatar.getAttribute("data-mood")).toBe("idle");
      expect(avatar.getAttribute("data-openbot-state")).toBe("idle");
      cleanup();
    }
  });
});
