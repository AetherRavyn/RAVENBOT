<script lang="ts">
  /**
   * A non-media attachment in the transcript.
   *
   * PDFs, archives, spreadsheets and other documents used to be preserved in
   * the message but invisible in the conversation: the model was told the file
   * existed and the person who sent it could no longer get it back. This is the
   * card that makes the file a real part of the message.
   */
  import {
    Download,
    File,
    FileArchive,
    FileSpreadsheet,
    FileText,
    Presentation,
  } from "@lucide/svelte";
  import {
    base64ByteLength,
    downloadAttachment,
    formatBytes,
  } from "$lib/attachments";

  interface Props {
    att: {
      name?: string | null;
      data?: string | null;
      mime_type?: string | null;
      mime?: string | null;
      size?: number | null;
    };
    class?: string;
  }

  let { att, class: cls = "" }: Props = $props();

  const mime = $derived((att.mime_type || att.mime || "").toLowerCase());
  const name = $derived(att.name || "attachment");
  const extension = $derived(
    name.includes(".") ? name.split(".").pop()?.toUpperCase() || "" : "",
  );
  // The inline payload is the source of truth. Rows written before the byte
  // count was fixed still carry the base64 character count in `size`.
  const size = $derived(
    att.data ? base64ByteLength(att.data) : Math.max(0, att.size ?? 0),
  );
  const Icon = $derived(
    mime.includes("zip") ||
      mime.includes("compressed") ||
      mime.includes("tar") ||
      mime.includes("rar")
      ? FileArchive
      : mime.includes("sheet") ||
          mime.includes("excel") ||
          mime.includes("csv") ||
          extension === "CSV" ||
          extension === "XLS" ||
          extension === "XLSX"
        ? FileSpreadsheet
        : mime.includes("presentation") ||
            mime.includes("powerpoint") ||
            extension === "PPT" ||
            extension === "PPTX"
          ? Presentation
          : mime === "application/pdf" || mime.startsWith("text/")
            ? FileText
            : File,
  );
  const meta = $derived(
    [extension || mime.split("/").pop()?.toUpperCase() || "FILE", formatBytes(size)]
      .filter(Boolean)
      .join(" · "),
  );

  function save() {
    downloadAttachment(att);
  }
</script>

<button
  type="button"
  class={"file-attachment " + cls}
  onclick={save}
  aria-label={`Download ${name}`}
  title={`Download ${name}`}
>
  <span class="file-attachment__icon" aria-hidden="true">
    <Icon class="size-4" strokeWidth={1.7} />
  </span>
  <span class="file-attachment__copy">
    <span class="file-attachment__name">{name}</span>
    <span class="file-attachment__meta">{meta}</span>
  </span>
  <Download class="file-attachment__download size-3.5" strokeWidth={1.8} />
</button>
