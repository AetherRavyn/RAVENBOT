// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import ShortcutSheet from "./ShortcutSheet.svelte";
import { setLocale } from "$lib/i18n";
import { a11yViolations } from "$lib/test/a11y";

afterEach(cleanup);

describe("ShortcutSheet", () => {
  it("lists the implemented shortcuts in three groups", async () => {
    setLocale("en");
    const onClose = vi.fn();
    const { container } = render(ShortcutSheet, { props: { open: true, onClose } });

    expect(container.textContent).toContain("Keyboard shortcuts");
    expect(container.textContent).toContain("Navigation");
    expect(container.textContent).toContain("Chat");
    expect(container.textContent).toContain("Workspace");
    expect(container.textContent).toContain("Command palette");
    expect(container.textContent).toContain("Send message");
    expect(container.textContent).toContain("Toggle browser");
    expect(container.querySelectorAll("kbd").length).toBeGreaterThan(8);
    expect(await a11yViolations(container)).toEqual([]);
  });

  it("closes from the footer and from Escape", async () => {
    setLocale("en");
    const onClose = vi.fn();
    render(ShortcutSheet, { props: { open: true, onClose } });
    await fireEvent.click(screen.getAllByRole("button", { name: "Close" })[0]);
    expect(onClose).toHaveBeenCalledTimes(1);

    await fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it("renders nothing while closed", () => {
    setLocale("en");
    const { container } = render(ShortcutSheet, {
      props: { open: false, onClose: vi.fn() },
    });
    expect(container.textContent).toBe("");
  });
});
