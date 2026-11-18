"""Select artifacts for host-specific release smoke checks."""

from pathlib import Path

from packaging.tags import sys_tags
from packaging.utils import parse_sdist_filename, parse_wheel_filename
from packaging.version import Version


def require_one(paths: list[Path], label: str) -> Path:
    if len(paths) != 1:
        raise ValueError(f"Expected one {label}, found {len(paths)}")
    return paths[0]


def release_inputs(directory: Path, version: str) -> tuple[Path, Path, Path, Path]:
    directory = directory.resolve()
    helper_wheel = require_one(
        [path for path in directory.glob("codepact-*.whl")], "helper wheel"
    )
    if parse_wheel_filename(helper_wheel.name)[1] != Version(version):
        raise ValueError("Helper wheel version does not match the release")

    supported = set(sys_tags())
    native_wheels: list[Path] = []
    for wheel in directory.glob("codepact_cli-*.whl"):
        _, wheel_version, _, tags = parse_wheel_filename(wheel.name)
        if wheel_version == Version(version) and tags & supported:
            native_wheels.append(wheel)
    cli_wheel = require_one(native_wheels, "host-compatible CLI wheel")

    sources = sorted(directory.glob("*.tar.gz"))
    by_name: dict[str, Path] = {}
    for source in sources:
        name, source_version = parse_sdist_filename(source.name)
        if source_version == Version(version):
            by_name[name] = source
    if set(by_name) != {"codepact", "codepact-cli"}:
        raise ValueError("Expected matching helper and CLI source archives")
    return helper_wheel, cli_wheel, by_name["codepact"], by_name["codepact-cli"]
