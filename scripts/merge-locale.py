#!/usr/bin/env python3
"""Merge a translated JSON fragment into one locale dictionary.

The fragment must have the same shape as the English subtree it replaces: every
key must already exist, every leaf must be a non-empty string, and no key may be
added or removed. This keeps a long translation pass from silently drifting away
from the English dictionary.

Usage:
    python3 scripts/merge-locale.py <code> <fragment.json>
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LOCALES = ROOT / "src" / "lib" / "i18n" / "locales"


def merge(target: dict, fragment: dict, path: tuple[str, ...] = ()) -> None:
    for key, value in fragment.items():
        name = ".".join(path + (key,))
        if key not in target:
            raise SystemExit(f"unknown key: {name}")
        current = target[key]
        if isinstance(value, dict):
            if not isinstance(current, dict):
                raise SystemExit(f"expected a string at {name}")
            merge(current, value, path + (key,))
        elif isinstance(value, str):
            if not isinstance(current, str):
                raise SystemExit(f"expected an object at {name}")
            if not value.strip():
                raise SystemExit(f"empty translation at {name}")
            target[key] = value
        else:
            raise SystemExit(f"unexpected {type(value).__name__} at {name}")


def main() -> int:
    code = sys.argv[1]
    fragment = json.loads(Path(sys.argv[2]).read_text(encoding="utf-8"))
    path = LOCALES / f"{code}.json"
    dictionary = json.loads(path.read_text(encoding="utf-8"))
    merge(dictionary, fragment)
    path.write_text(
        json.dumps(dictionary, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(f"merged {sys.argv[2]} into {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
