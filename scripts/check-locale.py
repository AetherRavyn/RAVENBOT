#!/usr/bin/env python3
"""Validate one translated locale against the English dictionary.

Checks keys, key order, empty values, placeholders and leftover translation
tokens, then reports values that are identical to English so a reviewer can
decide whether a brand name or an untranslated string is the reason.

Usage:
    python3 scripts/check-locale.py pt
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LOCALES = ROOT / "src" / "lib" / "i18n" / "locales"
PLACEHOLDER = re.compile(r"\{\{[^{}]+\}\}")
SENTINEL = re.compile(r"ZXQ[A-Z]+\d+QXZ")
ENGLISH = re.compile(r"\b(the|and|with|your|from|this|that|will|have|not|for|you)\b", re.I)


def flatten(value: object, prefix: tuple[str, ...] = ()) -> list[tuple[tuple[str, ...], str]]:
    rows: list[tuple[tuple[str, ...], str]] = []
    if isinstance(value, dict):
        for key, child in value.items():
            rows.extend(flatten(child, prefix + (key,)))
    elif isinstance(value, str):
        rows.append((prefix, value))
    else:
        raise TypeError(f"unexpected {type(value).__name__} at {'.'.join(prefix)}")
    return rows


def main() -> int:
    code = sys.argv[1]
    english = flatten(json.loads((LOCALES / "en.json").read_text(encoding="utf-8")))
    target = flatten(json.loads((LOCALES / f"{code}.json").read_text(encoding="utf-8")))
    problems: list[str] = []

    english_paths = [path for path, _ in english]
    target_paths = [path for path, _ in target]
    if target_paths != english_paths:
        missing = [p for p in english_paths if p not in set(target_paths)]
        extra = [p for p in target_paths if p not in set(english_paths)]
        for path in missing[:20]:
            problems.append(f"missing: {'.'.join(path)}")
        for path in extra[:20]:
            problems.append(f"extra: {'.'.join(path)}")

    target_by_path = dict(target)
    identical: list[str] = []
    suspicious: list[str] = []
    for path, source in english:
        value = target_by_path.get(path)
        if value is None:
            continue
        name = ".".join(path)
        if not value.strip():
            problems.append(f"empty: {name}")
        if SENTINEL.search(value) or "ZXQ" in value:
            problems.append(f"sentinel: {name}")
        if sorted(PLACEHOLDER.findall(source)) != sorted(PLACEHOLDER.findall(value)):
            problems.append(f"placeholder: {name}")
        if value == source:
            identical.append(name)
        elif len(source) > 12 and ENGLISH.search(value):
            suspicious.append(f"{name}: {value!r}")

    print(f"{code}: {len(target)} keys, {len(problems)} problems, {len(identical)} identical")
    for problem in problems[:40]:
        print(f"  ! {problem}")
    if identical:
        print("  identical (review): " + ", ".join(identical[:40]))
    if suspicious:
        print("  English words remain (review):")
        for item in suspicious[:20]:
            print(f"    {item}")
    return 1 if problems else 0


if __name__ == "__main__":
    raise SystemExit(main())
