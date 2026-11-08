// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import {
  attachmentBlob,
  base64ByteLength,
  formatBytes,
} from "./attachments";

describe("attachment sizes", () => {
  it("counts decoded bytes, not base64 characters", () => {
    expect(base64ByteLength("")).toBe(0);
    expect(base64ByteLength("aGk=")).toBe(2);
    expect(base64ByteLength("aGVsbG8=")).toBe(5);
    expect(base64ByteLength("aGVsbG8h")).toBe(6);
    // Whitespace from wrapped base64 must not inflate the size.
    expect(base64ByteLength("aGVs\nbG8=")).toBe(5);
  });

  it("formats the same number a download will have", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(2 * 1024 * 1024)).toBe("2 MB");
    expect(formatBytes(12.5 * 1024 * 1024)).toBe("13 MB");
    expect(formatBytes(Number.NaN)).toBe("0 B");
  });

  it("builds a typed Blob from the stored payload", async () => {
    const blob = attachmentBlob({
      data: "aGVsbG8=",
      mime_type: "text/plain",
    });
    expect(blob.type).toBe("text/plain");
    expect(blob.size).toBe(5);
    expect(await blob.text()).toBe("hello");
  });
});
