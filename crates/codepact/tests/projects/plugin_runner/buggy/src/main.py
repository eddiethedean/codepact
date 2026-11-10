from codepact.rust import Borrowed, Owned
from bridge import Token, close_token, plugin_config


def borrowed_cleanup(token: Borrowed[Token]) -> None:
    token.release()


def abandoned_token() -> None:
    token = Token()
    token.inspect()


def steal_borrowed_token(token: Borrowed[Token]) -> Owned[Token]:
    return token


def ignored_configuration() -> None:
    plugin_config()


def use_after_imported_close() -> str:
    token = Token()
    close_token(token)
    return token.inspect()


def valid_plugin() -> str:
    token = Token()
    result = token.inspect()
    token.release()
    return result
