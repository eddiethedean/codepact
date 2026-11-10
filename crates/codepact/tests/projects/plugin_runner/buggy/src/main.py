from codepact.rust import Borrowed, Owned
from bridge import Token, close_token, plugin_config

def borrowed_cleanup(token: Borrowed[Token]):
    token.release()

def abandoned_token():
    token = Token()

def steal_borrowed_token(token: Borrowed[Token]) -> Owned[Token]:
    return token

def ignored_configuration():
    plugin_config()

def use_after_imported_close():
    token = Token()
    close_token(token)
    return token.inspect()

def valid_plugin():
    token = Token()
    result = token.inspect()
    token.release()
    return result
