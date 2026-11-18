from codepact import resource


@resource(closed_by="close")
class ExportSession:
    def fetch_receipt(self) -> str:
        return "receipt-001"

    def close(self) -> None:
        pass
