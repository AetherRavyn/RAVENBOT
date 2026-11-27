<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { AlertTriangle, FileAudio, Loader2, NotebookPen, X } from "@lucide/svelte";
  import { rememberFocus, trapFocus } from "$lib/a11y";
  import { t } from "$lib/i18n";
  import { buildMeetingPrompt, meetingFileName } from "$lib/meeting";

  interface MeetingTranscript {
    text: string;
    language: string | null;
    duration_secs: number;
    truncated: boolean;
    file_name: string;
  }

  interface Props {
    onClose: () => void;
    onUsePrompt: (prompt: string) => void;
  }

  let { onClose, onUsePrompt }: Props = $props();

  let dialogEl = $state<HTMLElement | null>(null);
  let restoreFocus: (() => void) | null = null;
  let recordingPath = $state("");
  let result = $state<MeetingTranscript | null>(null);
  let choosing = $state(false);
  let busy = $state(false);
  let error = $state("");

  const fileName = $derived(
    result?.file_name || (recordingPath ? meetingFileName(recordingPath) : ""),
  );
  const characterCount = $derived(result ? Array.from(result.text).length : 0);
  const preview = $derived(previewOf(result?.text ?? ""));

  function previewOf(text: string): string {
    const flat = text.replace(/\s+/g, " ").trim();
    return flat.length > 420 ? `${flat.slice(0, 420)}…` : flat;
  }

  function errorHint(): string {
    const message = error.toLowerCase();
    if (/no stt|faster[-_]whisper|whisper|openai_api_key/.test(message)) {
      return t("meeting.noEngineHint");
    }
    if (/unsupported recording|unsupported/.test(message)) {
      return t("meeting.unsupported");
    }
    if (/25 mib|larger than the 25/.test(message)) {
      return t("meeting.tooLarge");
    }
    return "";
  }

  onMount(() => {
    restoreFocus = rememberFocus();
    return () => {
      if (restoreFocus) restoreFocus();
    };
  });

  $effect(() => {
    const element = dialogEl;
    if (!element) return;
    return trapFocus(element);
  });

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.stopPropagation();
      onClose();
    }
  }

  async function chooseRecording() {
    if (choosing || busy) return;
    choosing = true;
    error = "";
    try {
      const picked = await openDialog({
        multiple: false,
        directory: false,
        filters: [
          {
            name: t("meeting.title"),
            extensions: ["wav", "mp3", "ogg", "m4a", "webm", "flac"],
          },
        ],
      });
      const selected = Array.isArray(picked) ? picked[0] : picked;
      if (!selected) return;
      recordingPath = selected;
      result = null;
    } catch (cause) {
      error = String(cause);
    } finally {
      choosing = false;
    }
  }

  async function transcribe() {
    if (!recordingPath || busy) return;
    busy = true;
    error = "";
    result = null;
    try {
      const next = await invoke<MeetingTranscript>("transcribe_meeting_file", {
        path: recordingPath,
      });
      if (!next.text.trim()) {
        error = t("meeting.error");
        return;
      }
      result = next;
    } catch (cause) {
      error = String(cause);
    } finally {
      busy = false;
    }
  }

  function useInComposer() {
    if (!result) return;
    onUsePrompt(
      buildMeetingPrompt({
        text: result.text,
        fileName: result.file_name || fileName || meetingFileName(recordingPath),
        truncated: result.truncated,
      }),
    );
    onClose();
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-50 flex items-center justify-center bg-[var(--scrim)] p-4"
  onclick={(event) => {
    if (event.target === event.currentTarget) onClose();
  }}
>
  <div
    class="modal-panel flex max-h-[88vh] w-full max-w-2xl flex-col"
    role="dialog"
    aria-modal="true"
    aria-labelledby="meeting-notes-title"
    tabindex="-1"
    bind:this={dialogEl}
    onkeydown={handleKeydown}
    onclick={(event) => event.stopPropagation()}
  >
    <header class="flex items-start justify-between gap-3 border-b border-[var(--hairline)] px-5 py-4">
      <div class="flex min-w-0 items-start gap-3">
        <div class="flex size-9 shrink-0 items-center justify-center rounded-xl border border-[var(--brand)]/25 bg-[var(--brand-soft)]">
          <NotebookPen class="size-4 text-[var(--brand-text)]" />
        </div>
        <div class="min-w-0">
          <h2 id="meeting-notes-title" class="text-sm font-bold text-[var(--text-primary)]">
            {t("meeting.title")}
          </h2>
          <p class="mt-1 text-[11px] leading-5 text-[var(--text-tertiary)]">
            {t("meeting.description")}
          </p>
        </div>
      </div>
      <button
        type="button"
        class="flex size-7 shrink-0 cursor-pointer items-center justify-center rounded-lg text-[var(--text-tertiary)] hover:bg-[var(--surface-3)] hover:text-[var(--text-primary)]"
        aria-label={t("meeting.close")}
        title={t("meeting.close")}
        onclick={onClose}
      >
        <X class="size-4" />
      </button>
    </header>

    <div class="min-h-0 flex-1 space-y-4 overflow-y-auto px-5 py-4">
      <section class="space-y-2" aria-label={t("meeting.choose")}>
        <div class="flex flex-wrap items-center gap-2">
          <button
            type="button"
            class="inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-lg border border-[var(--brand)]/40 bg-[var(--brand-soft)] px-3 text-[11px] font-semibold text-[var(--brand-text)] transition-colors hover:bg-[var(--brand)]/15 disabled:opacity-50"
            onclick={() => void chooseRecording()}
            disabled={choosing || busy}
          >
            {#if choosing}
              <Loader2 class="size-3.5 animate-spin" />
            {:else}
              <FileAudio class="size-3.5" />
            {/if}
            {t("meeting.choose")}
          </button>
          {#if fileName}
            <span class="min-w-0 truncate rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-2.5 py-1.5 font-mono text-[11px] text-[var(--text-secondary)]">
              {t("meeting.file")}: {fileName}
            </span>
          {/if}
        </div>
        <p class="text-[11px] leading-5 text-[var(--text-muted)]">
          {t("meeting.chooseHint")}
        </p>

        <button
          type="button"
          class="inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-lg border border-[var(--hairline-strong)] bg-[var(--surface-2)] px-3 text-[11px] font-semibold text-[var(--text-secondary)] transition-colors hover:bg-[var(--surface-3)] hover:text-[var(--text-primary)] disabled:opacity-50"
          onclick={() => void transcribe()}
          disabled={!recordingPath || busy || choosing}
        >
          {#if busy}
            <Loader2 class="size-3.5 animate-spin" />
          {:else}
            <NotebookPen class="size-3.5" />
          {/if}
          {busy ? t("meeting.transcribing") : t("meeting.transcribe")}
        </button>
      </section>

      {#if error}
        <div class="rounded-xl border border-[var(--danger-border)] bg-[var(--danger-soft)] px-3 py-2" role="alert">
          <p class="text-[11px] font-semibold text-[var(--danger-text)]">
            {t("meeting.error")}: <span class="font-normal break-words">{error}</span>
          </p>
          {#if errorHint()}
            <p class="mt-1 text-[11px] leading-5 text-[var(--danger-text)]/90">{errorHint()}</p>
          {/if}
        </div>
      {/if}

      {#if result}
        <section class="space-y-2" aria-label={t("meeting.preview")}>
          <div class="flex flex-wrap items-center justify-between gap-2">
            <span class="text-[11px] font-semibold text-[var(--text-secondary)]">
              {t("meeting.preview")}
            </span>
            <span class="text-[11px] text-[var(--text-muted)]">
              {t("meeting.chars", { count: characterCount })}
            </span>
          </div>
          {#if result.truncated}
            <p class="flex items-start gap-2 rounded-lg border border-[var(--warning-border)] bg-[var(--warning-soft)] px-2.5 py-2 text-[11px] leading-5 text-[var(--warning-text)]" role="status">
              <AlertTriangle class="mt-0.5 size-3.5 shrink-0" />
              <span>{t("meeting.truncated")}</span>
            </p>
          {/if}
          <pre class="max-h-52 overflow-y-auto whitespace-pre-wrap break-words rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] p-3 font-mono text-[11px] leading-5 text-[var(--text-secondary)]">{preview}</pre>
        </section>
      {/if}

      <section class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] px-3 py-2">
        <ul class="list-disc space-y-1 pl-4 text-[11px] leading-5 text-[var(--text-muted)]">
          <li>{t("meeting.audioOnly")}</li>
          <li>{t("meeting.maxSize")}</li>
          <li>{t("meeting.noLiveJoin")}</li>
          <li>{t("meeting.noDiarization")}</li>
          <li>{t("meeting.costHint")}</li>
        </ul>
      </section>
    </div>

    <footer class="flex flex-wrap items-center justify-between gap-2 border-t border-[var(--hairline)] px-5 py-3">
      <p class="min-w-0 max-w-md text-[11px] leading-4 text-[var(--text-tertiary)]">
        {t("meeting.reviewHint")}
      </p>
      <div class="flex items-center gap-2">
        <button
          type="button"
          class="inline-flex h-8 cursor-pointer items-center rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-3 text-[11px] font-semibold text-[var(--text-secondary)] hover:bg-[var(--surface-3)]"
          onclick={onClose}
        >
          {t("meeting.close")}
        </button>
        <button
          type="button"
          class="inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-lg border border-[var(--brand)]/40 bg-[var(--brand-soft)] px-3 text-[11px] font-semibold text-[var(--brand-text)] hover:bg-[var(--brand)]/15 disabled:cursor-not-allowed disabled:opacity-50"
          onclick={useInComposer}
          disabled={!result}
        >
          <NotebookPen class="size-3.5" />
          {t("meeting.useInComposer")}
        </button>
      </div>
    </footer>
  </div>
</div>
