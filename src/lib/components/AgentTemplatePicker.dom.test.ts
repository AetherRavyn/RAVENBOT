// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import AgentTemplatePicker from "./AgentTemplatePicker.svelte";
import { AGENT_TEMPLATES } from "$lib/agentTemplates";
import { setLocale } from "$lib/i18n";
import { a11yViolations } from "$lib/test/a11y";

afterEach(cleanup);

describe("AgentTemplatePicker", () => {
  it("renders every template, marks the selected one and emits the choice", async () => {
    setLocale("en");
    const onSelect = vi.fn();
    const { container } = render(AgentTemplatePicker, {
      props: { selectedId: "research", onSelect },
    });

    for (const template of AGENT_TEMPLATES) {
      expect(container.textContent).toContain(
        template.id === "billing"
          ? "Billing & Ledger"
          : template.id === "research"
            ? "Research Scout"
            : template.id === "support"
              ? "Support Triage"
              : "Release Engineer",
      );
    }
    expect(
      screen.getByRole("button", { name: /Research Scout/ }).getAttribute("aria-pressed"),
    ).toBe("true");
    expect(await a11yViolations(container)).toEqual([]);

    await fireEvent.click(screen.getByRole("button", { name: /Release Engineer/ }));
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({ id: "release" }));
  });
});
