/**
 * Cross-surface message focus.
 *
 * A global-search hit names a bot, a thread, and a message. The palette may be
 * closing while the target conversation is still mounting, so the request is
 * kept as a pending hand-off until the thread that owns it takes it.
 */

export interface MessageFocusTarget {
  botId: string;
  threadId: string;
  messageId: string;
}

export const MESSAGE_FOCUS_EVENT = "ravenbot:focus-message";

/** A request to open one conversation without revealing a particular message. */
export interface ThreadFocusTarget {
  botId: string;
  threadId: string;
}

export const THREAD_FOCUS_EVENT = "ravenbot:focus-thread";

let pending: MessageFocusTarget | null = null;
let pendingThread: ThreadFocusTarget | null = null;

/** Ask the conversation to open a thread and reveal one message. */
export function requestMessageFocus(target: MessageFocusTarget): void {
  pending = target;
  if (typeof window !== "undefined") {
    window.dispatchEvent(
      new CustomEvent<MessageFocusTarget>(MESSAGE_FOCUS_EVENT, { detail: target }),
    );
  }
}

/**
 * Take the hand-off for `botId`, exactly once. A different bot leaves it
 * pending for the conversation that is about to mount.
 */
export function takePendingMessageFocus(botId: string): MessageFocusTarget | null {
  if (!pending || pending.botId !== botId) return null;
  const target = pending;
  pending = null;
  return target;
}

/** Ask a conversation to open one of its threads. */
export function requestThreadFocus(target: ThreadFocusTarget): void {
  pendingThread = target;
  if (typeof window !== "undefined") {
    window.dispatchEvent(
      new CustomEvent<ThreadFocusTarget>(THREAD_FOCUS_EVENT, { detail: target }),
    );
  }
}

/** Take the thread hand-off for `botId`, exactly once. */
export function takePendingThreadFocus(botId: string): ThreadFocusTarget | null {
  if (!pendingThread || pendingThread.botId !== botId) return null;
  const target = pendingThread;
  pendingThread = null;
  return target;
}

/** Drop an unclaimed hand-off, for example when its bot is deleted. */
export function clearPendingMessageFocus(botId: string): void {
  if (pending?.botId === botId) pending = null;
  if (pendingThread?.botId === botId) pendingThread = null;
}
