import { describe, expect, it } from "vitest";
import { needsDefaultAgentSetup } from "./setup";

describe("needsDefaultAgentSetup", () => {
  it("requires a provider and The Raven", () => {
    expect(needsDefaultAgentSetup([], null)).toBe(true);
    expect(needsDefaultAgentSetup(["openai"], null)).toBe(true);
    expect(needsDefaultAgentSetup([], { id: "raven" })).toBe(true);
    expect(needsDefaultAgentSetup(null, { id: "raven" })).toBe(true);
  });

  it("is complete only when both exist", () => {
    expect(needsDefaultAgentSetup(["openai"], { id: "raven" })).toBe(false);
  });
});
