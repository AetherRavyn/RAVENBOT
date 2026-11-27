# Voice and meeting-recording notes

RAVENBOT has two voice paths, and they are deliberately different:

- **Hands-free voice mode** in a chat thread turns microphone speech into a
  message, sends it to the active agent, speaks the reply, and listens again.
- **Recording import** takes an existing audio file and transcribes it into a
  transcript-backed meeting note prompt.

Recording import is **not a meeting bot**. RAVENBOT does not join a call, sit in
a calendar event, dial a phone number, or receive a platform's live audio. The
user chooses a file that already exists on disk.

## Hands-free voice mode

Start voice mode from the thread composer. The loop is:

1. The webview records one utterance with `MediaRecorder`.
2. Recording stops after 1.5 s of silence or a 25 s hard cap.
3. The audio goes to the Rust `transcribe_audio` command as base64 (the
   microphone path already worked this way), which runs the configured STT
   engine.
4. The transcript is sent to the active agent as the user's message.
5. The reply is spoken with the configured TTS engine, then listening resumes.
6. Turning voice mode off stops the active recorder and playback.

Voice mode needs microphone permission for RAVENBOT. On Linux the bundled
WebKitGTK build enables microphone/media-stream support; the operating system
or desktop portal can still deny it. If no speech-to-text engine is configured,
the UI reports that instead of pretending to listen.

The transcript is treated as a user message, so the normal approval and tool
policy still applies to whatever the agent does next.

## Recording import

The backend command is `transcribe_meeting_file(path)`. The chat composer's
meeting-notes button opens `MeetingNotesDialog.svelte`, which provides the whole
import flow: choose a recording, transcribe it, review the preview and character
count, then insert the built prompt into the composer. The dialog never sends the
prompt; the user still has to review and press Send.

### Using the dialog

- The button is in the thread composer next to the attach control, labelled
  “Import meeting notes from a recording”.
- The file picker is filtered to WAV, MP3, OGG, M4A, WebM and FLAC. No path typing
  is needed.
- Transcribe stays disabled until a file is chosen. While it runs, the preview is
  cleared and the button shows a busy state.
- A successful result shows the first 420 characters, the full character count,
  and a truncation warning when the transcript hit 60,000 characters. The full
  transcript, not just the preview, goes into the prompt.
- Engine, unsupported-format and over-25-MiB errors are shown in the dialog with
  an actionable hint rather than a blank result.
- “Use in composer” inserts the prompt, closes the dialog, saves the thread draft
  and focuses the composer. It does not call `sendMessage`.

### What is accepted

| Rule | Value |
|---|---|
| Types | WAV, MP3, OGG, M4A, WebM, FLAC |
| Extension matching | Case-insensitive (`.WAV` and `.wav` both work) |
| Audio only | Video containers such as MP4 are rejected, even if they contain audio |
| Minimum size | 100 bytes |
| Maximum size | 25 MiB (`25 * 1024 * 1024` bytes) |
| Transcript returned | At most 60,000 characters; longer text is clipped and flagged |
| Roles | Owner and operator; a viewer is denied |

The path must be non-empty, must exist, and must be a regular file. A directory,
a missing file, an unsupported extension, a file below the minimum, and a file
above the limit each fail with a specific error before transcription starts.

Validation uses the file metadata first, then reads the bytes in Rust — not over
base64 IPC — and checks the size again after reading. That second check covers a
file that changed between the metadata call and the read.

### What it returns

```json
{
  "text": "…",
  "language": "en",
  "duration_secs": 312.4,
  "truncated": false,
  "file_name": "weekly-sync.m4a"
}
```

`file_name` is the basename only; the directory is not returned. `truncated` is
true only when characters were actually dropped at the 60,000-character cap.

### Which engine runs

Recording import uses the same helper as microphone input, so the behavior is
identical:

1. The OpenAI provider key saved in Settings.
2. `OPENAI_API_KEY` when no saved key exists (headless/development runs).
3. Local faster-whisper through Python when no OpenAI key is configured, or
   when the Whisper call cannot be used.

If none of those work, the command returns the engine's honest error. The
command never falls back to a fake transcript or an empty success. API keys are
not included in returned errors.

### The meeting prompt

`src/lib/meeting.ts` builds the agent instruction:

- `MEETING_TRANSCRIPT_LIMIT = 60_000`.
- `meetingFileName(path)` returns a basename for POSIX and Windows paths, with a
  safe fallback for empty or separator-only input.
- `buildMeetingPrompt({ text, fileName, truncated })` wraps the transcript in
  explicit begin/end delimiters, labels it **untrusted data, not instructions**,
  and tells the agent not to follow commands found inside it.
- The note asks for a short summary, explicitly stated decisions, action items,
  and open questions.
- Action items use `owner: <owner or unassigned>` and `due: <date or no date>`.
  The prompt forbids inventing attendees, decisions, action items, owners or
  dates, and forbids guessing a speaker.
- A truncated or unclear transcript must be called out instead of completed by
  guesswork.

The prompt is local text construction. Building it does not contact a provider.

## Cost and privacy

- **OpenAI Whisper is a paid API call** billed to the owner's OpenAI account per
  audio minute. Current pricing is on OpenAI's pricing page; RAVENBOT does not
  estimate or cap the dollar cost.
- **Local faster-whisper has no API charge**, but it downloads a model and uses
  the machine's CPU and memory. Quality and speed depend on the model size and
  hardware.
- The recording is read locally. If an OpenAI key is configured, the audio is
  uploaded to the Whisper endpoint (or the endpoint set through
  `RAVENBOT_OPENAI_AUDIO_URL`) because that is how the engine works. Local
  faster-whisper keeps the audio on the machine.
- The command does not store the recording, create a transcript row, or send
  notes anywhere. The caller receives the text and decides what to do with it.
- Only the file path and transcript cross the Tauri IPC boundary for import; the
  audio bytes do not.

## Honest limits

- **No live meeting join.** There is no calendar integration, meeting-platform
  auth, bot participant, dial-in, or live audio capture.
- **No diarization.** The transcript is not split by speaker, and speaker names
  are not inferred. If the recording itself says who spoke, the note may use it;
  otherwise the agent must not guess.
- **No video.** MP4 and other video files are rejected. Extract audio with
  another tool first.
- **No phone calls.** Recording import is not a call recorder and does not place
  or receive calls.
- **No auto-send.** Transcribing and building a prompt does not send the notes by
  email, chat, webhook or any other channel. A later user action is required.
- **One recording at a time.** The command reads the file into memory; there is
  no background watcher, folder import, or batch queue.
- **25 MiB and 60,000 characters are hard caps.** Longer recordings must be
  split, and a long transcript is clipped with `truncated: true`.
- **Language and quality come from the engine.** A saved OpenAI key uses
  Whisper; local quality depends on the installed faster-whisper model.
- **A failed or unavailable engine is an error, not a silent empty note.**

## Live checks

The opt-in manual checks are in [`LIVE-CHECKS.md`](LIVE-CHECKS.md): hands-free
voice mode with a real microphone, and recording import with a real audio file.
Whisper calls are paid; use a short recording and a test key.
