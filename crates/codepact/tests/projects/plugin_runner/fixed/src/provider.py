from codepact import must_use, resource


@resource(closed_by="release")
class Token:
    def inspect(self) -> str:
        return "plugin"

    def release(self) -> None:
        pass


def close_token(token: Token) -> None:
    token.release()


@must_use
def plugin_config() -> int:
    return 1
