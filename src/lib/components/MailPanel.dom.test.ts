// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import MailPanel from "./MailPanel.svelte";
import { setLocale } from "$lib/i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = vi.mocked(invoke);

interface DraftSummary {
  id: string;
  bot_id: string;
  thread_id: string | null;
  run_id: string | null;
  to_addresses: string[];
  cc_addresses: string[];
  subject: string;
  status: string;
  error: string | null;
  created_at: string;
  updated_at: string;
  sent_at: string | null;
}

interface FullDraft extends DraftSummary {
  body: string;
}

const account = {
  configured: true,
  host: "smtp.example.com",
  port: 587,
  security: "starttls",
  username: "agent@example.com",
  from: "agent@example.com",
  allowlist: "example.com, boss@other.test",
  allowlist_valid: true,
  has_password: true,
};

const baseDrafts: DraftSummary[] = [
  {
    id: "draft-1",
    bot_id: "bot-1",
    thread_id: "thread-1",
    run_id: "run-1",
    to_addresses: ["person@example.com"],
    cc_addresses: ["cc@example.com"],
    subject: "Quarterly numbers",
    status: "draft",
    error: null,
    created_at: "2026-11-27T10:00:00Z",
    updated_at: "2026-11-27T10:00:00Z",
    sent_at: null,
  },
  {
    id: "draft-sending",
    bot_id: "bot-1",
    thread_id: "thread-1",
    run_id: "run-2",
    to_addresses: ["person@example.com"],
    cc_addresses: [],
    subject: "Sending report",
    status: "sending",
    error: null,
    created_at: "2026-11-27T10:01:00Z",
    updated_at: "2026-11-27T10:01:00Z",
    sent_at: null,
  },
  {
    id: "draft-sent",
    bot_id: "bot-1",
    thread_id: "thread-1",
    run_id: "run-3",
    to_addresses: ["person@example.com"],
    cc_addresses: [],
    subject: "Sent report",
    status: "sent",
    error: null,
    created_at: "2026-11-27T10:02:00Z",
    updated_at: "2026-11-27T10:03:00Z",
    sent_at: "2026-11-27T10:03:00Z",
  },
  {
    id: "draft-discarded",
    bot_id: "bot-1",
    thread_id: "thread-1",
    run_id: "run-4",
    to_addresses: ["person@example.com"],
    cc_addresses: [],
    subject: "Discarded report",
    status: "discarded",
    error: null,
    created_at: "2026-11-27T10:04:00Z",
    updated_at: "2026-11-27T10:05:00Z",
    sent_at: null,
  },
];

let drafts: DraftSummary[] = [];
let fullDrafts: Record<string, FullDraft> = {};
let sendFails = false;

function resetData() {
  drafts = baseDrafts.map((draft) => ({ ...draft }));
  fullDrafts = Object.fromEntries(
    drafts.map((draft) => [
      draft.id,
      { ...draft, body: `Body for ${draft.subject}` },
    ]),
  );
  sendFails = false;
}

function mapAddresses(raw: string): string[] {
  return raw
    .split(",")
    .map((value) => value.trim())
    .filter(Boolean);
}

