from codepact import resource

@resource(closed_by="close")
class Connection:
    def close(self):
        pass

def teardown():
    connection = Connection()
    alias = connection
    connection.close()
    alias.close()
