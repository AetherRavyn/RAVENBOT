import { beforeEach, describe, expect, it } from "vitest";
import { makeArtifact } from "./artifact";
import { artifactStore } from "./artifacts.svelte";

beforeEach(() => artifactStore.clear());

describe("artifact store", () => {
  it("opens an artifact and keeps a deduplicated recent list", () => {
    const first = makeArtifact("// First\nconst a = 1;", "ts");
    const second = makeArtifact("// Second\nconst b = 2;", "ts");
    artifactStore.open(first);
    artifactStore.open(second);
    artifactStore.open(first);
    expect(artifactStore.current?.title).toBe("First");
    expect(artifactStore.history.map((item) => item.title)).toEqual(["First", "Second"]);
  });

  it("closes the current artifact without discarding history", () => {
    const artifact = makeArtifact("// Kept\nconst a = 1;", "ts");
    artifactStore.open(artifact);
    artifactStore.close();
    expect(artifactStore.current).toBeNull();
    expect(artifactStore.history).toHaveLength(1);
    artifactStore.select(artifactStore.history[0]);
    expect(artifactStore.current?.id).toBe(artifact.id);
  });
});
