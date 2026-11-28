// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import SetupChecklist from "./SetupChecklist.svelte";
import { setLocale } from "$lib/i18n";
import { a11yViolations } from "$lib/test/a11y";
import { setupSteps, type SetupFacts } from "$lib/workspace/setupChecklist";

afterEach(cleanup);

function facts(overrides: Partial<SetupFacts> = {}): SetupFacts {
  return {
    providers: [],
    agent: null,
    ollamaModels: [],
    officeCount: 0,
    ...overrides,
  };
}

describe("SetupChecklist", () => {
  it("shows progress and only the next incomplete action", async () => {
    setLocale("en");
    const steps = setupSteps(facts({ providers: ["openai"] }));
    const onAction = vi.fn();
    const { container } = render(SetupChecklist, {
      props: { steps, next: "agent", onAction },
    });

    expect(container.textContent).toContain("Set up RAVENBOT");
    expect(container.textContent).toContain("1 of 3 complete");
    expect(screen.getByRole("button", { name: /Set up The Raven/ })).toBeTruthy();
    expect(screen.queryByRole("button", { name: /Create office/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /Open provider settings/ })).toBeNull();

    await fireEvent.click(screen.getByRole("button", { name: /Set up The Raven/ }));
    expect(onAction).toHaveBeenCalledWith("agent");
    expect(await a11yViolations(container)).toEqual([]);
  });

  it("offers the provider action first and the office action last", async () => {
    setLocale("en");
    const onAction = vi.fn();
    const first = render(SetupChecklist, {
      props: { steps: setupSteps(facts()), next: "provider", onAction },
    });
    await fireEvent.click(screen.getByRole("button", { name: /Open provider settings/ }));
    expect(onAction).toHaveBeenLastCalledWith("provider");
    first.unmount();

    const almost = facts({ providers: ["openai"], agent: { id: "raven" } });
    render(SetupChecklist, {
      props: { steps: setupSteps(almost), next: "office", onAction },
    });
    await fireEvent.click(screen.getByRole("button", { name: /Create office/ }));
    expect(onAction).toHaveBeenLastCalledWith("office");
  });

  it("renders nothing once all three steps are done", () => {
    setLocale("en");
    const done = facts({
      providers: ["openai"],
      agent: { id: "raven" },
      officeCount: 1,
    });
    const { container } = render(SetupChecklist, {
      props: { steps: setupSteps(done), next: null, onAction: vi.fn() },
    });
    expect(container.textContent).toBe("");
  });
});
