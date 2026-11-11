from contextlib import closing

from codepact.rust import Owned

from spool import Spool


def write_pages() -> None:
    with closing(Spool()) as spool:
        spool.write()


def skip_empty(has_pages: bool) -> None:
    if has_pages:
        write_pages()


def reject_invalid() -> None:
    with closing(Spool()) as spool:
        spool.write()
        raise ValueError("invalid footer")


def flush_after_scope() -> None:
    with closing(Spool()) as spool:
        spool.write()
        spool.write()


def open_spool() -> Owned[Spool]:
    return Spool()


def flush_optional(stop: bool) -> None:
    spool = Spool()
    if stop:
        spool.close()
        return
    spool.write()
    spool.close()
