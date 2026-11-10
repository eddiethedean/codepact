from codepact import must_use, resource


@resource(closed_by="close")
class BrokerLink:
    def ack(self) -> None:
        return None

    def close(self) -> None:
        pass


@must_use
def decode_message() -> str:
    return "payload"
