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

let pending: MessageFocusTarget | null = null;

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

/** Drop an unclaimed hand-off, for example when its bot is deleted. */
export function clearPendingMessageFocus(botId: string): void {
  if (pending?.botId === botId) pending = null;
}
