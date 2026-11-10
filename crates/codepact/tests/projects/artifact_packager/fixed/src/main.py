from typing import Annotated
from codepact.contracts import SharedBorrow
from archive import Archive, build_manifest

def early_return():
    archive = Archive()
    try:
        return 1
    finally:
        archive.close()

def inspect_finished_archive():
    archive = Archive()
    alias = archive
    manifest = archive.inspect()
    alias.close()
    return manifest

def close_borrowed_archive(archive: Annotated[Archive, SharedBorrow()]):
    return archive.inspect()

def ignore_manifest():
    builder = build_manifest
    return builder()

def valid_manifest():
    manifest = build_manifest()
    return manifest
