"""Read dependency versions from the repository lockfile."""

from pathlib import Path
import tomllib
from typing import cast


def locked_package_version(root: Path, name: str) -> str:
    lock = cast(
        dict[str, object], tomllib.loads((root / "uv.lock").read_text(encoding="utf-8"))
    )
    packages = lock.get("package")
    if not isinstance(packages, list):
        raise RuntimeError("uv.lock does not contain a package list")
    for package in cast(list[object], packages):
        if isinstance(package, dict):
            entry = cast(dict[str, object], package)
            version = entry.get("version")
            if entry.get("name") == name and isinstance(version, str):
                return version
    raise RuntimeError(f"uv.lock has no locked version for {name}")
