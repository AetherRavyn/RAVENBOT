// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import QuestionCard from "./QuestionCard.svelte";
import { setLocale } from "$lib/i18n";

describe("QuestionCard", () => {
  beforeEach(() => {
    cleanup();
    setLocale("en");
  });
  afterEach(cleanup);

  it("answers with a suggested chip in one click", async () => {
    const onAnswer = vi.fn();
    const { getByText, container } = render(QuestionCard, {
      props: {
        botName: "Scout",
        header: "Pick a target",
        question: "Which environment should the audit run against?",
        options: ["Staging", "Production"],
        onAnswer,
      },
    });

    expect(getByText("Needs you")).toBeTruthy();
    expect(container.querySelectorAll(".chat-question__chip")).toHaveLength(2);
    await fireEvent.click(getByText("Staging"));
    expect(onAnswer).toHaveBeenCalledWith("Staging");
  });

  it("answers with typed text and refuses an empty submit", async () => {
    const onAnswer = vi.fn();
    const { getByLabelText, getByText, container } = render(QuestionCard, {
      props: {
        question: "What should I name the branch?",
        allowCustom: true,
        onAnswer,
      },
    });

    const input = getByLabelText("Answer") as HTMLInputElement;
    const form = container.querySelector(".chat-question__form") as HTMLFormElement;
    await fireEvent.submit(form);
    expect(onAnswer).not.toHaveBeenCalled();

    await fireEvent.input(input, { target: { value: "audit-q3" } });
    await fireEvent.submit(form);
    expect(onAnswer).toHaveBeenCalledWith("audit-q3");
    expect(getByText("Answer")).toBeTruthy();
  });

  it("hides the text field when the agent asked for a closed choice", () => {
    const { container } = render(QuestionCard, {
      props: {
        question: "Ship it?",
        options: ["Yes", "No"],
        allowCustom: false,
        onAnswer: vi.fn(),
      },
    });
    expect(container.querySelector(".chat-question__form")).toBeNull();
  });
});
