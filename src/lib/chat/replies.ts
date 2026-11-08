/**
 * Quoted replies.
 *
 * A reply stores the id of the message it answers. The transcript is the only
 * place the quoted text lives, so the quote is resolved at render time: a
 * message edited later shows its current text, and a deleted message leaves a
 * clear "unavailable" marker rather than a stale copy.
 */
import { messageKey, messageText, type SearchableMessage } from "./search";

export interface ReplyQuote {
  /** Id of the message being quoted, used as the jump target. */
  id: string;
  /** Display name of the person or agent who wrote it. */
  author: string;
  /** One or two lines of the original text; empty for an attachment-only row. */
  text: string;
}

const MAX_QUOTE = 180;

/** Resolve a reply id against the loaded transcript. */
export function replyQuoteFor(
  messages: readonly SearchableMessage[],
  replyToId: string | null | undefined,
  authorFor: (message: SearchableMessage) => string,
  unavailable: string,
): ReplyQuote | null {
  if (!replyToId) return null;
  const id = String(replyToId);
  const target = messages.find((message) => messageKey(message) === id);
  if (!target) return { id, author: "", text: unavailable };

  const text = messageText(target).replace(/\s+/g, " ").trim();
  return {
    id,
    author: authorFor(target),
    text: text.length > MAX_QUOTE ? `${text.slice(0, MAX_QUOTE - 1)}…` : text,
  };
}
