// Shared file-attachment classification/reading for RAVENBOT composers.
//
// Images become inline vision attachments; text-like files are inlined into
// the message so the model receives them directly; everything else (PDFs,
// docs, archives) is attached as a document and surfaced to the model by name.

export interface PendingAttachment {
  name: string;
  mime: string;
  data: string;
  isImage: boolean;
}

export const ACCEPTED_IMAGE_MIMES = [
  "image/png",
  "image/jpeg",
  "image/jpg",
  "image/gif",
  "image/webp",
];

const TEXT_FILE_EXTS = [
  "txt", "md", "markdown", "rs", "ts", "tsx", "js", "jsx", "mjs", "cjs", "py",
  "json", "jsonc", "toml", "yaml", "yml", "css", "scss", "less", "html", "htm",
  "xml", "sh", "bash", "zsh", "fish", "sql", "go", "java", "c", "cpp", "cc",
  "h", "hpp", "rb", "php", "swift", "kt", "kts", "cs", "lua", "r", "dart",
  "env", "ini", "cfg", "conf", "log", "csv", "tsv", "tex", "vue", "svelte",
];

const TEXT_MIMES = [
  "application/json",
  "application/xml",
  "application/x-yaml",
  "application/yaml",
  "application/javascript",
  "application/toml",
  "application/sql",
];

export type FileKind = "image" | "text" | "document";

export function fileExt(file: File): string {
  return (file.name?.split(".").pop() || "").toLowerCase();
}

export function isImageFile(file: File): boolean {
  return ACCEPTED_IMAGE_MIMES.includes((file.type || "").toLowerCase());
}

export function isTextFile(file: File): boolean {
  const mime = (file.type || "").toLowerCase();
  if (mime.startsWith("text/")) return true;
  if (TEXT_MIMES.includes(mime)) return true;
  return TEXT_FILE_EXTS.includes(fileExt(file));
}

export function classifyFile(file: File): FileKind {
  if (isImageFile(file)) return "image";
  if (isTextFile(file)) return "text";
  return "document";
}

export function readAsDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result || ""));
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(file);
  });
}

export function readAsText(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result || ""));
    reader.onerror = () => reject(reader.error);
    reader.readAsText(file);
  });
}

export function base64FromDataUrl(dataUrl: string): string {
  return dataUrl.split(",")[1] || "";
}
