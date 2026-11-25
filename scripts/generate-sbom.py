#!/usr/bin/env python3
"""Generate a dependency SBOM for the Rust and frontend lockfiles.

This is deliberately dependency-free: release runners already have Python, and
adding another SBOM tool to every build would make the release path more
fragile than the artifact it is documenting. The output is CycloneDX 1.5 with
one component per locked Cargo or npm package.
"""

from __future__ import annotations

import datetime as _dt
import json
import re
import sys
import urllib.parse
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CARGO_LOCK = ROOT / "Cargo.lock"
NPM_LOCK = ROOT / "package-lock.json"


def cargo_components() -> list[dict]:
    if not CARGO_LOCK.exists():
        return []
    text = CARGO_LOCK.read_text(encoding="utf-8")
    components = []
    for block in text.split("[[package]]")[1:]:
        name = re.search(r'^name = "([^"]+)"', block, re.MULTILINE)
        version = re.search(r'^version = "([^"]+)"', block, re.MULTILINE)
        if not name or not version:
            continue
        components.append(
            {
                "type": "library",
                "name": name.group(1),
                "version": version.group(1),
                "purl": f"pkg:cargo/{urllib.parse.quote(name.group(1), safe='')}@{version.group(1)}",
            }
        )
    return components


def npm_components() -> list[dict]:
    if not NPM_LOCK.exists():
        return []
    try:
        data = json.loads(NPM_LOCK.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return []
    packages = data.get("packages") or {}
    components = []
    for path, meta in packages.items():
        if not path or not isinstance(meta, dict):
            continue
        name = meta.get("name") or path.rsplit("node_modules/", 1)[-1]
        version = meta.get("version")
        if not name or not version:
            continue
        components.append(
            {
                "type": "library",
                "name": name,
                "version": version,
                "purl": f"pkg:npm/{urllib.parse.quote(name, safe='')}@{version}",
            }
        )
    return components


def main() -> int:
    output = Path(sys.argv[1] if len(sys.argv) > 1 else "sbom.cdx.json")
    seen = set()
    components = []
    for component in cargo_components() + npm_components():
        key = component["purl"]
        if key in seen:
            continue
        seen.add(key)
        components.append(component)
    components.sort(key=lambda item: (item["name"], item["version"], item["purl"]))
    bom = {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "version": 1,
        "metadata": {
            "timestamp": _dt.datetime.now(_dt.timezone.utc).isoformat(),
            "tools": [
                {
                    "vendor": "RAVENBOT",
                    "name": "generate-sbom.py",
                    "version": "1",
                }
            ],
            "component": {
                "type": "application",
                "name": "RAVENBOT",
                "version": "0.2.0",
            },
        },
        "components": components,
    }
    output.write_text(json.dumps(bom, indent=2) + "\n", encoding="utf-8")
    print(f"{output}: {len(components)} components")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
