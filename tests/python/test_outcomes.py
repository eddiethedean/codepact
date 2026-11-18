import asyncio
import concurrent.futures
from dataclasses import FrozenInstanceError
from typing import get_type_hints
import unittest

import codepact
import codepact.rust as rust_api
from codepact.outcomes import (
    Err,
    Nothing,
    Ok,
    Option,
    Result,
    Some,
    capture,
    capture_async,
    from_nullable,
    to_nullable,
)


class OutcomesTests(unittest.TestCase):
    def assert_same_exception(self, exception: BaseException, callback) -> None:
        with self.assertRaises(type(exception)) as caught:
            callback()
        self.assertIs(caught.exception, exception)

    def test_public_namespaces_share_variant_and_adapter_identities(self) -> None:
        for name in (
            "Ok",
            "Err",
            "Some",
            "Nothing",
            "Result",
            "Option",
            "from_nullable",
            "to_nullable",
            "capture",
            "capture_async",
            "MustHandle",
        ):
            with self.subTest(name=name):
                self.assertIs(getattr(codepact, name), getattr(rust_api, name))

    def test_persistent_metadata_and_reasoned_discard(self) -> None:
        def producer() -> codepact.MustHandle[Option[int]]:
            return Some(3)

        metadata = get_type_hints(producer, include_extras=True)["return"].__metadata__
        self.assertEqual(metadata, (codepact.MustHandleOutcome(),))
        value = producer()
        self.assertIsNone(codepact.discard_outcome(value, reason="cached result"))
        self.assertEqual(value, Some(3))
        with self.assertRaisesRegex(ValueError, "nonempty reason"):
            codepact.discard_outcome(value, reason="  ")

    def test_variants_keep_payload_identity_freeze_fields_and_support_matching(self) -> None:
        payload = object()
        result = Ok(payload)
        self.assertIs(result.value, payload)
        error = object()
        self.assertIs(Err(error).error, error)
        self.assertEqual(Ok(1), Ok(1))
        self.assertNotEqual(Ok(1), Err(1))
        self.assertEqual(Some("present"), Some("present"))
        self.assertEqual(Nothing(), Nothing())
        self.assertEqual(Ok(payload), Ok(payload))
        self.assertEqual(repr(Ok(3)), "Ok(value=3)")
        self.assertEqual(repr(Err("bad")), "Err(error='bad')")
        self.assertEqual(repr(Some(3)), "Some(value=3)")
        self.assertEqual(repr(Nothing()), "Nothing()")
        self.assertEqual(hash(Ok(1)), hash(Ok(1)))
        with self.assertRaises(TypeError):
            hash(Ok([]))
        for outcome, field in (
            (result, "value"),
            (Err("bad"), "error"),
            (Some(1), "value"),
        ):
            with self.subTest(outcome=type(outcome).__name__):
                with self.assertRaises(FrozenInstanceError):
                    setattr(outcome, field, "changed")

        positional = None
        match Ok(3):
            case Ok(value):
                positional = value
            case _:
                self.fail("positional dataclass matching should find Ok")
        keyword = None
        match Err(ValueError("bad")):
            case Err(error=error):
                keyword = error
            case _:
                self.fail("keyword dataclass matching should find Err")
        self.assertEqual(positional, 3)
        self.assertIsInstance(keyword, ValueError)

    def test_nullable_conversion_preserves_falsey_values_and_records_loss(self) -> None:
        for value in (0, False, "", [], {}, ()):
            with self.subTest(value=value):
                optional = from_nullable(value)
                self.assertIsInstance(optional, Some)
                self.assertIs(optional.value, value)
                self.assertIs(to_nullable(optional), value)
        absent = from_nullable(None)
        self.assertEqual(absent, Nothing())
        self.assertIsNone(to_nullable(absent))
        explicit_none = Some(None)
        self.assertNotEqual(explicit_none, absent)
        self.assertIsNone(to_nullable(explicit_none))

    def test_sync_capture_invokes_once_forwards_arguments_and_captures_subclasses(self) -> None:
        value = object()
        calls: list[tuple[tuple[object, ...], dict[str, object]]] = []

        def successful(*args, **kwargs):
            calls.append((args, kwargs))
            return value

        result = capture(successful, (ValueError,), 7, key="value")
        self.assertIsInstance(result, Ok)
        self.assertIs(result.value, value)
        self.assertEqual(calls, [((7,), {"key": "value"})])

        class Specific(ValueError):
            pass

        error = Specific("expected")

        def raises():
            calls.append(((), {}))
            raise error

        captured = capture(raises, (ValueError,))
        self.assertIsInstance(captured, Err)
        self.assertIs(captured.error, error)
        self.assertEqual(len(calls), 2)

    def test_sync_capture_wraps_an_awaitable_without_running_or_joining_it(self) -> None:
        ran = False

        async def operation() -> int:
            nonlocal ran
            ran = True
            return 1

        result = capture(lambda: operation(), (ValueError,))
        self.assertIsInstance(result, Ok)
        self.assertFalse(ran)
        result.value.close()
        self.assertFalse(ran)

    def test_sync_capture_rejects_invalid_declarations_and_preserves_unselected_errors(self) -> None:
        called = False

        def should_not_run():
            nonlocal called
            called = True
            return 1

        for declaration in ((), [ValueError], (BaseException,), (42,), (object,)):
            with self.subTest(declaration=declaration):
                with self.assertRaises(TypeError):
                    capture(should_not_run, declaration)  # type: ignore[arg-type]
        self.assertFalse(called)

        unselected = OSError("not selected")
        self.assert_same_exception(
            unselected,
            lambda: capture(lambda: (_ for _ in ()).throw(unselected), (ValueError,)),
        )
        broad = Exception("broad exception selection")
        captured = capture(lambda: (_ for _ in ()).throw(broad), (Exception,))
        self.assertIsInstance(captured, Err)
        self.assertIs(captured.error, broad)

    def test_sync_capture_propagates_process_control_cancellation_and_groups(self) -> None:
        exceptions: tuple[BaseException, ...] = (
            KeyboardInterrupt(),
            SystemExit(3),
            GeneratorExit(),
            concurrent.futures.CancelledError("cancelled"),
            ExceptionGroup("group", [ValueError("inner")]),
            BaseExceptionGroup("base group", [KeyboardInterrupt()]),
        )
        for exception in exceptions:
            with self.subTest(exception=type(exception).__name__):
                self.assert_same_exception(
                    exception,
                    lambda exception=exception: capture(
                        lambda: (_ for _ in ()).throw(exception), (Exception,)
                    ),
                )

    def test_async_capture_invokes_once_forwards_arguments_and_captures_selected_errors(self) -> None:
        async def run() -> None:
            calls: list[tuple[tuple[object, ...], dict[str, object]]] = []
            value = object()

            async def successful(*args, **kwargs):
                calls.append((args, kwargs))
                return value

            result = await capture_async(successful, (ValueError,), 9, key="async")
            self.assertIsInstance(result, Ok)
            self.assertIs(result.value, value)
            self.assertEqual(calls, [((9,), {"key": "async"})])

            class Specific(ValueError):
                pass

            error = Specific("async failure")

            async def raises_while_awaiting():
                calls.append(((), {}))
                raise error

            captured = await capture_async(raises_while_awaiting, (ValueError,))
            self.assertIsInstance(captured, Err)
            self.assertIs(captured.error, error)
            self.assertEqual(len(calls), 2)

            invocation_error = RuntimeError("invocation failed")

            def raises_before_returning_awaitable():
                raise invocation_error

            with self.assertRaises(RuntimeError) as caught:
                await capture_async(raises_before_returning_awaitable, (ValueError,))  # type: ignore[arg-type]
            self.assertIs(caught.exception, invocation_error)

            unselected = OSError("not selected")

            async def raises_unselected():
                raise unselected

            with self.assertRaises(OSError) as caught_unselected:
                await capture_async(raises_unselected, (ValueError,))
            self.assertIs(caught_unselected.exception, unselected)

            for declaration in ((), [ValueError], (BaseException,), (42,)):
                with self.subTest(declaration=declaration):
                    with self.assertRaises(TypeError):
                        await capture_async(successful, declaration)  # type: ignore[arg-type]

        asyncio.run(run())

    def test_async_capture_propagates_groups_and_concurrent_cancellation(self) -> None:
        async def run() -> None:
            for exception in (
                concurrent.futures.CancelledError("cancelled"),
                ExceptionGroup("group", [ValueError("inner")]),
                KeyboardInterrupt(),
            ):
                async def raises(exception=exception):
                    raise exception

                with self.subTest(exception=type(exception).__name__):
                    with self.assertRaises(type(exception)) as caught:
                        await capture_async(raises, (Exception,))
                    self.assertIs(caught.exception, exception)

        asyncio.run(run())

    def test_async_task_cancellation_before_start_skips_the_callable(self) -> None:
        async def run() -> None:
            called = False

            async def operation():
                nonlocal called
                called = True
                return 1

            task = asyncio.create_task(capture_async(operation, (Exception,)))
            task.cancel()
            with self.assertRaises(asyncio.CancelledError):
                await task
            self.assertFalse(called)

        asyncio.run(run())

    def test_async_task_cancellation_during_await_and_cleanup_propagates(self) -> None:
        async def run() -> None:
            started = asyncio.Event()
            cleanup_started = asyncio.Event()
            finish_cleanup = asyncio.Event()

            async def operation():
                started.set()
                try:
                    await asyncio.Future()
                finally:
                    cleanup_started.set()
                    await finish_cleanup.wait()

            task = asyncio.create_task(capture_async(operation, (Exception,)))
            await asyncio.wait_for(started.wait(), timeout=2)
            task.cancel()
            await asyncio.wait_for(cleanup_started.wait(), timeout=2)
            self.assertFalse(task.done())
            finish_cleanup.set()
            with self.assertRaises(asyncio.CancelledError):
                await task
            self.assertTrue(task.cancelled())

        asyncio.run(run())


if __name__ == "__main__":
    unittest.main()
