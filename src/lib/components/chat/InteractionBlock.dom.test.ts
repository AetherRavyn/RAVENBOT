// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import InteractionBlock from "./InteractionBlock.svelte";
import HoldButton from "./HoldButton.svelte";
import { setLocale } from "$lib/i18n";
import { a11yViolations } from "$lib/test/a11y";

const base = {
  header: "Deploy",
  question: "Deploy to production?",
  answering: false,
};

beforeEach(() => {
  cleanup();
  setLocale("en");
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("InteractionBlock", () => {
  it("falls back to the original question card for an old row", async () => {
    const onAnswer = vi.fn();
    const { getByText } = render(InteractionBlock, {
      props: {
        ...base,
        kind: "question",
        options: ["Postgres", "SQLite"],
        onAnswer,
      },
    });

    expect(getByText("Postgres")).toBeTruthy();
    await fireEvent.click(getByText("Postgres"));
    expect(onAnswer).toHaveBeenCalledWith("Postgres");
  });

  it("renders a confirm block and reports its action and select values", async () => {
    const onAnswer = vi.fn();
    const { getByText, container } = render(InteractionBlock, {
      props: {
        ...base,
        kind: "confirm",
        payload: {
          title: "Deploy to production",
          description: "This changes the live site.",
          fields: [{ id: "env", label: "Environment", value: "production" }],
          actions: [
            { id: "deploy", label: "Deploy", style: "primary" },
            { id: "cancel", label: "Cancel", style: "secondary" },
          ],
        },
        onAnswer,
      },
    });

    expect(container.querySelector(".chat-interaction--confirm")).not.toBeNull();
    expect(getByText("production")).toBeTruthy();
    await fireEvent.click(getByText("Deploy"));
    expect(JSON.parse(onAnswer.mock.calls[0][0])).toEqual({
      action_id: "deploy",
      values: {},
    });
    expect(await a11yViolations(container)).toEqual([]);
  });

  it("requires a hold for a dangerous action", async () => {
    vi.useFakeTimers();
    const onAnswer = vi.fn();
    const { getByText } = render(InteractionBlock, {
      props: {
        ...base,
        kind: "confirm",
        payload: {
          title: "Delete the workspace",
          danger: true,
          actions: [
            { id: "delete", label: "Delete", style: "danger", danger: true, hold_ms: 600 },
          ],
        },
        onAnswer,
      },
    });

    const button = getByText("Delete").closest("button") as HTMLButtonElement;
    await fireEvent.pointerDown(button);
    vi.advanceTimersByTime(300);
    await fireEvent.pointerUp(button);
    vi.advanceTimersByTime(600);
    expect(onAnswer).not.toHaveBeenCalled();

    await fireEvent.pointerDown(button);
    vi.advanceTimersByTime(600);
    expect(onAnswer).toHaveBeenCalledTimes(1);
    expect(JSON.parse(onAnswer.mock.calls[0][0]).action_id).toBe("delete");
  });

  it("collects a multi-select choice only on submit", async () => {
    const onAnswer = vi.fn();
    const { getByText, getByRole } = render(InteractionBlock, {
      props: {
        ...base,
        kind: "choice",
        payload: {
          title: "Which checks?",
          multiple: true,
          options: [
            { id: "tests", label: "Tests" },
            { id: "lint", label: "Lint" },
            { id: "build", label: "Build" },
          ],
        },
        onAnswer,
      },
    });

    await fireEvent.click(getByText("Tests"));
    await fireEvent.click(getByText("Build"));
    expect(onAnswer).not.toHaveBeenCalled();
    await fireEvent.click(getByRole("button", { name: "Submit" }));
    expect(JSON.parse(onAnswer.mock.calls[0][0])).toEqual({
      action_id: "submit",
      values: { selected: ["tests", "build"] },
    });
  });

  it("validates required form fields before answering", async () => {
    const onAnswer = vi.fn();
    const { getByRole } = render(InteractionBlock, {
      props: {
        ...base,
        kind: "form",
        payload: {
          title: "New budget",
          fields: [
            { id: "name", label: "Name", kind: "text", required: true },
            { id: "limit", label: "Limit", kind: "text", value: "50" },
          ],
        },
        onAnswer,
      },
    });

    await fireEvent.click(getByRole("button", { name: "Submit" }));
    expect(onAnswer).not.toHaveBeenCalled();
    expect(getByRole("alert").textContent).toContain("Required: Name");

    await fireEvent.input(getByRole("textbox", { name: /Name/ }), {
      target: { value: "Research" },
    });
    await fireEvent.click(getByRole("button", { name: "Submit" }));
    expect(JSON.parse(onAnswer.mock.calls[0][0])).toEqual({
      action_id: "submit",
      values: { name: "Research", limit: "50" },
    });
  });
});

describe("HoldButton", () => {
  it("confirms only after the full hold and cancels on release", async () => {
    vi.useFakeTimers();
    const onConfirm = vi.fn();
    const { getByRole } = render(HoldButton, {
      props: { label: "Delete", holdMs: 500, onConfirm },
    });
    const button = getByRole("button", { name: "Hold to Delete" });

    await fireEvent.pointerDown(button);
    vi.advanceTimersByTime(200);
    await fireEvent.pointerUp(button);
    vi.advanceTimersByTime(500);
    expect(onConfirm).not.toHaveBeenCalled();

    await fireEvent.pointerDown(button);
    vi.advanceTimersByTime(500);
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });
});
