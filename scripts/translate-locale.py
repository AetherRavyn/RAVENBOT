#!/usr/bin/env python3
"""Translate the English dictionary into new locales with Google Translate.

Development tool, not a runtime dependency. It protects placeholders, HTML
tags, URLs, environment variables and inline code with opaque tokens, translates
in batches, and validates that the result still has every English key, the same
placeholder names and no empty strings.

Usage:
    python3 scripts/translate-locale.py pt it ko ru hi tr
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import time
from pathlib import Path

import requests

ROOT = Path(__file__).resolve().parent.parent
LOCALES = ROOT / "src" / "lib" / "i18n" / "locales"
SOURCE = LOCALES / "en.json"
ENDPOINT = "https://translate.googleapis.com/translate_a/t"

SENTINEL = re.compile(r"ZXQ(?:PH|HT|URL|ENV|CODE|TERM)\d+QXZ")
PLACEHOLDER = re.compile(r"\{\{[^{}]+\}\}")
HTML = re.compile(r"<[^>]+>")
URL = re.compile(r"https?://[^\s)>\]]+")
ENV = re.compile(r"\$\{[^}]+\}")
CODE = re.compile(r"`[^`]+`")

# Product and protocol names are names, not words to translate. Google will
# happily turn "Slack" into "Folga" and "Discord" into "Discórdia" if left
# alone; a token keeps them intact in every locale.
TERMS = [
    "OpenCode Go",
    "Microsoft Teams",
    "GitHub Actions",
    "RAVENBOT",
    "OpenRouter",
    "Anthropic",
    "OpenAI",
    "OpenCode",
    "Ollama",
    "LM Studio",
    "GitHub",
    "Docker",
    "Linux",
    "macOS",
    "Windows",
    "Telegram",
    "Slack",
    "Discord",
    "WhatsApp",
    "Tailscale",
    "Cloudflare",
    "SQLite",
    "Markdown",
    "JavaScript",
    "TypeScript",
    "Python",
    "systemd",
    "launchd",
    "cargo",
    "MCP",
    "API",
    "JSON",
    "URL",
    "HTTP",
    "HTTPS",
    "CSV",
    "PDF",
    "HTML",
    "CSS",
    "OAuth",
    "npm",
    "Rust",
]
TERM_PATTERN = re.compile(
    r"(?<![A-Za-z0-9])("
    + "|".join(re.escape(term) for term in sorted(TERMS, key=len, reverse=True))
    + r")(?![A-Za-z0-9])"
)


# Keys are paths of literal JSON keys, not dot-joined strings: the English
# dictionary already contains literal keys such as studio["state.idle"], and a
# split-on-dot representation would silently turn them into nested objects.
PathTuple = tuple[str, ...]


def flatten(value: object, prefix: PathTuple = ()) -> list[tuple[PathTuple, str]]:
    rows: list[tuple[PathTuple, str]] = []
    if isinstance(value, dict):
        for key, child in value.items():
            rows.extend(flatten(child, prefix + (key,)))
    elif isinstance(value, str):
        rows.append((prefix, value))
    else:
        raise TypeError(f"unexpected {type(value).__name__} at {'.'.join(prefix)}")
    return rows


def unflatten(rows: list[tuple[PathTuple, str]]) -> dict:
    root: dict = {}
    for path, value in rows:
        cursor = root
        for part in path[:-1]:
            cursor = cursor.setdefault(part, {})
        cursor[path[-1]] = value
    return root


def label(path: PathTuple) -> str:
    return ".".join(path)


def protect(text: str, store: dict[str, str]) -> str:
    def token(kind: str, original: str) -> str:
        key = f"ZXQ{kind}{len(store)}QXZ"
        store[key] = original
        return key

    text = PLACEHOLDER.sub(lambda m: token("PH", m.group(0)), text)
    text = CODE.sub(lambda m: token("CODE", m.group(0)), text)
    text = HTML.sub(lambda m: token("HT", m.group(0)), text)
    text = URL.sub(lambda m: token("URL", m.group(0)), text)
    text = ENV.sub(lambda m: token("ENV", m.group(0)), text)
    text = TERM_PATTERN.sub(lambda m: token("TERM", m.group(0)), text)
    return text


def restore(text: str, store: dict[str, str]) -> str:
    # Tokens nest: an inline-code span can contain a placeholder token, so the
    # later (outer) token must be restored before the earlier one. Reversing the
    # insertion order also makes this a single pass.
    for key, original in reversed(list(store.items())):
        text = text.replace(key, original)
    return text


def request_batch(lines: list[str], target: str, session: requests.Session) -> list[str] | None:
    if not lines:
        return []
    payload = [("q", line) for line in lines]
    for attempt in range(5):
        try:
            response = session.post(
                ENDPOINT,
                params={"client": "gtx", "sl": "en", "tl": target},
                data=payload,
                timeout=90,
                headers={"User-Agent": "RAVENBOT-locale-tool/1.0"},
            )
        except requests.RequestException as error:
            print(f"  ! request failed ({error}); retrying", file=sys.stderr)
            time.sleep(2**attempt)
            continue
        if response.status_code == 429 or response.status_code >= 500:
            time.sleep(2**attempt)
            continue
        if response.status_code != 200:
            print(f"  ! HTTP {response.status_code}; retrying", file=sys.stderr)
            time.sleep(2**attempt)
            continue
        try:
            data = response.json()
        except ValueError:
            return None
        if not isinstance(data, list) or len(data) != len(lines):
            # A single q sometimes returns [[translation, ...]]; a multi-q
            # request is a flat list. Normalise only after checking the count.
            return None
        out: list[str] = []
        for item in data:
            if isinstance(item, str):
                out.append(item)
            elif isinstance(item, list) and item and isinstance(item[0], str):
                out.append(item[0])
            else:
                return None
        return out
    return None


def translate_lines(
    lines: list[str], target: str, batch_size: int, session: requests.Session, label: str
) -> list[str]:
    result: list[str] = []
    for start in range(0, len(lines), batch_size):
        batch = lines[start : start + batch_size]
        translated = request_batch(batch, target, session)
        if translated is None and len(batch) > 1:
            middle = len(batch) // 2
            translated = translate_lines(
                batch[:middle], target, batch_size, session, label
            ) + translate_lines(batch[middle:], target, batch_size, session, label)
        elif translated is None:
            print(f"  ! {label}: keeping English for {batch[0]!r}", file=sys.stderr)
            translated = batch
        result.extend(translated)
        time.sleep(0.2)
    return result


def translate_locale(code: str, batch_size: int, rows: list[tuple[str, str]]) -> int:
    protected: list[str] = []
    stores: list[dict[str, str]] = []
    for _path, value in rows:
        store: dict[str, str] = {}
        protected.append(protect(value, store))
        stores.append(store)

    session = requests.Session()
    translated = translate_lines(protected, code, batch_size, session, code)

    restored: list[tuple[str, str]] = []
    problems: list[str] = []
    for (path, source), text, store in zip(rows, translated, stores):
        value = restore(text, store).strip()
        if not value:
            problems.append(f"{label(path)}: empty translation, kept English")
            value = source
        if SENTINEL.search(value):
            problems.append(f"{label(path)}: unresolved sentinel in {value!r}")
        source_placeholders = sorted(PLACEHOLDER.findall(source))
        value_placeholders = sorted(PLACEHOLDER.findall(value))
        if source_placeholders != value_placeholders:
            problems.append(
                f"{label(path)}: placeholders {source_placeholders} became {value_placeholders}"
            )
        restored.append((path, value))

    output = LOCALES / f"{code}.json"
    output.write_text(
        json.dumps(unflatten(restored), ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )

    # Validate the shape against English rather than trusting the writer.
    again = flatten(json.loads(output.read_text(encoding="utf-8")))
    if [path for path, _ in again] != [path for path, _ in rows]:
        print(f"{code}: key order or coverage changed", file=sys.stderr)
        return 2
    english_by_path = dict(rows)
    same = 0
    for path, value in again:
        if not value:
            problems.append(f"{label(path)}: empty after round-trip")
        if sorted(PLACEHOLDER.findall(value)) != sorted(PLACEHOLDER.findall(english_by_path[path])):
            problems.append(f"{label(path)}: placeholder drift after round-trip")
        if value == english_by_path[path]:
            same += 1

    print(f"{code}: {len(again)} keys -> {output} ({same} identical to English)")
    for problem in problems:
        print(f"  ! {problem}", file=sys.stderr)
    return 1 if problems else 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("codes", nargs="+")
    parser.add_argument("--batch", type=int, default=100)
    args = parser.parse_args()

    english = json.loads(SOURCE.read_text(encoding="utf-8"))
    rows = flatten(english)
    status = 0
    for code in args.codes:
        status |= translate_locale(code, args.batch, rows)
    return status


if __name__ == "__main__":
    raise SystemExit(main())
