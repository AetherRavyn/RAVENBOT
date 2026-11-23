// @vitest-environment jsdom
import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  MESSAGE_FOCUS_EVENT,
  THREAD_FOCUS_EVENT,
  clearPendingMessageFocus,
  requestMessageFocus,
  requestThreadFocus,
  takePendingMessageFocus,
  takePendingThreadFocus,
} from "$lib/chat/messageFocus";

const target = {
  botId: "bot-1",
  threadId: "thread-1",
  messageId: "message-1",
};

describe("message focus hand-off", () => {
  beforeEach(() => {
    clearPendingMessageFocus("bot-1");
  });

  it("dispatches the request and gives it to the owning bot once", () => {
    const listener = vi.fn();
    window.addEventListener(MESSAGE_FOCUS_EVENT, listener);
    requestMessageFocus(target);

    expect(listener).toHaveBeenCalledTimes(1);
    expect(takePendingMessageFocus("bot-2")).toBeNull();
    expect(takePendingMessageFocus("bot-1")).toEqual(target);
    expect(takePendingMessageFocus("bot-1")).toBeNull();
    window.removeEventListener(MESSAGE_FOCUS_EVENT, listener);
  });

  it("keeps a request for a conversation that has not mounted yet", () => {
    requestMessageFocus(target);
    expect(takePendingMessageFocus("other-bot")).toBeNull();
    expect(takePendingMessageFocus("bot-1")).toEqual(target);
  });

  it("hands a thread-only request to the owning conversation once", () => {
    const listener = vi.fn();
    window.addEventListener(THREAD_FOCUS_EVENT, listener);
    requestThreadFocus({ botId: "bot-1", threadId: "thread-9" });

    expect(listener).toHaveBeenCalledTimes(1);
    expect(takePendingThreadFocus("bot-2")).toBeNull();
    expect(takePendingThreadFocus("bot-1")).toEqual({
      botId: "bot-1",
      threadId: "thread-9",
    });
    expect(takePendingThreadFocus("bot-1")).toBeNull();
    window.removeEventListener(THREAD_FOCUS_EVENT, listener);
    clearPendingMessageFocus("bot-1");
  });
});
