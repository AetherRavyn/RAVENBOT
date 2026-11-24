import { describe, expect, it, vi } from "vitest";
import {
  createMarkdownWorkerClient,
  OFF_THREAD_LEX_MIN_CHARS,
  type MarkdownWorkerLike,
} from "./offThread";

class FakeWorker implements MarkdownWorkerLike {
  messages: any[] = [];
  terminated = false;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onerror: ((event: ErrorEvent) => void) | null = null;
  onmessageerror: ((event: MessageEvent) => void) | null = null;

  postMessage(message: unknown): void {
    this.messages.push(message);
  }

  terminate(): void {
    this.terminated = true;
  }

  respond(message: unknown): void {
    this.onmessage?.({ data: message } as MessageEvent);
  }

  crash(message = "worker crashed"): void {
    this.onerror?.({ message } as ErrorEvent);
  }
}

function textTokens(text: string) {
  return [{ type: "text", raw: text, text }];
}

async function settle(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}

describe("markdown worker client", () => {
  it("lexes off-thread and serves the result from cache", async () => {
    const worker = new FakeWorker();
    const client = createMarkdownWorkerClient(() => worker, 1_000);
    const pending = client.lex("hello");
    await settle();
    expect(worker.messages[0]).toMatchObject({ kind: "lex", body: "hello" });

    worker.respond({
      id: worker.messages[0].id,
      ok: true,
      tokens: textTokens("hello"),
    });
    const tokens = await pending;
    expect(tokens).toEqual(textTokens("hello"));
    expect(client.cachedLex("hello")).toEqual(tokens);

    const cached = await client.lex("hello");
    expect(cached).toEqual(tokens);
    expect(worker.messages).toHaveLength(1);
  });

  it("coalesces duplicate lex requests for the same body", async () => {
    const worker = new FakeWorker();
    const client = createMarkdownWorkerClient(() => worker, 1_000);
    const first = client.lex("same body");
    const second = client.lex("same body");
    await settle();
    expect(worker.messages).toHaveLength(1);
    worker.respond({
      id: worker.messages[0].id,
      ok: true,
      tokens: textTokens("same body"),
    });
    await expect(first).resolves.toEqual(textTokens("same body"));
    await expect(second).resolves.toEqual(textTokens("same body"));
  });

  it("highlights with the language in the cache key", async () => {
    const worker = new FakeWorker();
    const client = createMarkdownWorkerClient(() => worker, 1_000);
    const first = client.highlight("const x = 1;", "ts");
    await settle();
    worker.respond({
      id: worker.messages[0].id,
      ok: true,
      tokens: [{ type: "keyword", text: "const" }],
    });
    await first;
    expect(client.cachedHighlight("const x = 1;", "ts")).toBeTruthy();
    expect(client.cachedHighlight("const x = 1;", "js")).toBeUndefined();

    const second = client.highlight("const x = 1;", "ts");
    await second;
    expect(worker.messages).toHaveLength(1);
  });

  it("reports unavailable and falls back without throwing", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => undefined);
    const client = createMarkdownWorkerClient(() => null, 20);
    expect(client.available()).toBe(false);
    await expect(client.lex("body")).resolves.toBeNull();
    await expect(client.highlight("code", "ts")).resolves.toBeNull();
    warn.mockRestore();
  });

  it("kills a worker that times out and cools down", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => undefined);
    const worker = new FakeWorker();
    const client = createMarkdownWorkerClient(() => worker, 15);
    await expect(client.lex("slow body")).resolves.toBeNull();
    expect(worker.terminated).toBe(true);
    expect(warn).toHaveBeenCalled();
    warn.mockRestore();
  });

  it("falls back when the worker crashes or sends malformed JSON", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => undefined);
    const worker = new FakeWorker();
    const client = createMarkdownWorkerClient(() => worker, 1_000);
    const pending = client.lex("body");
    await settle();
    worker.crash("segfault");
    await expect(pending).resolves.toBeNull();
    expect(worker.terminated).toBe(true);

    const worker2 = new FakeWorker();
    const client2 = createMarkdownWorkerClient(() => worker2, 1_000);
    const malformed = client2.lex("body");
    await settle();
    worker2.respond({ id: worker2.messages[0].id, ok: true, tokens: "not an array" });
    await expect(malformed).resolves.toBeNull();
    warn.mockRestore();
  });

  it("prewarms only long, unseen bodies and caches their tokens", async () => {
    const worker = new FakeWorker();
    const client = createMarkdownWorkerClient(() => worker, 1_000);
    const body = "x".repeat(OFF_THREAD_LEX_MIN_CHARS);
    client.prewarm(["short", body, body]);
    await settle();
    expect(worker.messages).toHaveLength(1);
    expect(worker.messages[0]).toMatchObject({ kind: "lex", body });
    worker.respond({
      id: worker.messages[0].id,
      ok: true,
      tokens: textTokens("prewarmed"),
    });
    await settle();
    expect(client.cachedLex(body)).toEqual(textTokens("prewarmed"));
    client.prewarm([body]);
    expect(worker.messages).toHaveLength(1);
  });

  it("resets caches, pending requests and the worker", async () => {
    const workers: FakeWorker[] = [];
    const client = createMarkdownWorkerClient(() => {
      const worker = new FakeWorker();
      workers.push(worker);
      return worker;
    }, 1_000);
    const pending = client.lex("old");
    await settle();
    client.reset();
    await expect(pending).resolves.toBeNull();
    expect(client.cachedLex("old")).toBeUndefined();
    expect(workers[0].terminated).toBe(true);

    const next = client.lex("new");
    await settle();
    expect(workers).toHaveLength(2);
    workers[1].respond({
      id: workers[1].messages[0].id,
      ok: true,
      tokens: textTokens("new"),
    });
    await expect(next).resolves.toEqual(textTokens("new"));
  });
});
