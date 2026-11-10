from codepact.rust import Owned
from warehouse import Cursor, render_select


def overwrite_cursor() -> None:
    cursor = Cursor()
    cursor = Cursor()
    cursor.finish()


def query_closed_cursor() -> None:
    cursor = Cursor()
    cursor.finish()
    cursor.execute()


def handoff_closed_cursor() -> Owned[Cursor]:
    cursor = Cursor()
    try:
        return cursor
    finally:
        cursor.finish()


def discard_query() -> None:
    render_select()


def valid_load() -> None:
    cursor = Cursor()
    try:
        cursor.execute()
    finally:
        cursor.finish()
