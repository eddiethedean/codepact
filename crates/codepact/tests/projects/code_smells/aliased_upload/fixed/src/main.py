from codepact import move

from upload import Upload, submit


def duplicate_owner() -> None:
    owner = Upload()
    owner.write()
    submit(move(owner))


def retry_after_handoff() -> None:
    owner = Upload()
    receiver = move(owner)
    receiver.write()
    submit(move(receiver))
