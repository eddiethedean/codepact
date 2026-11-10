from codepact.rust import Borrowed
from bridge import Token, close_token, plugin_config


def borrowed_cleanup(token: Borrowed[Token]) -> str:
    return token.inspect()


def abandoned_token() -> None:
    token = Token()
    token.release()


def steal_borrowed_token(token: Borrowed[Token]) -> str:
    return token.inspect()


def ignored_configuration() -> int:
    return plugin_config()


def use_after_imported_close() -> str:
    token = Token()
    result = token.inspect()
    close_token(token)
    return result


def valid_plugin() -> str:
    token = Token()
    result = token.inspect()
    token.release()
    return result
