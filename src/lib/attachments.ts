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

/** The largest file a composer will keep as an inline attachment. */
export const MAX_ATTACHMENT_BYTES = 24 * 1024 * 1024;

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

/** Decoded byte length of a base64 payload (no data-URI prefix). */
export function base64ByteLength(data: string): number {
  const clean = (data || "").replace(/\s+/g, "");
  if (!clean) return 0;
  const padding = clean.endsWith("==") ? 2 : clean.endsWith("=") ? 1 : 0;
  return Math.max(0, Math.floor((clean.length * 3) / 4) - padding);
}

/** Compact, locale-neutral file size for an attachment card. */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  const index = Math.min(units.length - 1, Math.floor(Math.log(bytes) / Math.log(1024)));
  const value = bytes / 1024 ** index;
  const digits = index === 0 || value >= 10 ? 0 : 1;
  return `${Number(value.toFixed(digits))} ${units[index]}`;
}

/** Build a Blob from a stored attachment's base64 payload. */
export function attachmentBlob(att: {
  data?: string | null;
  mime_type?: string | null;
  mime?: string | null;
}): Blob {
  const binary = atob(att.data || "");
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
  return new Blob([bytes], {
    type: att.mime_type || att.mime || "application/octet-stream",
  });
}

/**
 * Save an attachment through the browser's download path.
 *
 * Returns false when there are no bytes to save, so the caller can leave the
 * control honest rather than opening an empty file.
 */
export function downloadAttachment(att: {
  name?: string | null;
  data?: string | null;
  mime_type?: string | null;
  mime?: string | null;
}): boolean {
  if (!att?.data) return false;
  const url = URL.createObjectURL(attachmentBlob(att));
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = att.name || "attachment";
  anchor.rel = "noopener";
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
  setTimeout(() => URL.revokeObjectURL(url), 0);
  return true;
}
