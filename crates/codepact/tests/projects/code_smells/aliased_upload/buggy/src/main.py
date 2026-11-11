from codepact import move

from upload import Upload, submit


def duplicate_owner() -> None:
    owner = Upload()
    retained = owner
    _queued_owner = move(owner)
    retained.close()


def retry_after_handoff() -> None:
    owner = Upload()
    receiver = move(owner)
    owner.write()
    submit(move(receiver))
