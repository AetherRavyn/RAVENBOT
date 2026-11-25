import { describe, expect, it } from "vitest";
import {
  decodeAttentionItems,
  decodeMessageContent,
  decodeMessages,
  decodeThreads,
} from "./decode";

const STAMP = "2026-11-25T09:00:00.000Z";

describe("decodeThreads", () => {
  it("keeps a complete thread row intact", () => {
    const thread = {
      id: "thread-1",
      bot_id: "bot-1",
      title: "Plan the release",
      is_active: true,
      ephemeral: true,
      channel_id: "channel-1",
      kind: "office",
      visible: false,
      created_at: STAMP,
      updated_at: STAMP,
    };

    expect(decodeThreads([thread])).toEqual([thread]);
  });

  it("fills defaults for missing fields instead of throwing", () => {
    expect(decodeThreads([{ id: "thread-2" }])).toEqual([
      {
        id: "thread-2",
        bot_id: "",
        title: "",
        is_active: true,
        ephemeral: false,
        channel_id: null,
        kind: "chat",
        visible: true,
        created_at: "",
        updated_at: "",
      },
    ]);
  });

  it("falls back to chat for an unknown kind", () => {
    const [thread] = decodeThreads([{ id: "thread-3", kind: "something-new" }]);
    expect(thread.kind).toBe("chat");
  });

  it("drops malformed rows and non-array payloads", () => {
    const decoded = decodeThreads([
      null,
      "thread",
      42,
      [],
      {},
      { id: "" },
      { id: 7 },
      { id: "thread-4" },
    ]);

    expect(decoded).toHaveLength(1);
    expect(decoded[0].id).toBe("thread-4");
    expect(decodeThreads(undefined)).toEqual([]);
    expect(decodeThreads({ id: "thread-5" })).toEqual([]);
  });
});

describe("decodeMessages", () => {
  it("keeps a complete message and its nested content intact", () => {
    const message = {
      id: "message-1",
      thread_id: "thread-1",
      role: "assistant",
      content: {
        type: "Text",
        text: "Here is the result.",
        sources: [{ url: "https://example.com", title: "Example" }],
        reasoning: "checked the docs",
        tools: [
          {
            name: "web_search",
            arguments: { query: "release" },
            result: "ok",
            is_error: false,
            duration_ms: 120,
            cost: 0.01,
          },
        ],
        spans: [
          {
            id: "span-1",
            kind: "tool",
            label: "web_search",
            started_ms: 10,
            duration_ms: 120,
            failed: true,
            detail: "timeout",
          },
        ],
      },
      attachments: [
        {
          id: "attachment-1",
          name: "notes.txt",
          mime_type: "text/plain",
          size: 12,
          path: "/tmp/notes.txt",
          data: "aGVsbG8=",
          is_image: false,
        },
      ],
      sender_bot_id: "bot-1",
      sender_name: "Scout",
      reply_to_id: "message-0",
      reactions: ["👍"],
      context_used_tokens: 100,
      context_window_tokens: 8000,
      context_compacted: true,
      context_system_tokens: 20,
      context_tool_tokens: 30,
      context_memory_tokens: 5,
      context_message_tokens: 45,
      created_at: STAMP,
    };

    expect(decodeMessages([message])).toEqual([message]);
  });

  it("normalizes a bare string content into a Text block", () => {
    const [message] = decodeMessages([
      { id: "message-2", thread_id: "thread-1", role: "user", content: "plain text" },
    ]);

    expect(message.content).toEqual({ type: "Text", text: "plain text" });
  });

  it("normalizes a legacy object without a type tag", () => {
    expect(decodeMessageContent({ text: "legacy body" })).toEqual({
      type: "Text",
      text: "legacy body",
    });
  });

  it("normalizes case and separator variants of a tag", () => {
    expect(
      decodeMessageContent({
        type: "tool_call",
        tool_name: "shell_exec",
        arguments: { command: "ls" },
      }),
    ).toEqual({
      type: "ToolCall",
      tool_name: "shell_exec",
      arguments: { command: "ls" },
    });

    expect(
      decodeMessageContent({
        type: "ToolResult",
        tool_name: "shell_exec",
        result: "ok",
        is_error: true,
      }),
    ).toEqual({
      type: "ToolResult",
      tool_name: "shell_exec",
      result: "ok",
      is_error: true,
    });
  });

  it("keeps checklist rows and repairs an unknown status", () => {
    expect(
      decodeMessageContent({
        type: "Checklist",
        text: "Steps",
        items: [
          { label: "One", status: "Completed", result: "ok", thread_id: "t", bot_id: "b" },
          { label: "Two", status: "NotAStatus", result: 42 },
          null,
        ],
      }),
    ).toEqual({
      type: "Checklist",
      text: "Steps",
      items: [
        { label: "One", status: "Completed", result: "ok", thread_id: "t", bot_id: "b" },
        { label: "Two", status: "Pending", result: null, thread_id: null, bot_id: null },
      ],
    });
  });

  it("falls back to safe empty text for unknown or non-object content", () => {
    expect(decodeMessageContent({ type: "Mystery", text: "kept as prose" })).toEqual({
      type: "Text",
      text: "kept as prose",
    });
    expect(decodeMessageContent({ type: "Mystery" })).toEqual({
      type: "Text",
      text: "",
    });
    expect(decodeMessageContent(42)).toEqual({ type: "Text", text: "42" });
    expect(decodeMessageContent(null)).toEqual({ type: "Text", text: "" });
  });

  it("does not throw on malformed nested arrays", () => {
    const [message] = decodeMessages([
      {
        id: "message-3",
        thread_id: "thread-1",
        role: "assistant",
        content: {
          type: "Text",
          text: "still readable",
          tools: [null, {}, { name: "" }],
          spans: [null, {}, { id: "" }],
        },
        attachments: [null, "file", { id: "" }, {}],
      },
    ]);

    expect(message.content).toEqual({ type: "Text", text: "still readable" });
    expect(message.attachments).toEqual([]);
  });

  it("fills safe defaults for a sparse row", () => {
    const [message] = decodeMessages([{ id: "message-4" }]);

    expect(message).toMatchObject({
      id: "message-4",
      thread_id: "",
      role: "assistant",
      content: { type: "Text", text: "" },
      attachments: [],
      reactions: [],
      context_compacted: false,
      created_at: "",
    });
  });

  it("drops malformed rows and non-array payloads", () => {
    const decoded = decodeMessages([
      null,
      "message",
      42,
      [],
      {},
      { id: "" },
      { id: "message-5", content: "kept" },
    ]);

    expect(decoded.map((message) => message.id)).toEqual(["message-5"]);
    expect(decodeMessages(undefined)).toEqual([]);
    expect(decodeMessages({ messages: [] })).toEqual([]);
  });
});

