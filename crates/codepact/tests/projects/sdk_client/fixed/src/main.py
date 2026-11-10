from codepact import must_use
from catalog import get_version


class Catalog:
    @must_use
    def fetch(self) -> int:
        return 1

    @staticmethod
    @must_use
    def create_request() -> int:
        return 2

    @classmethod
    @must_use
    def create_cursor(cls) -> int:
        return 3


def ignored_instance() -> int:
    client = Catalog()
    return client.fetch()


def ignored_static() -> int:
    return Catalog.create_request()


def ignored_class() -> int:
    return Catalog.create_cursor()


def ignored_alias() -> str:
    operation = get_version
    return operation()


def valid_result() -> int:
    client = Catalog()
    return client.fetch()
