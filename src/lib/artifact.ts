// Artifact detection & typing for the canvas/preview panel

export type ArtifactKind = "code" | "html" | "markdown";

export interface Artifact {
  id: string;
  title: string;
  language: string;
  kind: ArtifactKind;
  content: string;
}

let artifactCounter = 0;

export function nextArtifactId(): string {
  artifactCounter += 1;
  return `artifact-${Date.now()}-${artifactCounter}`;
}

export function artifactKindFor(language: string): ArtifactKind {
  const lang = (language || "").toLowerCase();
  if (lang === "html" || lang === "htm" || lang === "svg") return "html";
  if (lang === "markdown" || lang === "md") return "markdown";
  return "code";
}

export function qualifiesAsArtifact(code: string): boolean {
  if (!code || !code.trim()) return false;
  const lineCount = code.split("\n").length;
  return lineCount >= 8 || code.length >= 500;
}

export function makeArtifact(content: string, language: string): Artifact {
  const kind = artifactKindFor(language);
  const langLabel = language || "text";
  // Derive a title from a leading comment/filename if present
  const firstLine = content
    .split("\n")
    .map((l) => l.trim())
    .find((l) => l.length > 0) || "";
  const commentMatch = firstLine.match(/^(?:\/\/|#|<!--|--)\s*(.+?)(?:-->)?$/);
  const derivedTitle =
    commentMatch && commentMatch[1].trim().length > 0 && commentMatch[1].trim().length < 60
      ? commentMatch[1].trim()
      : `${langLabel} snippet`;
  return {
    id: nextArtifactId(),
    title: derivedTitle,
    language: langLabel,
    kind,
    content,
  };
}
