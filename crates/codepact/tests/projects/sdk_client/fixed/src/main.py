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



def ignored_instance():
    client = Catalog()
    return client.fetch()

def ignored_static():
    return Catalog.create_request()

def ignored_class():
    return Catalog.create_cursor()

def ignored_alias():
    operation = get_version
    return operation()

def valid_result():
    client = Catalog()
    return client.fetch()
