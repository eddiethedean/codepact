from codepact import resource

@resource(closed_by="release")
class Token:
    def inspect(self):
        return "plugin"

    def release(self):
        pass

def close_token(token: Token):
    token.release()

from codepact import must_use

@must_use
def plugin_config() -> int:
    return 1
