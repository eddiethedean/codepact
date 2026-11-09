from codepact.rust import Borrowed, MustUse


class Connection:
    pass


class Other:
    pass


connection: Borrowed[Connection] = Other()  # expected type error
must_use_connection: MustUse[Connection] = Other()  # expected type error
