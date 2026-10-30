from typepact.rust import Borrowed


class Connection:
    pass


class Other:
    pass


connection: Borrowed[Connection] = Other()  # expected type error
