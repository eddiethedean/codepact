from codepact.rust import Owned
from warehouse import Cursor, render_select

def overwrite_cursor():
    cursor = Cursor()
    cursor.finish()
    cursor = Cursor()
    cursor.finish()

def query_closed_cursor():
    cursor = Cursor()
    cursor.execute()
    cursor.finish()

def handoff_closed_cursor() -> Owned[Cursor]:
    cursor = Cursor()
    return cursor

def discard_query():
    return render_select()

def valid_load():
    cursor = Cursor()
    try:
        cursor.execute()
    finally:
        cursor.finish()
