/**
 * The artifact currently open in the workspace side panel, plus a small recent
 * list. Conversation code blocks call `open`; the panel reads `current`.
 *
 * The history is in memory by design: an artifact is a view of text that is
 * already persisted in the message that produced it, not a second document
 * store to migrate.
 */
import type { Artifact } from "$lib/artifact";

class ArtifactStore {
  current = $state<Artifact | null>(null);
  history = $state<Artifact[]>([]);

  open(artifact: Artifact): void {
    this.current = artifact;
    const without = this.history.filter((item) => item.id !== artifact.id);
    this.history = [artifact, ...without].slice(0, 20);
  }

  select(artifact: Artifact): void {
    this.current = artifact;
    const without = this.history.filter((item) => item.id !== artifact.id);
    this.history = [artifact, ...without].slice(0, 20);
  }

  close(): void {
    this.current = null;
  }

  clear(): void {
    this.current = null;
    this.history = [];
  }
}

export const artifactStore = new ArtifactStore();
