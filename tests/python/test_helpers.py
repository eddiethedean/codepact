import unittest

import typepact
from typepact.contracts import Borrowed, Resource, SharedBorrow
from typepact.rust import Borrowed as ExplicitBorrowed


class HelpersTest(unittest.TestCase):
    def test_decorator_preserves_identity_and_records_metadata(self):
        class Connection:
            pass

        decorated = typepact.resource(Connection, closed_by="close")
        self.assertIs(decorated, Connection)
        self.assertEqual(Connection.__typepact_resource__, Resource("close"))

    def test_decorator_form_preserves_identity(self):
        @typepact.resource(closed_by="close")
        class Connection:
            pass

        self.assertEqual(Connection.__dict__["__typepact_resource__"], Resource("close"))

    def test_aliases_retain_underlying_type_and_imports_are_lightweight(self):
        class Connection:
            pass

        self.assertIs(Borrowed[Connection].__origin__, Connection)
        self.assertIs(ExplicitBorrowed[Connection].__origin__, Connection)
        self.assertEqual(SharedBorrow(), Borrowed[Connection].__metadata__[0])
        self.assertNotIn("typepact._native", __import__("sys").modules)


if __name__ == "__main__":
    unittest.main()
