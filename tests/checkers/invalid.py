from codepact.rust import Borrowed, MustUse


class Connection:
    pass


class Other:
    pass


connection: Borrowed[Connection] = Other()  # typing-case: borrowed-assignment
must_use_connection: MustUse[Connection] = Other()  # typing-case: must-use-assignment
