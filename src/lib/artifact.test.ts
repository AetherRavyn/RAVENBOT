import { describe, expect, it } from "vitest";
import {
  artifactKindFor,
  makeArtifact,
  qualifiesAsArtifact,
  type Artifact,
} from "./artifact";

describe("qualifiesAsArtifact", () => {
  it("accepts blocks of 8+ lines", () => {
    const code = Array.from({ length: 8 }, (_, i) => `line ${i}`).join("\n");
    expect(qualifiesAsArtifact(code)).toBe(true);
  });

  it("accepts short but large blocks", () => {
    const code = "x".repeat(500);
    expect(qualifiesAsArtifact(code)).toBe(true);
  });

  it("rejects small snippets", () => {
    expect(qualifiesAsArtifact("println!(\"hi\");")).toBe(false);
  });

  it("rejects empty/whitespace-only blocks", () => {
    expect(qualifiesAsArtifact("")).toBe(false);
    expect(qualifiesAsArtifact("   \n  \n")).toBe(false);
  });
});

describe("artifactKindFor", () => {
  it("maps html languages to html kind", () => {
    expect(artifactKindFor("html")).toBe("html");
    expect(artifactKindFor("htm")).toBe("html");
    expect(artifactKindFor("svg")).toBe("html");
  });

  it("maps markdown languages to markdown kind", () => {
    expect(artifactKindFor("markdown")).toBe("markdown");
    expect(artifactKindFor("md")).toBe("markdown");
  });

  it("defaults everything else to code kind", () => {
    expect(artifactKindFor("rust")).toBe("code");
    expect(artifactKindFor("ts")).toBe("code");
    expect(artifactKindFor("")).toBe("code");
  });
});

describe("makeArtifact", () => {
  it("derives a title from a leading comment", () => {
    const artifact = makeArtifact("// streaming accumulator\ncode();", "js");
    expect(artifact.title).toBe("streaming accumulator");
    expect(artifact.language).toBe("js");
    expect(artifact.kind).toBe("code");
  });

  it("derives a title from an html comment", () => {
    const artifact = makeArtifact("<!-- landing page -->\n<div></div>", "html");
    expect(artifact.title).toBe("landing page");
    expect(artifact.kind).toBe("html");
  });

  it("falls back to a language-based title", () => {
    const artifact = makeArtifact("fn main() {}", "rust");
    expect(artifact.title).toBe("rust snippet");
  });

  it("falls back from a too-long comment title", () => {
    const artifact = makeArtifact("// " + "y".repeat(80), "py");
    expect(artifact.title).toBe("py snippet");
  });

  it("always returns a complete artifact object", () => {
    const artifact = makeArtifact("body", "md");
    const keys = Object.keys(artifact).sort();
    expect(keys).toEqual(["content", "id", "kind", "language", "title"].sort());
  });
});
