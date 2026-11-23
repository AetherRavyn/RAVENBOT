// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import ComposerMentionMenu from "./ComposerMentionMenu.svelte";
import { setLocale } from "$lib/i18n";

beforeEach(() => {
  cleanup();
  setLocale("en");
});
afterEach(cleanup);

describe("ComposerMentionMenu", () => {
  it("renders the active row and reports the picked item", async () => {
    const onPick = vi.fn();
    const { getByText, container } = render(ComposerMentionMenu, {
      props: {
        sigil: "#",
        activeIndex: 1,
        items: [
          { id: "src/a.ts", label: "src/a.ts", detail: "file", kind: "file" },
          { id: "src/b.ts", label: "src/b.ts", detail: "file", kind: "file" },
        ],
        onPick,
      },
    });

    expect(container.querySelectorAll(".composer-mention")).toHaveLength(2);
    expect(container.querySelectorAll(".composer-mention")[1].getAttribute("data-active")).not.toBeNull();
    await fireEvent.click(getByText("src/b.ts"));
    expect(onPick).toHaveBeenCalledWith(
      expect.objectContaining({ id: "src/b.ts", kind: "file" }),
    );
  });
});
