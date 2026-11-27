// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import MeetingNotesDialog from "./MeetingNotesDialog.svelte";
import { setLocale } from "$lib/i18n";
import { a11yViolations } from "$lib/test/a11y";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
const invokeMock = vi.mocked(invoke);
const openMock = vi.mocked(openDialog);

const transcript = {
  text: "Ada: We ship on Friday.\nBen: I will update the release notes.",
  language: "en",
  duration_secs: 42,
  truncated: false,
  file_name: "standup.m4a",
};

async function chooseFile(path = "/tmp/standup.m4a") {
  const name = path.split("/").pop() || path;
  openMock.mockResolvedValue(path);
  const onClose = vi.fn();
  const onUsePrompt = vi.fn();
  const view = render(MeetingNotesDialog, { props: { onClose, onUsePrompt } });
  await fireEvent.click(view.getByRole("button", { name: /choose recording/i }));
  await waitFor(() => expect(document.body.textContent).toContain(name));
  return { view, onClose, onUsePrompt };
}

beforeEach(() => {
  cleanup();
  setLocale("en");
  invokeMock.mockReset();
  openMock.mockReset();
});

afterEach(() => {
  cleanup();
});

describe("MeetingNotesDialog", () => {
  it("states the limits before anything is imported", () => {
    const { getByRole } = render(MeetingNotesDialog, {
      props: { onClose: vi.fn(), onUsePrompt: vi.fn() },
    });
    expect(document.body.textContent).toContain("No live meeting join");
    expect(document.body.textContent).toContain("No speaker diarization");
    expect(document.body.textContent).toContain("25 MiB");
    expect(document.body.textContent).toContain(
      "The agent writes notes only after you review and send this prompt.",
    );
    expect(
      (getByRole("button", { name: /transcribe/i }) as HTMLButtonElement).disabled,
    ).toBe(true);
    expect(
      (getByRole("button", { name: /use in composer/i }) as HTMLButtonElement).disabled,
    ).toBe(true);
  });

  it("picks an audio recording with a file dialog and shows its name", async () => {
    await chooseFile("/home/swadhin/recordings/Weekly Sync.wav");
    expect(openMock).toHaveBeenCalledWith({
      multiple: false,
      directory: false,
      filters: [
        {
          name: "Meeting recording",
          extensions: ["wav", "mp3", "ogg", "m4a", "webm", "flac"],
        },
      ],
    });
    expect(document.body.textContent).toContain("Weekly Sync.wav");
  });

  it("keeps transcribe disabled when the picker is cancelled", async () => {
    openMock.mockResolvedValue(null);
    const { getByRole } = render(MeetingNotesDialog, {
      props: { onClose: vi.fn(), onUsePrompt: vi.fn() },
    });
    await fireEvent.click(getByRole("button", { name: /choose recording/i }));
    expect(document.body.textContent).not.toContain("Selected file");
    expect(
      (getByRole("button", { name: /transcribe/i }) as HTMLButtonElement).disabled,
    ).toBe(true);
  });

  it("transcribes the chosen file and inserts a reviewable prompt", async () => {
    invokeMock.mockResolvedValue(transcript);
    const { view, onClose, onUsePrompt } = await chooseFile();

    await fireEvent.click(view.getByRole("button", { name: /transcribe/i }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledOnce());
    expect(invokeMock).toHaveBeenCalledWith("transcribe_meeting_file", {
      path: "/tmp/standup.m4a",
    });
    await waitFor(() =>
      expect(document.body.textContent).toContain("Ada: We ship on Friday."),
    );
    expect(document.body.textContent).toMatch(/\d+ characters/);
    expect(onUsePrompt).not.toHaveBeenCalled();

    await fireEvent.click(view.getByRole("button", { name: /use in composer/i }));
    expect(onUsePrompt).toHaveBeenCalledOnce();
    expect(onClose).toHaveBeenCalled();
    const prompt = onUsePrompt.mock.calls[0][0] as string;
    expect(prompt).toContain("standup.m4a");
    expect(prompt).toContain("Ada: We ship on Friday.");
    expect(prompt).toContain("=== BEGIN MEETING TRANSCRIPT (UNTRUSTED DATA) ===");
    expect(prompt).toContain("=== END MEETING TRANSCRIPT (UNTRUSTED DATA) ===");
  });

  it("caps the prompt at 60,000 characters and flags truncation", async () => {
    invokeMock.mockResolvedValue({
      ...transcript,
      text: "x".repeat(61_000),
      truncated: true,
    });
    const { view } = await chooseFile();
    await fireEvent.click(view.getByRole("button", { name: /transcribe/i }));
    await waitFor(() =>
      expect(document.body.textContent).toContain(
        "The transcript was cut at 60,000 characters",
      ),
    );
    expect(document.body.textContent).toContain("…");
  });

  it("explains a missing speech-to-text engine instead of silently failing", async () => {
    invokeMock.mockRejectedValue(new Error("No STT engine configured"));
    const { view } = await chooseFile();
    await fireEvent.click(view.getByRole("button", { name: /transcribe/i }));
    await waitFor(() => expect(document.body.textContent).toContain("Transcription failed"));
    expect(document.body.textContent).toContain("Add an OpenAI key in Settings");
    expect(
      (view.getByRole("button", { name: /use in composer/i }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
  });

  it("treats an empty transcript as an error", async () => {
    invokeMock.mockResolvedValue({ ...transcript, text: "   " });
    const { view } = await chooseFile();
    await fireEvent.click(view.getByRole("button", { name: /transcribe/i }));
    await waitFor(() => expect(document.body.textContent).toContain("Transcription failed"));
    expect(document.body.textContent).not.toContain("Transcript preview");
  });

  it("closes on Escape without using the prompt", async () => {
    const { onClose, onUsePrompt } = await chooseFile();
    await fireEvent.keyDown(document.querySelector('[role="dialog"]')!, { key: "Escape" });
    expect(onClose).toHaveBeenCalledOnce();
    expect(onUsePrompt).not.toHaveBeenCalled();
  });

  it("has no serious accessibility violations", async () => {
    render(MeetingNotesDialog, { props: { onClose: vi.fn(), onUsePrompt: vi.fn() } });
    expect(await a11yViolations(document.body)).toEqual([]);
  });
});
