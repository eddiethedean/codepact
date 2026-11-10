from codepact.rust import Owned
from writer import Writer, report_checksum

def interrupted_export():
    writer = Writer()
    try:
        raise ValueError("export failed")
    finally:
        writer.finish()

def write_after_finalizing():
    writer = Writer()
    writer.write()
    writer.finish()

def finalized_handoff() -> Owned[Writer]:
    writer = Writer()
    return writer

def ignored_checksum():
    return report_checksum()

def valid_export():
    writer = Writer()
    try:
        writer.write()
    finally:
        writer.finish()
