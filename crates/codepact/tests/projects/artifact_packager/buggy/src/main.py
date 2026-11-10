from typing import Annotated
from codepact.contracts import SharedBorrow
from archive import Archive, build_manifest


def early_return() -> int:
    archive = Archive()
    archive.inspect()
    return 1


def inspect_finished_archive() -> str:
    archive = Archive()
    alias = archive
    alias.close()
    return archive.inspect()


def close_borrowed_archive(archive: Annotated[Archive, SharedBorrow()]) -> None:
    archive.close()


def ignore_manifest() -> None:
    builder = build_manifest
    builder()


def valid_manifest() -> str:
    manifest = build_manifest()
    return manifest
