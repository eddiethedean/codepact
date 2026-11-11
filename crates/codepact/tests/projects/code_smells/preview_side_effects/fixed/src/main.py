from codepact.rust import Borrowed

from decoder import Decoder


def preview(view: Borrowed[Decoder]) -> None:
    local_view = view
    local_view.read()


def prepare(decoder: Decoder) -> None:
    decoder.read()


def render() -> None:
    decoder = Decoder()
    prepare(decoder)
    decoder.read()
    decoder.close()
