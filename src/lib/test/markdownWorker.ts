/**
 * Shared test double for the markdown worker.
 *
 * Component tests need a real request/response boundary without starting a
 * browser worker; the client and worker protocol are exercised separately.
 */
import type { MarkdownWorkerLike } from "$lib/markdown/offThread";

export class FakeMarkdownWorker implements MarkdownWorkerLike {
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

  requests(kind: string): any[] {
    return this.messages.filter((message) => message?.kind === kind);
  }

  /** Install a constructor that hands every `new Worker(...)` this instance. */
  install(): void {
    const worker = this;
    class FakeWorker {
      constructor() {
        return worker;
      }
    }
    globalThis.Worker = FakeWorker as unknown as typeof Worker;
  }

  uninstall(): void {
    delete (globalThis as { Worker?: unknown }).Worker;
  }
}

export function textToken(text: string) {
  return { type: "text", raw: text, text };
}
