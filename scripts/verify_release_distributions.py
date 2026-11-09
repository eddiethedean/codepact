"""Verify the pure helper and complete native CLI release inventory."""

import argparse
from email.parser import BytesParser
from pathlib import Path
import tarfile
import zipfile

from packaging.utils import parse_sdist_filename, parse_wheel_filename
from packaging.version import Version

PLATFORMS = {
    "manylinux_2_17_x86_64",
    "manylinux_2_28_aarch64",
    "musllinux_1_2_x86_64",
    "musllinux_1_2_aarch64",
    "macosx_10_13_x86_64",
    "macosx_11_0_arm64",
    "win_amd64",
    "win_arm64",
}


def verify(directory: Path, version: str) -> None:
    root = Path(__file__).resolve().parents[1]
    license_text = (root / "LICENSE").read_bytes()
    if (root / "cli/LICENSE").read_bytes() != license_text:
        raise ValueError("CLI license file must match the repository license")
    wheels = sorted(directory.glob("*.whl"))
    sources = sorted(directory.glob("*.tar.gz"))
    if len(wheels) != 9 or len(sources) != 2 or len(list(directory.iterdir())) != 11:
        raise ValueError(
            "Expected eight CLI wheels, one pure helper wheel and two source archives"
        )
    found: set[str] = set()
    pure = 0
    for wheel in wheels:
        name, artifact_version, _, tags = parse_wheel_filename(wheel.name)
        if name not in {"codepact", "codepact-cli"} or artifact_version != Version(
            version
        ):
            raise ValueError(f"Wrong wheel name/version: {wheel.name}")
        if name == "codepact":
            pure += 1
            if not wheel.name.endswith("-py3-none-any.whl"):
                raise ValueError("codepact must remain pure Python")
            with zipfile.ZipFile(wheel) as archive:
                names = archive.namelist()
                for module in ("__init__.py", "contracts.py", "rust.py", "py.typed"):
                    if f"codepact/{module}" not in names:
                        raise ValueError(f"Missing helper: {module}")
                metadata_file = next(
                    name for name in names if name.endswith(".dist-info/METADATA")
                )
                metadata = BytesParser().parsebytes(archive.read(metadata_file))
                license_files = [
                    name
                    for name in names
                    if name.endswith(".dist-info/licenses/LICENSE")
                ]
                if (
                    len(license_files) != 1
                    or archive.read(license_files[0]) != license_text
                    or "LICENSE" not in metadata.get_all("License-File", [])
                ):
                    raise ValueError("Helper wheel must include the MIT license text")
                if f"codepact-cli=={version}" not in metadata.get_all(
                    "Requires-Dist", []
                ):
                    raise ValueError(
                        "Helper wheel must require its matching CLI version"
                    )
                if (
                    metadata["Version"] != version
                    or metadata["Requires-Python"] != ">=3.11"
                ):
                    raise ValueError("Invalid helper metadata")
                if any(".data/scripts/" in name for name in names):
                    raise ValueError("Helper wheel must not bundle the executable")
            continue
        platforms = {tag.platform for tag in tags} & PLATFORMS
        if len(platforms) != 1 or platforms & found:
            raise ValueError(f"Unexpected or duplicate platform: {wheel.name}")
        if any(tag.interpreter != "py3" or tag.abi != "none" for tag in tags):
            raise ValueError(f"Expected Python-independent binary wheel: {wheel.name}")
        found.update(platforms)
        with zipfile.ZipFile(wheel) as archive:
            names = archive.namelist()
            executable = "codepact.exe" if "win_" in wheel.name else "codepact"
            if not any(name.endswith(f".data/scripts/{executable}") for name in names):
                raise ValueError(f"Missing Rust CLI: {wheel.name}")
            metadata_file = next(
                name for name in names if name.endswith(".dist-info/METADATA")
            )
            metadata = BytesParser().parsebytes(archive.read(metadata_file))
            license_files = [
                name for name in names if name.endswith(".dist-info/licenses/LICENSE")
            ]
            if (
                len(license_files) != 1
                or archive.read(license_files[0]) != license_text
                or "LICENSE" not in metadata.get_all("License-File", [])
            ):
                raise ValueError(
                    f"CLI wheel must include the MIT license text: {wheel.name}"
                )
            if (
                metadata["Version"] != version
                or metadata["Requires-Python"] != ">=3.11"
            ):
                raise ValueError(f"Invalid wheel metadata: {wheel.name}")
    if pure != 1:
        raise ValueError("Expected one pure codepact wheel")
    if found != PLATFORMS:
        raise ValueError(f"Missing platforms: {PLATFORMS - found}")
    source_names: set[str] = set()
    for source in sources:
        name, artifact_version = parse_sdist_filename(source.name)
        if (
            name not in {"codepact", "codepact-cli"}
            or name in source_names
            or artifact_version != Version(version)
        ):
            raise ValueError("Source archive name/version mismatch")
        source_names.add(name)
        required = (
            (
                "/Cargo.lock",
                "/Cargo.toml",
                "/pyproject.toml",
                "/crates/codepact/src/main.rs",
                "/crates/codepact/LICENSE",
                "/LICENSE",
            )
            if name == "codepact-cli"
            else ("/pyproject.toml", "/python/codepact/py.typed", "/LICENSE")
        )
        with tarfile.open(source) as archive:
            names = archive.getnames()
            for suffix in required:
                if not any(name.endswith(suffix) for name in names):
                    raise ValueError(f"Source archive missing {suffix}")
            license_members = [name for name in names if name.endswith("/LICENSE")]
            matching_license = False
            for member_name in license_members:
                member = archive.extractfile(member_name)
                if member is not None and member.read() == license_text:
                    matching_license = True
                    break
            if not matching_license:
                raise ValueError(
                    f"Source archive lacks the repository MIT license: {source.name}"
                )
    print(
        f"Verified eight CLI wheels, one pure wheel and two source archives: {version}"
    )


class Arguments(argparse.Namespace):
    directory: Path = Path(".")
    version: str = ""


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--version", required=True)
    arguments = Arguments()
    _ = parser.parse_args(namespace=arguments)
    verify(arguments.directory, arguments.version)
