"""Check repository documentation for broken relative file links."""

from __future__ import annotations

import re
import sys
from pathlib import Path
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
LINK = re.compile(r"(?<!!)\[[^\]]*\]\(([^)]+)\)")


def markdown_without_fences(text: str) -> str:
    """Remove fenced code blocks so example syntax is not treated as links."""
    kept: list[str] = []
    fence_char: str | None = None
    fence_width = 0
    for line in text.splitlines(keepends=True):
        marker = re.match(r"^\s*(`{3,}|~{3,})", line)
        if fence_char is None and marker is not None:
            fence_char = marker.group(1)[0]
            fence_width = len(marker.group(1))
            continue
        if fence_char is not None:
            if re.match(rf"^\s*{re.escape(fence_char)}{{{fence_width},}}\s*$", line):
                fence_char = None
                fence_width = 0
            continue
        kept.append(line)
    return "".join(kept)


def documentation_files() -> list[Path]:
    paths = [
        ROOT / "README.md",
        ROOT / "CONTRIBUTING.md",
        ROOT / "ROADMAP.md",
        ROOT / "cli/README.md",
    ]
    paths.extend((ROOT / "docs").rglob("*.md"))
    paths.extend((ROOT / "docs").rglob("*.rst"))
    return sorted(path for path in paths if path.is_file())


def relative_target(source: Path, destination: str) -> Path | None:
    destination = destination.strip()
    if destination.startswith("<") and ">" in destination:
        destination = destination[1 : destination.index(">")]
    else:
        destination = destination.split(maxsplit=1)[0]
    parsed = urlsplit(destination)
    if parsed.scheme or parsed.netloc or not parsed.path:
        return None
    return (source.parent / unquote(parsed.path)).resolve()


def main() -> int:
    missing: list[str] = []
    files = documentation_files()
    for source in files:
        text = source.read_text(encoding="utf-8")
        if source.suffix == ".md":
            text = markdown_without_fences(text)
        for match in LINK.finditer(text):
            target = relative_target(source, match.group(1))
            if target is not None and not target.exists():
                local = source.relative_to(ROOT)
                missing.append(f"{local}: missing link target {match.group(1)}")
    if missing:
        print("Broken relative documentation links:", file=sys.stderr)
        print("\n".join(f"- {item}" for item in missing), file=sys.stderr)
        return 1
    print(f"Checked relative links in {len(files)} documentation files.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
