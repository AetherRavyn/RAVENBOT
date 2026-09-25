// Shared voice I/O for RAVENBOT.
//
// The browser Web Speech API is not available in Tauri's WebKitGTK webview
// (Linux), so speech-to-text and text-to-speech run through the Rust backend
// (`transcribe_audio` / `synthesize_speech`). This keeps voice working on
// Linux and macOS with honest errors when no engine is configured.

import { invoke } from "@tauri-apps/api/core";

export interface UtteranceRecorder {
  /** Stop recording immediately; `result` resolves with what was captured. */
  stop(): void;
  /** Resolves with the recorded audio once stopped, or null when empty. */
  result: Promise<Blob | null>;
}

/** Pick a MediaRecorder container the platform supports. */
export function pickAudioMime(): string {
  const MR: any = (window as any).MediaRecorder;
  if (MR?.isTypeSupported) {
    for (const c of [
      "audio/webm;codecs=opus",
      "audio/webm",
      "audio/ogg;codecs=opus",
      "audio/mp4",
    ]) {
      if (MR.isTypeSupported(c)) return c;
    }
  }
  return "";
}

/** Turn a backend/invoke error into an actionable message for the user. */
export function voiceErrorMessage(e: unknown): string {
  const msg = String((e as any)?.message || e || "");
  if (/getUserMedia|not available|permission|denied|NotAllowed/i.test(msg)) {
    return "Microphone unavailable or permission denied. Grant microphone access to RAVENBOT and try again.";
  }
  if (/No STT engine|Whisper|faster-whisper|Transcription failed/i.test(msg)) {
    return "No speech-to-text engine is configured. Add an OpenAI API key (Settings → API Keys) or install faster-whisper, then try again.";
  }
  if (/No TTS engine|PyTorch|transformers|HUGGINGFACE/i.test(msg)) {
    return "No text-to-speech engine is configured. Set HUGGINGFACE_INFERENCE_TOKEN or install the local TTS model.";
  }
  return `Voice error: ${msg}`;
}

export function blobToBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const dataUrl = String(reader.result || "");
      resolve(dataUrl.split(",")[1] || "");
    };
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(blob);
  });
}

/**
 * Acquire the microphone and start recording one utterance. Recording stops
 * automatically after `silenceMs` of quiet (voice-activity detection) or the
 * `maxMs` hard cap, or immediately when `stop()` is called.
 */
export async function recordUtterance(
  opts: { maxMs?: number; silenceMs?: number } = {},
): Promise<UtteranceRecorder> {
  const maxMs = opts.maxMs ?? 20000;
  const silenceMs = opts.silenceMs ?? 1500;
  const md = navigator.mediaDevices;
  if (!md?.getUserMedia) {
    throw new Error("Microphone capture is not available in this environment.");
  }
  const stream = await md.getUserMedia({ audio: true });
  const mimeType = pickAudioMime();
  const recorder = new MediaRecorder(stream, mimeType ? { mimeType } : undefined);
  const chunks: Blob[] = [];
  recorder.ondataavailable = (e) => {
    if (e.data && e.data.size > 0) chunks.push(e.data);
  };

  const ctx = new AudioContext();
  const source = ctx.createMediaStreamSource(stream);
  const analyser = ctx.createAnalyser();
  analyser.fftSize = 512;
  source.connect(analyser);
  // Some WebViews start the context suspended; voice-activity detection needs
  // it running. Best effort — recording still works if this is ignored.
  try {
    await ctx.resume();
  } catch {}
  const buf = new Uint8Array(analyser.frequencyBinCount);
  const startedAt = Date.now();
  let lastVoiceAt = Date.now();
  let heardVoice = false;
  let stopped = false;
  let timer = 0;

  const finish = () => {
    if (stopped) return;
    stopped = true;
    window.clearInterval(timer);
    try {
      recorder.stop();
    } catch {}
    try {
      source.disconnect();
    } catch {}
    try {
      ctx.close();
    } catch {}
    stream.getTracks().forEach((t) => t.stop());
  };

  timer = window.setInterval(() => {
    analyser.getByteTimeDomainData(buf);
    let peak = 0;
    for (let i = 0; i < buf.length; i++) peak = Math.max(peak, Math.abs(buf[i] - 128));
    if (peak > 12) {
      heardVoice = true;
      lastVoiceAt = Date.now();
    }
    const now = Date.now();
    if (now - startedAt >= maxMs || (heardVoice && now - lastVoiceAt >= silenceMs)) finish();
  }, 100);

  const result = new Promise<Blob | null>((resolve) => {
    recorder.onstop = () =>
      resolve(chunks.length ? new Blob(chunks, { type: mimeType || "audio/webm" }) : null);
    recorder.onerror = () => resolve(null);
    recorder.start();
  });

  return { stop: finish, result };
}

/** Send recorded audio to the backend STT engine and return the transcript. */
export async function transcribeBlob(blob: Blob): Promise<string> {
  const base64 = await blobToBase64(blob);
  const mime = blob.type || "audio/webm";
  const format = mime.includes("ogg") ? "ogg" : mime.includes("mp4") ? "m4a" : "webm";
  const res: any = await invoke("transcribe_audio", { audioBase64: base64, format });
  return String(res?.text || "").trim();
}

export interface SpeechHandle {
  stop(): void;
  done: Promise<void>;
}

/** Synthesize `text` via the backend and play it; resolves when playback ends. */
export async function speakText(text: string, voice?: string): Promise<SpeechHandle> {
  const res: any = await invoke("synthesize_speech", {
    text,
    voice: voice || null,
  });
  const audio = new Audio(`data:audio/wav;base64,${res.audio_b64}`);
  const done = new Promise<void>((resolve) => {
    audio.onended = () => resolve();
    audio.onerror = () => resolve();
    audio.play().catch(() => resolve());
  });
  return {
    stop() {
      try {
        audio.pause();
      } catch {}
    },
    done,
  };
}

/** Strip markdown/reasoning so TTS reads only the answer. */
export function stripForSpeech(md: string): string {
  return (md || "")
    .replace(/<think>[\s\S]*?<\/think>/gi, " ")
    .replace(/([\s\S]*?)<\/think>/g, "")
    .replace(/```[\s\S]*?```/g, " code omitted. ")
    .replace(/!\[[^\]]*\]\([^)]*\)/g, "")
    .replace(/\[([^\]]*)\]\(([^)]*)\)/g, "$1")
    .replace(/[#*_>`|~]+/g, " ")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, 1500);
}
