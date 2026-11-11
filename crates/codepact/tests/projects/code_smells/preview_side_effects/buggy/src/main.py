from codepact.rust import Borrowed

from decoder import Decoder


def preview(view: Borrowed[Decoder]) -> None:
    local_view = view
    local_view.close()


def prepare(decoder: Decoder) -> None:
    decoder.close()


def render() -> None:
    decoder = Decoder()
    prepare(decoder)
    decoder.read()
