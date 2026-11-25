// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import SourceChips from "./SourceChips.svelte";
import { setLocale } from "$lib/i18n";

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn(async () => {}) }));

afterEach(cleanup);

describe("SourceChips", () => {
  it("renders a numbered domain chip for every source", () => {
    setLocale("en");
    const { getByText, container } = render(SourceChips, {
      props: {
        sources: [
          { url: "https://example.com/a", title: "Example" },
          { url: "https://docs.example.org/b" },
        ],
      },
    });
    expect(getByText("Sources (2)")).toBeTruthy();
    expect(getByText("example.com")).toBeTruthy();
    expect(getByText("docs.example.org")).toBeTruthy();
    expect(container.querySelectorAll("button")).toHaveLength(2);
  });

  it("renders nothing when there are no sources", () => {
    const { container } = render(SourceChips, { props: { sources: [] } });
    expect(container.querySelector(".rounded-xl")).toBeNull();
  });

  it("opens the source through the platform opener", async () => {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    const { getByText } = render(SourceChips, {
      props: { sources: [{ url: "https://example.com/a" }] },
    });
    await fireEvent.click(getByText("example.com"));
    await waitFor(() =>
      expect(openUrl).toHaveBeenCalledWith("https://example.com/a"),
    );
  });
});
