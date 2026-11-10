from codepact.rust import Owned
from handles import CsvFile, validate_schema

def abandon_input():
    stream = CsvFile()

def preview_closed():
    stream = CsvFile()
    stream.close()
    return stream.read()

def return_closed_input() -> Owned[CsvFile]:
    stream = CsvFile()
    mirror = stream
    stream.close()
    return mirror

def ignore_validation():
    validate_schema()

def valid_preview():
    stream = CsvFile()
    preview = stream.read()
    stream.close()
    return preview
