from typing import Annotated
from codepact.contracts import SharedBorrow
from archive import Archive, build_manifest

def early_return():
    archive = Archive()
    return 1

def inspect_finished_archive():
    archive = Archive()
    alias = archive
    alias.close()
    return archive.inspect()

def close_borrowed_archive(archive: Annotated[Archive, SharedBorrow()]):
    archive.close()

def ignore_manifest():
    builder = build_manifest
    builder()

def valid_manifest():
    manifest = build_manifest()
    return manifest
