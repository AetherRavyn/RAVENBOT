// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  blobToBase64,
  pickAudioMime,
  speakText,
  stripForSpeech,
  transcribeBlob,
  voiceErrorMessage,
} from "./voice";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = vi.mocked(invoke);

function withMediaRecorder(types: string[]) {
  (window as any).MediaRecorder = {
    isTypeSupported: (type: string) => types.includes(type),
  };
}

afterEach(() => {
  delete (window as any).MediaRecorder;
  vi.unstubAllGlobals();
  invokeMock.mockReset();
});

describe("pickAudioMime", () => {
  it("prefers opus WebM when the webview supports it", () => {
    withMediaRecorder(["audio/webm;codecs=opus", "audio/webm", "audio/mp4"]);
    expect(pickAudioMime()).toBe("audio/webm;codecs=opus");
  });

  it("falls back in priority order and skips unsupported types", () => {
    withMediaRecorder(["audio/mp4", "audio/ogg;codecs=opus"]);
    expect(pickAudioMime()).toBe("audio/ogg;codecs=opus");
    withMediaRecorder(["audio/mp4"]);
    expect(pickAudioMime()).toBe("audio/mp4");
  });

  it("returns an empty string when no container is supported", () => {
    withMediaRecorder([]);
    expect(pickAudioMime()).toBe("");
    delete (window as any).MediaRecorder;
    expect(pickAudioMime()).toBe("");
  });
});

describe("voiceErrorMessage", () => {
  it("explains microphone permission failures", () => {
    const message = voiceErrorMessage(new Error("NotAllowedError: Permission denied"));
    expect(message).toContain("Microphone unavailable or permission denied");
    expect(message).toContain("Grant microphone access");
  });

  it("explains missing speech-to-text engines", () => {
    const message = voiceErrorMessage("No STT engine configured");
    expect(message).toContain("No speech-to-text engine is configured");
    expect(message).toContain("OpenAI API key");
  });

  it("explains missing text-to-speech engines", () => {
    const message = voiceErrorMessage("No TTS engine available");
    expect(message).toContain("No text-to-speech engine is configured");
    expect(message).toContain("HUGGINGFACE_INFERENCE_TOKEN");
  });

  it("passes unknown errors through with a visible prefix", () => {
    expect(voiceErrorMessage("backend exploded")).toBe("Voice error: backend exploded");
    expect(voiceErrorMessage({ message: "boom" })).toBe("Voice error: boom");
  });
});

describe("stripForSpeech", () => {
  it("removes reasoning blocks, code, links and markdown punctuation", () => {
    const text = stripForSpeech(
      "<think>secret reasoning</think>Hello **world** [`docs`](https://example.test) ![chart](x.png)\n```js\nrm -rf /\n```\nDone.",
    );
    expect(text).not.toContain("secret reasoning");
    expect(text).not.toContain("rm -rf");
    expect(text).not.toContain("https://example.test");
    expect(text).toContain("code omitted.");
    expect(text).toContain("Hello world docs");
    expect(text).toContain("Done.");
  });

  it("drops everything before a dangling closing think tag", () => {
    expect(stripForSpeech("visible\n\n  answer </think>   more")).toBe("more");
  });

  it("caps spoken text at 1,500 characters", () => {
    expect(stripForSpeech("a".repeat(2_000))).toHaveLength(1_500);
    expect(stripForSpeech("")).toBe("");
  });
});

describe("blobToBase64", () => {
  it("returns the payload without the data URL prefix", async () => {
    const encoded = await blobToBase64(new Blob(["hello"], { type: "text/plain" }));
    expect(encoded).toBe("aGVsbG8=");
  });
});

describe("transcribeBlob", () => {
  it("sends base64 audio with a backend-supported format", async () => {
    invokeMock.mockResolvedValue({ text: "  hello there  " });
    await expect(
      transcribeBlob(new Blob(["hello"], { type: "audio/webm;codecs=opus" })),
    ).resolves.toBe("hello there");
    expect(invokeMock).toHaveBeenCalledWith("transcribe_audio", {
      audioBase64: "aGVsbG8=",
      format: "webm",
    });
  });

  it("maps OGG and MP4 containers to their backend formats", async () => {
    invokeMock.mockResolvedValue({ text: "ok" });
    await transcribeBlob(new Blob(["a"], { type: "audio/ogg" }));
    expect(invokeMock).toHaveBeenLastCalledWith("transcribe_audio", {
      audioBase64: "YQ==",
      format: "ogg",
    });
    await transcribeBlob(new Blob(["a"], { type: "audio/mp4" }));
    expect(invokeMock).toHaveBeenLastCalledWith("transcribe_audio", {
      audioBase64: "YQ==",
      format: "m4a",
    });
  });

  it("returns an empty string when the backend omits text", async () => {
    invokeMock.mockResolvedValue({});
    await expect(transcribeBlob(new Blob(["a"]))).resolves.toBe("");
  });
});

describe("speakText", () => {
  it("plays returned WAV audio and resolves when playback ends", async () => {
    invokeMock.mockResolvedValue({ audio_b64: "UklGRg==" });
    const played: string[] = [];
    const paused = vi.fn();
    class FakeAudio {
      onended: (() => void) | null = null;
      onerror: (() => void) | null = null;
      constructor(public src: string) {
        played.push(src);
      }
      play() {
        queueMicrotask(() => this.onended?.());
        return Promise.resolve();
      }
      pause() {
        paused();
      }
    }
    vi.stubGlobal("Audio", FakeAudio as unknown as typeof Audio);

    const handle = await speakText("hello", "voice-1");
    expect(played).toEqual(["data:audio/wav;base64,UklGRg=="]);
    expect(invokeMock).toHaveBeenCalledWith("synthesize_speech", {
      text: "hello",
      voice: "voice-1",
    });
    await expect(handle.done).resolves.toBeUndefined();
    handle.stop();
    expect(paused).toHaveBeenCalledOnce();
  });

  it("still resolves playback when play() is rejected", async () => {
    invokeMock.mockResolvedValue({ audio_b64: "UklGRg==" });
    class BlockedAudio {
      onended: (() => void) | null = null;
      onerror: (() => void) | null = null;
      play() {
        return Promise.reject(new Error("autoplay blocked"));
      }
      pause() {}
    }
    vi.stubGlobal("Audio", BlockedAudio as unknown as typeof Audio);

    const handle = await speakText("hello");
    await expect(handle.done).resolves.toBeUndefined();
    expect(invokeMock).toHaveBeenCalledWith("synthesize_speech", {
      text: "hello",
      voice: null,
    });
  });
});
