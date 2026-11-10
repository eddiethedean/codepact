from codepact import resource

@resource(closed_by="close")
class BrokerLink:
    def ack(self):
        return None

    def close(self):
        pass

from codepact import must_use

@must_use
def decode_message() -> str:
    return "payload"
