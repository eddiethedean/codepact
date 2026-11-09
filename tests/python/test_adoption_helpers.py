from __future__ import annotations

import unittest

from codepact import discard, must_use
from codepact.contracts import MustUseResult
from codepact.rust import MustUse


class AdoptionHelperTests(unittest.TestCase):
    def test_must_use_decorator_preserves_identity_and_signature(self) -> None:
        def fetch(value: int) -> int:
            return value

        decorated = must_use(fetch)
        self.assertIs(decorated, fetch)
        self.assertEqual(decorated(4), 4)
        self.assertEqual(decorated.__annotations__, {"value": "int", "return": "int"})
        self.assertIsInstance(fetch.__codepact_must_use__, MustUseResult)

    def test_must_use_alias_preserves_the_underlying_type(self) -> None:
        self.assertIs(MustUse[int].__origin__, int)
        self.assertIsInstance(MustUse[int].__metadata__[0], MustUseResult)

    def test_discard_is_a_noop_over_the_value(self) -> None:
        self.assertIsNone(discard(object()))


if __name__ == "__main__":
    unittest.main()
