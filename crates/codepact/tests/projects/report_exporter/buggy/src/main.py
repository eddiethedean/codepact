from codepact.rust import Owned
from writer import Writer, report_checksum

def interrupted_export():
    writer = Writer()
    raise ValueError("export failed")

def write_after_finalizing():
    writer = Writer()
    writer.finish()
    writer.write()

def finalized_handoff() -> Owned[Writer]:
    writer = Writer()
    writer.finish()
    return writer

def ignored_checksum():
    report_checksum()

def valid_export():
    writer = Writer()
    try:
        writer.write()
    finally:
        writer.finish()
