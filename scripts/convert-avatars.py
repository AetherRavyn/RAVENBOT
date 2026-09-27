#!/usr/bin/env python3
"""
Swap the remaining `<img src={... getDiceBearUrl ...}>` avatar displays for
<RavenAvatar>, which is generated locally and reacts to the work.

Left alone on purpose:
  - AvatarPicker: a chooser for the user's own image, so previews should be
    real remote images.
  - Office and user avatars where `avatar_url` is a custom upload: those keep
    the image and only fall back to a generated face.

The patterns are mechanical, so they are matched rather than hand-edited; every
substitution is checked for a matching replacement so a file is never silently
half-converted.
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "src"

SKIP = {"AvatarPicker.svelte"}

# <img\n  src={EXPR}\n  alt="..."\n  class="..."\n/>  (attributes in any order)
IMG_RE = re.compile(
    r"<img\s+(?P<attrs>(?:[^<>]|\{[^{}]*(?:\{[^{}]*\}[^{}]*)*\})*?)/>",
    re.S,
)

# A src whose expression mentions getDiceBearUrl.
def is_dicebear(attrs: str) -> bool:
    return "getDiceBearUrl" in attrs


def attr(attrs: str, name: str) -> str | None:
    m = re.search(rf'\b{name}=(?:"([^"]*)"|\{{([^}}]*)\}})', attrs)
    if not m:
        return None
    return m.group(1) if m.group(1) is not None else m.group(2).strip()


def set_class(attrs: str, value: str) -> str:
    if re.search(r'\bclass=', attrs):
        return re.sub(r'\bclass=(?:"[^"]*"|\{[^}]*\})', f'class="{value}"', attrs, count=1)
    return attrs + f' class="{value}"'


def convert(path: Path) -> tuple[int, int]:
    text = path.read_text()
    converted = replaced = 0

    def repl(m: re.Match) -> str:
        nonlocal converted, replaced
        attrs = m.group("attrs")
        if not is_dicebear(attrs):
            return m.group(0)

        alt = attr(attrs, "alt")
        cls = attr(attrs, "class") or ""
        # The name is the seed for the generated face. Find the expression that
        # feeds getDiceBearUrl and reuse its first argument if it is a plain
        # identifier, otherwise fall back to the alt text.
        name = None
        src_expr = attr(attrs, "src") or ""
        dm = re.search(r"getDiceBearUrl\(\s*([^,()]+?)\s*(?:,|\))", src_expr)
        if dm:
            cand = dm.group(1).strip()
            if re.fullmatch(r"[\w.\[\]'\"]+", cand) and "getDiceBearUrl" not in cand:
                name = cand

        if name is None:
            if not alt or alt == "Agent" or alt == "office":
                return m.group(0)  # nothing identifiable to derive a name from
            name = alt

        converted += 1

        # Keep the existing avatar_url as a custom image when the source had one.
        custom = None
        cm = re.search(r"([\w.?]+)\.avatar_url", src_expr)
        if cm and "||" not in src_expr:
            custom = f"{cm.group(1)}.avatar_url"
        elif "||" in src_expr:
            am = re.match(r"\s*([\w.?]+)\.avatar_url\s*\|\|", src_expr)
            if am:
                custom = f"{am.group(1)}.avatar_url"

        if custom:
            image = f'imageUrl={{{custom}}}'
        else:
            image = ""

        # `decorative` because these sit next to a name; the alt text was
        # almost always the name, which the DOM already carries.
        label = "decorative" if alt in (None, "", "Agent", "office", "You") else ""
        alt_attr = " alt=\"\"" if alt not in (None, "") else ""

        new_attrs = set_class(attrs, cls)
        parts = [f"name={{{name}}}"]
        if image:
            parts.append(image)
        if label:
            parts.append(label)
        props = " ".join(parts)
        replaced += 1
        return f"<RavenAvatar {props}{alt_attr} />"

    new = IMG_RE.sub(repl, text)
    if new != text:
        # Make sure the component is imported.
        if "<RavenAvatar" in new and 'from "$lib/components/RavenAvatar.svelte"' not in new:
            anchor = re.search(r'^(\s*import .*?;\s*)$', new, re.M)
            if anchor:
                indent = re.match(r'\s*', anchor.group(1)).group(0)
                new = (
                    new[: anchor.start(1)]
                    + f'{indent}import RavenAvatar from "$lib/components/RavenAvatar.svelte";\n'
                    + new[anchor.start(1) :]
                )
            else:
                print(f"  !! {path.name}: converted but could not find an import to anchor to")
        path.write_text(new)
    return converted, replaced


def main() -> int:
    total_c = total_r = 0
    for path in sorted(SRC.rglob("*.svelte")):
        if path.name in SKIP:
            print(f"skip {path.name}")
            continue
        c, r = convert(path)
        if c:
            total_c += c
            total_r += r
            print(f"{path.name}: {c} avatars")
            if c != r:
                print(f"  !! {c} matched but only {r} replaced — inspect this file")
                return 1
    print(f"\ntotal: {total_c} converted")
    return 0


if __name__ == "__main__":
    sys.exit(main())
