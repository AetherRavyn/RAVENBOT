//! Conversation-history helpers.
//!
//! The thread switcher used to create rows called `Thread #4`, and the selected
//! thread was forgotten on every reload. Keeping these rules in one small module
//! makes them testable without mounting the whole chat surface.

const SELECTED_THREAD_PREFIX = "ravenbot:selected-thread:";
const PLACEHOLDER = /^thread\s*#\d+$/i;
const MAX_TITLE_CHARS = 48;

/** localStorage key for the last thread opened for a bot. */
export function selectedThreadKey(botId: string): string {
  return `${SELECTED_THREAD_PREFIX}${botId}`;
}

export function readSelectedThread(
  botId: string | null | undefined,
  storage: Pick<Storage, "getItem"> | null = typeof localStorage === "undefined" ? null : localStorage,
): string | null {
  if (!botId || !storage) return null;
  try {
    const value = storage.getItem(selectedThreadKey(botId))?.trim();
    return value || null;
  } catch {
    return null;
  }
}

export function rememberSelectedThread(
  botId: string | null | undefined,
  threadId: string | null,
  storage: Pick<Storage, "setItem" | "removeItem"> | null = typeof localStorage === "undefined"
    ? null
    : localStorage,
): void {
  if (!botId || !storage) return;
  try {
    if (threadId) {
      storage.setItem(selectedThreadKey(botId), threadId);
    } else {
      storage.removeItem(selectedThreadKey(botId));
    }
  } catch {
    // A full or disabled localStorage must not break the conversation.
  }
}

/** True for the generated titles a user has not replaced yet. */
export function isPlaceholderThreadTitle(title: string | null | undefined): boolean {
  const value = (title ?? "").trim();
  return value === "" || value === "New Thread" || PLACEHOLDER.test(value);
}

/**
 * Turn the first user message into a short, readable thread name.
 *
 * Tool prefixes (`[DeepSearch]`, `[Think]`) are control flags, not part of what
 * the person asked, so they never become the title. Whitespace is collapsed and
 * the cut is made on a word boundary when one is close to the limit.
 */
export function threadTitleFromMessage(text: string, maxChars = MAX_TITLE_CHARS): string {
  let cleaned = text;
  // The composer can stack both flags (`[Think] [DeepSearch] …`), and one
  // pass would leave the second flag in the title.
  while (/^\s*\[(?:deepsearch|think)\]\s*/i.test(cleaned)) {
    cleaned = cleaned.replace(/^\s*\[(?:deepsearch|think)\]\s*/i, "");
  }
  cleaned = cleaned.replace(/\s+/g, " ").trim();
  if (!cleaned) return "";

  const characters = Array.from(cleaned);
  const limit = Math.max(12, maxChars);
  if (characters.length <= limit) return cleaned;

  const cut = characters.slice(0, limit).join("");
  const lastSpace = cut.lastIndexOf(" ");
  const title = (lastSpace >= Math.floor(limit * 0.6) ? cut.slice(0, lastSpace) : cut).trimEnd();
  return `${title}…`;
}
