from codepact.rust import MustUse, Owned
from warehouse import Cursor, render_select


def overwrite_cursor() -> None:
    cursor = Cursor()
    cursor.finish()
    cursor = Cursor()
    cursor.finish()


def query_closed_cursor() -> None:
    cursor = Cursor()
    cursor.execute()
    cursor.finish()


def handoff_closed_cursor() -> Owned[Cursor]:
    cursor = Cursor()
    return cursor


def discard_query() -> MustUse[str]:
    return render_select()


def valid_load() -> None:
    cursor = Cursor()
    try:
        cursor.execute()
    finally:
        cursor.finish()
