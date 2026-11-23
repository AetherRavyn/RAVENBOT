/**
 * Local composer drafts.
 *
 * One key per conversation, not one per app: switching agents or threads must
 * never move a half-written message into the wrong conversation. Writes are
 * debounced so typing does not hit localStorage on every keystroke, and a
 * failed send can put the text back where the user left it.
 */
const PREFIX = "ravenbot.draft.";

export function threadDraftKey(botId: string, threadId: string | null | undefined): string {
  return `thread:${botId}:${threadId || "new"}`;
}

export function officeDraftKey(roomId: string): string {
  return `office:${roomId}`;
}

class DraftStore {
  private timers = new Map<string, ReturnType<typeof setTimeout>>();

  load(key: string): string {
    try {
      return localStorage.getItem(PREFIX + key) ?? "";
    } catch {
      return "";
    }
  }

  /** Save after a short idle so a fast typist does not serialize per key. */
  save(key: string, text: string): void {
    const existing = this.timers.get(key);
    if (existing) clearTimeout(existing);
    this.timers.set(
      key,
      setTimeout(() => {
        this.timers.delete(key);
        this.saveNow(key, text);
      }, 300),
    );
  }

  /** Save immediately, for a switch or a failed send. */
  saveNow(key: string, text: string): void {
    const existing = this.timers.get(key);
    if (existing) {
      clearTimeout(existing);
      this.timers.delete(key);
    }
    try {
      if (text.trim()) localStorage.setItem(PREFIX + key, text);
      else localStorage.removeItem(PREFIX + key);
    } catch {
      /* storage unavailable — the draft simply does not survive a restart */
    }
  }

  clear(key: string): void {
    const existing = this.timers.get(key);
    if (existing) {
      clearTimeout(existing);
      this.timers.delete(key);
    }
    try {
      localStorage.removeItem(PREFIX + key);
    } catch {
      /* storage unavailable */
    }
  }
}

export const drafts = new DraftStore();
