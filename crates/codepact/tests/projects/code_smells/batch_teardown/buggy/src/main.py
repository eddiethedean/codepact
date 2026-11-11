from contextlib import closing

from codepact.rust import Owned

from spool import Spool


def skip_empty(has_pages: bool) -> None:
    spool = Spool()
    if has_pages:
        spool.write()
    else:
        return
    spool.close()


def reject_invalid() -> None:
    spool = Spool()
    spool.write()
    raise ValueError("invalid footer")


def flush_after_scope() -> None:
    spool = Spool()
    with closing(spool):
        spool.write()
    spool.write()  # The closing context has already released the spool.


def open_spool() -> Owned[Spool]:
    spool = Spool()
    with closing(spool):
        return spool


def flush_optional(stop: bool) -> None:
    spool = Spool()
    if stop:
        spool.close()
    spool.write()
