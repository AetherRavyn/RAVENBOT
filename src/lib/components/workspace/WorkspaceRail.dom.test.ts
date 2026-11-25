// @vitest-environment jsdom
import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, cleanup, fireEvent } from "@testing-library/svelte";
import WorkspaceRail from "./WorkspaceRail.svelte";
import { setLocale } from "$lib/i18n";
import { a11yViolations } from "$lib/test/a11y";

describe("WorkspaceRail", () => {
  beforeEach(() => {
    cleanup();
    setLocale("en");
  });

  it("has no serious accessibility violations", async () => {
    const { container } = render(WorkspaceRail, {
      props: {
        onOpenSettings: vi.fn(),
        onOpenSkills: vi.fn(),
        onOpenMarketplace: vi.fn(),
      },
    });
    expect(await a11yViolations(container)).toEqual([]);
  });

  it("calls the marketplace handler from the storefront button", async () => {
    const onOpenMarketplace = vi.fn();
    const { getByLabelText } = render(WorkspaceRail, {
      props: {
        onOpenSettings: vi.fn(),
        onOpenSkills: vi.fn(),
        onOpenMarketplace,
      },
    });

    await fireEvent.click(getByLabelText("Marketplace"));
    expect(onOpenMarketplace).toHaveBeenCalledTimes(1);
  });
});
