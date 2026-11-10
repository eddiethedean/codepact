from codepact.rust import MustUse, Owned
from writer import Writer, report_checksum


def interrupted_export() -> None:
    writer = Writer()
    try:
        raise ValueError("export failed")
    finally:
        writer.finish()


def write_after_finalizing() -> None:
    writer = Writer()
    writer.write()
    writer.finish()


def finalized_handoff() -> Owned[Writer]:
    writer = Writer()
    return writer


def ignored_checksum() -> MustUse[str]:
    return report_checksum()


def valid_export() -> None:
    writer = Writer()
    try:
        writer.write()
    finally:
        writer.finish()
