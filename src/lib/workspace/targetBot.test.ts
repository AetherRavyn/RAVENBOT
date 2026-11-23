import { describe, expect, it } from "vitest";
import { resolveTargetBotId } from "./targetBot";

describe("resolveTargetBotId", () => {
  const bots = [{ id: "a" }, { id: "b" }];

  it("uses a selected agent that still exists", () => {
    expect(resolveTargetBotId(bots, "b")).toBe("b");
  });

  it("falls back to the first agent when the selection is stale", () => {
    expect(resolveTargetBotId(bots, "deleted")).toBe("a");
  });

  it("returns null only when there are no agents", () => {
    expect(resolveTargetBotId([], "b")).toBeNull();
    expect(resolveTargetBotId([{ id: null }], "b")).toBeNull();
  });
});
