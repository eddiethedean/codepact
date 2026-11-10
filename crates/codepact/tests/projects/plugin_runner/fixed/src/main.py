from codepact.rust import Borrowed, Owned
from bridge import Token, close_token, plugin_config

def borrowed_cleanup(token: Borrowed[Token]):
    return token.inspect()

def abandoned_token():
    token = Token()
    token.release()

def steal_borrowed_token(token: Borrowed[Token]) -> str:
    return token.inspect()

def ignored_configuration():
    return plugin_config()

def use_after_imported_close():
    token = Token()
    result = token.inspect()
    close_token(token)
    return result

def valid_plugin():
    token = Token()
    result = token.inspect()
    token.release()
    return result
