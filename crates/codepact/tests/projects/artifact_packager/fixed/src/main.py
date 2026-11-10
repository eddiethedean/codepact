from typing import Annotated
from codepact.contracts import SharedBorrow
from archive import Archive, build_manifest


def early_return() -> int:
    archive = Archive()
    try:
        return 1
    finally:
        archive.close()


def inspect_finished_archive() -> str:
    archive = Archive()
    alias = archive
    manifest = archive.inspect()
    alias.close()
    return manifest


def close_borrowed_archive(archive: Annotated[Archive, SharedBorrow()]) -> str:
    return archive.inspect()


def ignore_manifest() -> str:
    builder = build_manifest
    return builder()


def valid_manifest() -> str:
    manifest = build_manifest()
    return manifest
