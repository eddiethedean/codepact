from codepact.rust import Owned
from writer import Writer, report_checksum


def interrupted_export() -> None:
    writer = Writer()
    writer.write()
    raise ValueError("export failed")


def write_after_finalizing() -> None:
    writer = Writer()
    writer.finish()
    writer.write()


def finalized_handoff() -> Owned[Writer]:
    writer = Writer()
    writer.finish()
    return writer


def ignored_checksum() -> None:
    report_checksum()


def valid_export() -> None:
    writer = Writer()
    try:
        writer.write()
    finally:
        writer.finish()
