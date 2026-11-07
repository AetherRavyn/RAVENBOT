import { beforeEach, describe, expect, it } from "vitest";
import { agentActivity, type AgentActivityEvent } from "$lib/agentActivity.svelte";

const base: AgentActivityEvent = {
  surface: "browser",
  bot_id: "bot-1",
  action: "click",
  detail: "click #submit",
  ok: true,
  at: "2026-11-05T01:00:00Z",
};

describe("agent activity store", () => {
  beforeEach(() => {
    agentActivity.clear();
    agentActivity.computerPaused = false;
  });

  it("keeps the newest event first and separates the surfaces", () => {
    agentActivity.record(base);
    agentActivity.record({
      ...base,
      surface: "computer",
      action: "type",
      detail: "typed 12 characters",
      at: "2026-11-05T01:00:01Z",
    });

    expect(agentActivity.latest("browser")?.detail).toBe("click #submit");
    expect(agentActivity.latest("computer")?.detail).toBe("typed 12 characters");
    expect(agentActivity.recent("computer")).toHaveLength(1);
    expect(agentActivity.recent("browser")).toHaveLength(1);
  });

  it("caps the retained history", () => {
    for (let index = 0; index < 50; index += 1) {
      agentActivity.record({ ...base, detail: `action ${index}` });
    }
    expect(agentActivity.events).toHaveLength(40);
    expect(agentActivity.events[0].detail).toBe("action 49");
  });
});
