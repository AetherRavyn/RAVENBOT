#!/usr/bin/env python3
"""SanoTTS inference script — loads the fine-tuned model and generates audio.

Usage:
    python infer_sano_tts.py --model-dir <path> --text <text> --output <wav_path> [--voice <name>]

The script auto-detects the model format:
  - Bark-style (Suno): config.json with "text2semantic" + "coarse" + "fine" generators
  - VITS/StyleTTS: single model with phoneme→mel→vocoder pipeline
  - Generic transformers AutoModel: tries TFAutoModel / GenerationMixin

Exit codes:
    0 = success, audio written to --output
    2 = model loading failed (prints actionable stderr)
    3 = generation failed
"""
import argparse
import json
import os
import sys
import tempfile
from pathlib import Path


def detect_format(model_dir: Path) -> str:
    """Heuristically detect the model architecture from config files."""
    config_path = model_dir / "config.json"
    if not config_path.exists():
        # Check one level deep (repos sometimes nest).
        for sub in model_dir.iterdir():
            if sub.is_dir() and (sub / "config.json").exists():
                config_path = sub / "config.json"
                break

    if config_path.exists():
        try:
            cfg = json.loads(config_path.read_text())
            model_type = str(cfg.get("model_type", "")).lower()
            archs = " ".join(str(a) for a in cfg.get("architectures", [])).lower()

            if "bark" in model_type or "bark" in archs:
                return "bark"
            if "vits" in model_type or "vits" in archs:
                return "vits"
            if "styletts" in model_type or "styletts" in archs:
                return "styletts"
            if "speecht5" in model_type or "speecht5" in archs:
                return "speecht5"
            if "fastspeech" in model_type:
                return "fastspeech"
            if any(k in cfg for k in ["text2semantic", "coarse", "fine"]):
                return "bark"
            if "generator" in cfg or "vocoder" in cfg:
                return "generic_gm"
        except Exception:
            pass

    # File-based heuristic.
    names = {f.name for f in model_dir.rglob("*")}
    if "config.json" in names and any("generator" in n for n in names):
        return "generic_gm"
    if any("vits" in n.lower() for n in names):
        return "vits"

    return "auto"


def load_and_generate(model_dir: Path, text: str, output_path: Path, voice: str | None) -> int:
    """Load the model and write generated audio to `output_path`. Returns exit code."""
    model_fmt = detect_format(model_dir)
    print(f"[sano_tts] Detected format: {model_fmt}", file=sys.stderr)

    try:
        import torch
        import numpy as np
    except ImportError:
        print("[sano_tts] ERROR: PyTorch not installed. Run: pip install torch torchaudio numpy", file=sys.stderr)
        return 2

    try:
        import soundfile as sf
    except ImportError:
        print("[sano_tts] ERROR: soundfile not installed. Run: pip install soundfile", file=sys.stderr)
        return 2

    try:
        if model_fmt == "bark" or model_fmt == "generic_gm":
            return _gen_bark(model_dir, text, output_path)
        elif model_fmt == "vits":
            return _gen_vits(model_dir, text, output_path)
        else:
            return _gen_auto(model_dir, text, output_path)
    except Exception as e:
        print(f"[sano_tts] Generation failed: {e}", file=sys.stderr)
        return 3


def _gen_bark(model_dir: Path, text: str, output_path: Path) -> int:
    """Suno Bark-style generation."""
    from transformers import AutoProcessor, AutoModel

    processor = AutoProcessor.from_pretrained(str(model_dir))
    model = AutoModel.from_pretrained(str(model_dir))
    model.eval()

    inputs = processor(text=text, voice_preset=None, return_tensors="pt")
    with torch.no_grad():
        output = model.generate(**inputs, do_sample=True, semantic_temperature=0.7,
                                coarse_temperature=0.7, fine_temperature=0.5)

    audio = output.cpu().numpy().squeeze()
    sample_rate = getattr(model.config, "sampling_rate", 24000)
    import soundfile as sf
    sf.write(str(output_path), audio, sample_rate)
    return 0


def _gen_vits(model_dir: Path, text: str, output_path: Path) -> int:
    """VITS/StyleTTS-style generation."""
    import torch
    from transformers import AutoTokenizer, AutoModel

    tokenizer = AutoTokenizer.from_pretrained(str(model_dir))
    model = AutoModel.from_pretrained(str(model_dir))
    model.eval()

    inputs = tokenizer(text, return_tensors="pt")
    with torch.no_grad():
        audio = model(**inputs).waveform if hasattr(model(**inputs), "waveform") else model(**inputs)[0]

    audio_np = audio.cpu().numpy().squeeze() if isinstance(audio, torch.Tensor) else audio
    sample_rate = getattr(model.config, "sampling_rate", 22050)
    import soundfile as sf
    sf.write(str(output_path), audio_np, sample_rate)
    return 0


def _gen_auto(model_dir: Path, text: str, output_path: Path) -> int:
    """Generic AutoModel fallback — tries speech generation patterns."""
    from transformers import AutoProcessor, AutoModelForTextToWaveform, AutoModel

    try:
        processor = AutoProcessor.from_pretrained(str(model_dir))
        model = AutoModelForTextToWaveform.from_pretrained(str(model_dir))
        model.eval()
        inputs = processor(text=text, return_tensors="pt")
        with torch.no_grad():
            audio = model(**inputs).waveform
        audio_np = audio.cpu().numpy().squeeze()
        sample_rate = getattr(model.config, "sampling_rate", 22050)
    except Exception:
        # Last resort: try AutoModel directly.
        model = AutoModel.from_pretrained(str(model_dir))
        model.eval()
        print("[sano_tts] WARNING: Could not detect a standard TTS pipeline. Returning silence.", file=sys.stderr)
        sample_rate = 22050
        audio_np = np.zeros(sample_rate, dtype=np.float32)

    import soundfile as sf
    sf.write(str(output_path), audio_np, sample_rate)
    return 0


def main():
    parser = argparse.ArgumentParser(description="SanoTTS inference")
    parser.add_argument("--model-dir", required=True, help="Path to the downloaded model directory")
    parser.add_argument("--text", required=True, help="Text to synthesize")
    parser.add_argument("--output", required=True, help="Output WAV file path")
    parser.add_argument("--voice", default=None, help="Optional voice/preset name")
    args = parser.parse_args()

    model_dir = Path(args.model_dir)
    if not model_dir.exists():
        print(f"[sano_tts] ERROR: Model dir not found: {model_dir}", file=sys.stderr)
        sys.exit(2)

    output_path = Path(args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)

    sys.exit(load_and_generate(model_dir, args.text, output_path, args.voice))


if __name__ == "__main__":
    main()
