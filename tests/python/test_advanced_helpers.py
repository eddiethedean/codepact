import inspect
import unittest
from typing import Annotated, Literal, get_args, get_origin

import codepact
from codepact.contracts import StateAccess
from codepact.rust import State


class AdvancedHelpersTest(unittest.TestCase):
    def test_state_machine_and_transition_preserve_identity_and_signatures(
        self,
    ) -> None:
        @codepact.state_machine(states=("ready", "closed"), initial="ready")
        class Channel:
            @codepact.transition(
                from_state=("ready",), to_state="closed", on_raise="ready"
            )
            def close(self, reason: str = "done") -> None:
                pass

        declaration = Channel.__dict__["__codepact_state_machine__"]
        transition = Channel.__dict__["close"].__codepact_transition__
        self.assertEqual(declaration, (("ready", "closed"), "ready"))
        self.assertEqual(transition, (("ready",), "closed", "ready", None))
        self.assertEqual(
            inspect.signature(Channel.close),
            inspect.Signature(
                [
                    inspect.Parameter("self", inspect.Parameter.POSITIONAL_OR_KEYWORD),
                    inspect.Parameter(
                        "reason",
                        inspect.Parameter.POSITIONAL_OR_KEYWORD,
                        default="done",
                        annotation=str,
                    ),
                ],
                return_annotation=None,
            ),
        )

    def test_state_alias_retains_underlying_type_and_metadata(self) -> None:
        class Channel:
            pass

        annotation = State[Channel, Literal["ready"]]
        self.assertIs(get_origin(annotation), State)
        self.assertEqual(get_args(annotation), (Channel, Literal["ready"]))
        alias_value = State.__value__
        self.assertIs(get_origin(alias_value), Annotated)
        state_metadata = get_args(alias_value)[1]
        self.assertIs(get_origin(state_metadata), StateAccess)

    def test_invalid_finite_metadata_fails_at_declaration_time(self) -> None:
        with self.assertRaises(ValueError):
            codepact.state_machine(states=("ready", "ready"), initial="ready")
        with self.assertRaises(ValueError):
            codepact.state_machine(states=("ready",), initial="missing")
        with self.assertRaises(ValueError):
            codepact.state_machine(states=("class",), initial="class")
        with self.assertRaises(ValueError):
            codepact.transition(from_state=(), to_state="closed")
        with self.assertRaises(ValueError):
            codepact.transition(from_state="ready", to_state="not-valid!")


if __name__ == "__main__":
    unittest.main()
