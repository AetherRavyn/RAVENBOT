import { describe, expect, it } from "vitest";
import {
  hasModelPath,
  nextSetupStep,
  setupComplete,
  setupSteps,
  type SetupFacts,
} from "./setupChecklist";

function facts(overrides: Partial<SetupFacts> = {}): SetupFacts {
  return {
    providers: [],
    agent: null,
    ollamaModels: [],
    officeCount: 0,
    ...overrides,
  };
}

describe("first-run setup checklist", () => {
  it("starts with all three steps open", () => {
    const steps = setupSteps(facts());
    expect(steps.map((step) => step.id)).toEqual(["provider", "agent", "office"]);
    expect(steps.every((step) => !step.done)).toBe(true);
    expect(nextSetupStep(steps)).toBe("provider");
    expect(setupComplete(steps)).toBe(false);
  });

  it("accepts a cloud key or a local model as the provider path", () => {
    expect(hasModelPath(facts({ providers: ["openai"] }))).toBe(true);
    expect(hasModelPath(facts({ ollamaModels: [{ id: "llama3.1" }] }))).toBe(true);
  });

  it("requires a local model when the agent is configured for Ollama", () => {
    const localAgent = { id: "raven", config: { model_provider: "ollama" } };
    expect(hasModelPath(facts({ agent: localAgent }))).toBe(false);
    expect(
      hasModelPath(facts({ agent: localAgent, ollamaModels: [{ id: "llama3.1" }] })),
    ).toBe(true);
    // A cloud key does not rescue an agent pointed at a stopped local runtime.
    expect(hasModelPath(facts({ agent: localAgent, providers: ["openai"] }))).toBe(false);
  });

  it("walks provider → agent → office in order", () => {
    const provider = facts({ providers: ["openai"] });
    expect(nextSetupStep(setupSteps(provider))).toBe("agent");

    const agent = facts({ providers: ["openai"], agent: { id: "raven" } });
    expect(nextSetupStep(setupSteps(agent))).toBe("office");

    const office = facts({
      providers: ["openai"],
      agent: { id: "raven" },
      officeCount: 1,
    });
    expect(nextSetupStep(setupSteps(office))).toBeNull();
    expect(setupComplete(setupSteps(office))).toBe(true);
  });

  it("treats malformed provider and model lists as empty", () => {
    expect(hasModelPath(facts({ providers: null, ollamaModels: null }))).toBe(false);
  });
});