beforeEach(() => {
  cleanup();
  setLocale("en");
  invokeMock.mockReset();
  resetData();
  invokeMock.mockImplementation(async (command: string, args?: any) => {
    switch (command) {
      case "get_agent_email":
        return { ...account };
      case "list_email_drafts":
        return drafts.map((draft) => ({ ...draft }));
      case "get_email_draft": {
        const found = fullDrafts[args.id];
        return found ? { ...found } : null;
      }
      case "set_agent_email": {
        const input = args.input;
        return {
          ...account,
          host: input.host,
          port: input.port,
          security: input.security,
          username: input.username,
          from: input.from,
          allowlist: input.allowlist,
          allowlist_valid: true,
          has_password: account.has_password || Boolean(input.password),
        };
      }
      case "test_agent_email":
        return null;
      case "clear_agent_email":
        return null;
      case "update_email_draft": {
        const next: FullDraft = {
          ...fullDrafts[args.id],
          to_addresses: mapAddresses(args.to),
          cc_addresses: mapAddresses(args.cc),
          subject: args.subject,
          body: args.body,
          updated_at: "2026-11-27T11:00:00Z",
        };
        fullDrafts[args.id] = next;
        drafts = drafts.map((draft) =>
          draft.id === args.id ? { ...next } : draft,
        );
        return { ...next };
      }
      case "send_email_draft": {
        const existing = fullDrafts[args.id];
        if (sendFails) {
          const failed: FullDraft = {
            ...existing,
            error: "SMTP rejected the message",
            updated_at: "2026-11-27T11:01:00Z",
          };
          fullDrafts[args.id] = failed;
          drafts = drafts.map((draft) =>
            draft.id === args.id ? { ...failed } : draft,
          );
          throw new Error("SMTP rejected the message");
        }
        const sent: FullDraft = {
          ...existing,
          status: "sent",
          error: null,
          sent_at: "2026-11-27T11:02:00Z",
          updated_at: "2026-11-27T11:02:00Z",
        };
        fullDrafts[args.id] = sent;
        drafts = drafts.map((draft) =>
          draft.id === args.id ? { ...sent } : draft,
        );
        return null;
      }
      case "discard_email_draft": {
        const discarded: FullDraft = {
          ...fullDrafts[args.id],
          status: "discarded",
          updated_at: "2026-11-27T11:03:00Z",
        };
        fullDrafts[args.id] = discarded;
        drafts = drafts.map((draft) =>
          draft.id === args.id ? { ...discarded } : draft,
        );
        return null;
      }
      default:
        throw new Error(`unexpected command ${command}`);
    }
  });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("MailPanel", () => {
  it("renders the account and newest drafts without inventing an inbox", async () => {
    const { getByText, getByLabelText } = render(MailPanel);
    await waitFor(() => expect(getByText("Configured")).toBeTruthy());

    expect(getByText("Quarterly numbers")).toBeTruthy();
    expect(getByText("Sending report")).toBeTruthy();
    expect(getByText("Sent report")).toBeTruthy();
    expect(getByText("Discarded report")).toBeTruthy();
    expect(
      (getByLabelText("Recipient allowlist") as HTMLInputElement).value,
    ).toBe("example.com, boss@other.test");
    expect(document.body.textContent).toContain("There is no inbox");
    expect(document.querySelector('input[aria-label="Inbox"]')).toBeNull();
    expect(document.body.textContent).not.toContain("IMAP inbox");
  });

  it("shows a load error when the backend is unavailable", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_agent_email") throw new Error("offline");
      if (command === "list_email_drafts") return [];
      throw new Error(`unexpected command ${command}`);
    });
    const { getByRole } = render(MailPanel);
    await waitFor(() =>
      expect(getByRole("alert").textContent).toContain(
        "Could not load Mail.",
      ),
    );
    expect(getByRole("alert").textContent).toContain("offline");
  });

  it("saves the account with the exact input and omits a blank password", async () => {
    const { getByText, getByLabelText } = render(MailPanel);
    await waitFor(() => expect(getByText("Configured")).toBeTruthy());

    await fireEvent.input(getByLabelText("SMTP host"), {
      target: { value: "smtp.changed.test" },
    });
    await fireEvent.input(getByLabelText("Port"), {
      target: { value: "2525" },
    });
    await fireEvent.input(getByLabelText("Recipient allowlist"), {
      target: { value: "example.com, other.test" },
    });
    await fireEvent.click(getByText("Save account"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_agent_email", {
        input: {
          host: "smtp.changed.test",
          port: 2525,
          security: "starttls",
          username: "agent@example.com",
          from: "agent@example.com",
          allowlist: "example.com, other.test",
        },
      }),
    );
    const call = invokeMock.mock.calls.find(
      ([command]) => command === "set_agent_email",
    );
    expect(call).toBeTruthy();
    expect((call?.[1] as any).input).not.toHaveProperty("password");
    await waitFor(() => expect(getByText("Saved.")).toBeTruthy());
  });

  it("confirms before a real test email", async () => {
    const confirmMock = vi
      .spyOn(window, "confirm")
      .mockReturnValue(true);
    const { getByText } = render(MailPanel);
    await waitFor(() => expect(getByText("Configured")).toBeTruthy());

    await fireEvent.click(getByText("Send test email"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("test_agent_email", {
        to: "agent@example.com",
      }),
    );
    expect(confirmMock).toHaveBeenCalledWith(
      "Send a real test email to agent@example.com?",
    );
    await waitFor(() =>
      expect(getByText("Test email sent.")).toBeTruthy(),
    );
  });

  it("confirms before removing the account", async () => {
    const confirmMock = vi
      .spyOn(window, "confirm")
      .mockReturnValue(true);
    const { getByText } = render(MailPanel);
    await waitFor(() => expect(getByText("Configured")).toBeTruthy());

    await fireEvent.click(getByText("Remove account"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("clear_agent_email"),
    );
    expect(confirmMock).toHaveBeenCalled();
    await waitFor(() => expect(getByText("Not configured")).toBeTruthy());
  });

  it("opens a draft, edits it, saves, and refreshes the list", async () => {
    const { getByText, getByLabelText } = render(MailPanel);
    await waitFor(() => expect(getByText("Quarterly numbers")).toBeTruthy());
    const before = invokeMock.mock.calls.filter(
      ([command]) => command === "list_email_drafts",
    ).length;

    await fireEvent.click(getByText("Quarterly numbers"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("get_email_draft", {
        id: "draft-1",
      }),
    );
    const body = getByLabelText("Body") as HTMLTextAreaElement;
    expect(body.value).toBe("Body for Quarterly numbers");

    await fireEvent.input(getByLabelText("Subject"), {
      target: { value: "Updated subject" },
    });
    await fireEvent.input(body, {
      target: { value: "Updated body" },
    });
    await fireEvent.click(getByText("Save changes"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("update_email_draft", {
        id: "draft-1",
        to: "person@example.com",
        cc: "cc@example.com",
        subject: "Updated subject",
        body: "Updated body",
      }),
    );
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.filter(
          ([command]) => command === "list_email_drafts",
        ).length,
      ).toBeGreaterThan(before),
    );
  });

  it("loads an invalid saved allowlist and saves a repaired one", async () => {
    let stored = {
      ...account,
      allowlist: "bad domain",
      allowlist_valid: false,
    };
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command === "get_agent_email") return { ...stored };
      if (command === "list_email_drafts") return [];
      if (command === "set_agent_email") {
        const input = args.input;
        stored = {
          ...stored,
          host: input.host,
          port: input.port,
          security: input.security,
          username: input.username,
          from: input.from,
          allowlist: input.allowlist,
          allowlist_valid: true,
        };
        return { ...stored };
      }
      throw new Error(`unexpected command ${command}`);
    });

    const { getByText, getByLabelText, queryByText } = render(MailPanel);
    const warning =
      "The saved allowlist is invalid. Correct it and save to repair it.";
    await waitFor(() => expect(getByText(warning)).toBeTruthy());
    const allowlistInput = getByLabelText(
      "Recipient allowlist",
    ) as HTMLInputElement;
    expect(allowlistInput.value).toBe("bad domain");
    expect(getByText("Configured")).toBeTruthy();

    await fireEvent.input(allowlistInput, {
      target: { value: "example.com" },
    });
    await fireEvent.click(getByText("Save account"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_agent_email", {
        input: {
          host: "smtp.example.com",
          port: 587,
          security: "starttls",
          username: "agent@example.com",
          from: "agent@example.com",
          allowlist: "example.com",
        },
      }),
    );
    await waitFor(() => expect(queryByText(warning)).toBeNull());
    expect(allowlistInput.value).toBe("example.com");
  });

  it("does not send unsaved edits and sends the saved values after Save", async () => {
    const confirmMock = vi
      .spyOn(window, "confirm")
      .mockReturnValue(true);
    const { getByText, getByLabelText, getByRole } = render(MailPanel);
    await waitFor(() => expect(getByText("Quarterly numbers")).toBeTruthy());
    await fireEvent.click(getByText("Quarterly numbers"));
    await waitFor(() => expect(getByLabelText("Body")).toBeTruthy());

    await fireEvent.input(getByLabelText("Subject"), {
      target: { value: "Unsaved subject" },
    });
    expect(getByText("Save your changes before sending. The stored draft is what gets sent.")).toBeTruthy();
    expect(
      (getByRole("button", { name: "Send" }) as HTMLButtonElement).disabled,
    ).toBe(true);
    await fireEvent.click(getByRole("button", { name: "Send" }));
    expect(invokeMock).not.toHaveBeenCalledWith("send_email_draft", {
      id: "draft-1",
    });

    await fireEvent.click(getByText("Save changes"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("update_email_draft", {
        id: "draft-1",
        to: "person@example.com",
        cc: "cc@example.com",
        subject: "Unsaved subject",
        body: "Body for Quarterly numbers",
      }),
    );
    await waitFor(() =>
      expect(
        (getByRole("button", { name: "Send" }) as HTMLButtonElement).disabled,
      ).toBe(false),
    );
    await fireEvent.click(getByRole("button", { name: "Send" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("send_email_draft", {
        id: "draft-1",
      }),
    );
    const confirmation = confirmMock.mock.calls.at(-1)?.[0] as string;
    expect(confirmation).toContain("Unsaved subject");
    expect(confirmation).not.toContain("Quarterly numbers");
  });

  it("confirms a send, sends once, and shows the sent state", async () => {
    const confirmMock = vi
      .spyOn(window, "confirm")
      .mockReturnValue(false);
    const { getByText, getByLabelText, getByRole } = render(MailPanel);
    await waitFor(() => expect(getByText("Quarterly numbers")).toBeTruthy());
    await fireEvent.click(getByText("Quarterly numbers"));
    await waitFor(() => expect(getByLabelText("Body")).toBeTruthy());

    await fireEvent.click(getByRole("button", { name: "Send" }));
    await waitFor(() =>
      expect(confirmMock).toHaveBeenCalledWith(
        expect.stringContaining("person@example.com, cc@example.com"),
      ),
    );
    expect(invokeMock).not.toHaveBeenCalledWith("send_email_draft", {
      id: "draft-1",
    });

    confirmMock.mockReturnValue(true);
    await fireEvent.click(getByRole("button", { name: "Send" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("send_email_draft", {
        id: "draft-1",
      }),
    );
    await waitFor(() =>
      expect(
        (getByRole("button", { name: "Send" }) as HTMLButtonElement).disabled,
      ).toBe(true),
    );
    expect((getByLabelText("Subject") as HTMLInputElement).disabled).toBe(
      true,
    );
    await waitFor(() =>
      expect(document.body.textContent).toMatch(/Sent /),
    );
  });

  it("confirms a discard and leaves the draft read-only", async () => {
    const confirmMock = vi
      .spyOn(window, "confirm")
      .mockReturnValue(true);
    const { getByText, getByLabelText, getByRole } = render(MailPanel);
    await waitFor(() => expect(getByText("Quarterly numbers")).toBeTruthy());
    await fireEvent.click(getByText("Quarterly numbers"));
    await waitFor(() => expect(getByLabelText("Body")).toBeTruthy());

    await fireEvent.click(getByRole("button", { name: "Discard" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("discard_email_draft", {
        id: "draft-1",
      }),
    );
    expect(confirmMock).toHaveBeenCalledWith(
      "Discard this draft? This cannot be undone.",
    );
    await waitFor(() =>
      expect((getByLabelText("Subject") as HTMLInputElement).disabled).toBe(
        true,
      ),
    );
  });

  it("disables edit, send and discard for sending, sent and discarded", async () => {
    const { getByText, getByLabelText, getByRole } = render(MailPanel);
    await waitFor(() => expect(getByText("Sending report")).toBeTruthy());

    await fireEvent.click(getByText("Sending report"));
    await waitFor(() => expect(getByText("Sending…")).toBeTruthy());
    expect((getByLabelText("To") as HTMLInputElement).disabled).toBe(true);
    expect(
      (getByRole("button", { name: "Send" }) as HTMLButtonElement).disabled,
    ).toBe(true);
    await fireEvent.click(getByRole("button", { name: "Close" }));

    await fireEvent.click(getByText("Sent report"));
    await waitFor(() =>
      expect(document.body.textContent).toMatch(/Sent /),
    );
    expect((getByLabelText("Subject") as HTMLInputElement).disabled).toBe(
      true,
    );
    expect(
      (getByRole("button", { name: "Discard" }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
    await fireEvent.click(getByRole("button", { name: "Close" }));

    await fireEvent.click(getByText("Discarded report"));
    await waitFor(() =>
      expect((getByLabelText("Subject") as HTMLInputElement).disabled).toBe(
        true,
      ),
    );
    expect(
      (getByRole("button", { name: "Send" }) as HTMLButtonElement).disabled,
    ).toBe(true);
  });

  it("shows a failed send, reloads the draft, and keeps it retryable", async () => {
    sendFails = true;
    vi.spyOn(window, "confirm").mockReturnValue(true);
    const { getByText, getAllByText, getByLabelText, getByRole } = render(
      MailPanel,
    );
    await waitFor(() => expect(getByText("Quarterly numbers")).toBeTruthy());
    await fireEvent.click(getByText("Quarterly numbers"));
    await waitFor(() => expect(getByLabelText("Body")).toBeTruthy());
    const before = invokeMock.mock.calls.filter(
      ([command]) => command === "get_email_draft",
    ).length;

    await fireEvent.click(getByRole("button", { name: "Send" }));
    await waitFor(() =>
      expect(
        getAllByText(/SMTP rejected the message/).length,
      ).toBeGreaterThan(0),
    );
    await waitFor(() =>
      expect(
        (getByLabelText("Subject") as HTMLInputElement).disabled,
      ).toBe(false),
    );
    expect(
      (getByRole("button", { name: "Send" }) as HTMLButtonElement).disabled,
    ).toBe(false);
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.filter(
          ([command]) => command === "get_email_draft",
        ).length,
      ).toBeGreaterThan(before),
    );
  });
});