describe("decodeAttentionItems", () => {
  it("keeps a complete approval/alert row intact", () => {
    const item = {
      id: "attention-1",
      kind: "alert",
      bot_id: "bot-1",
      bot_name: "Scout",
      thread_id: "thread-1",
      office_id: "office-1",
      office_name: "Acme",
      title: "Budget warning",
      detail: "82% of the office budget is spent",
      risk: null,
      state: "running",
      question_kind: null,
      alert_kind: "budget",
      percent: 82.5,
      budget: 10,
      cost: 8.25,
      created_at: STAMP,
    };

    expect(decodeAttentionItems([item])).toEqual([item]);
  });

  it("fills defaults and omits absent optional alert fields", () => {
    const [item] = decodeAttentionItems([
      {
        id: "attention-2",
        kind: "question",
        bot_name: "Scout",
        title: "Pick one",
        detail: "Which database?",
        created_at: STAMP,
      },
    ]);

    expect(item).toEqual({
      id: "attention-2",
      kind: "question",
      bot_id: null,
      bot_name: "Scout",
      thread_id: null,
      office_id: null,
      office_name: null,
      title: "Pick one",
      detail: "Which database?",
      risk: null,
      state: "pending",
      question_kind: null,
      created_at: STAMP,
    });
    expect("alert_kind" in item).toBe(false);
    expect("percent" in item).toBe(false);
    expect("budget" in item).toBe(false);
    expect("cost" in item).toBe(false);
  });

  it("defaults an unknown kind to queue so it never pings as a gate", () => {
    const [item] = decodeAttentionItems([
      { id: "attention-3", kind: "future-kind", title: "Unknown" },
    ]);
    expect(item.kind).toBe("queue");
  });

  it("coerces non-numeric alert fields to null", () => {
    const [item] = decodeAttentionItems([
      {
        id: "attention-4",
        kind: "alert",
        alert_kind: "budget",
        percent: "82",
        budget: Number.NaN,
        cost: null,
      },
    ]);

    expect(item.percent).toBeNull();
    expect(item.budget).toBeNull();
    expect(item.cost).toBeNull();
  });

  it("drops malformed rows and non-array payloads", () => {
    const decoded = decodeAttentionItems([
      null,
      "approval",
      42,
      [],
      {},
      { id: "" },
      { id: "attention-5", kind: "approval", title: "Allow?" },
    ]);

    expect(decoded.map((item) => item.id)).toEqual(["attention-5"]);
    expect(decodeAttentionItems(undefined)).toEqual([]);
    expect(decodeAttentionItems({ items: [] })).toEqual([]);
  });
});
