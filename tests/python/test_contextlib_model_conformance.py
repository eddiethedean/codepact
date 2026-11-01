import asyncio
import contextlib
import unittest


class BodyError(Exception):
    pass


class CleanupError(Exception):
    pass


class ClosingResource:
    def __init__(self, events: list[str], *, fail: bool = False) -> None:
        self.events = events
        self.fail = fail

    def close(self) -> None:
        self.events.append("close")
        if self.fail:
            raise CleanupError


class AsyncClosingResource:
    def __init__(self, events: list[str], *, fail: bool = False) -> None:
        self.events = events
        self.fail = fail

    async def aclose(self) -> None:
        self.events.append("aclose")
        if self.fail:
            raise CleanupError


class ContextlibModelConformanceTest(unittest.TestCase):
    def test_closing_runs_once_and_does_not_suppress_body_errors(self) -> None:
        events: list[str] = []
        resource = ClosingResource(events)

        with contextlib.closing(resource):
            pass
        self.assertEqual(events, ["close"])

        with self.assertRaises(BodyError):
            with contextlib.closing(resource):
                raise BodyError
        self.assertEqual(events, ["close", "close"])

    def test_closing_failure_propagates_over_body_failure(self) -> None:
        events: list[str] = []
        resource = ClosingResource(events, fail=True)

        with self.assertRaises(CleanupError):
            with contextlib.closing(resource):
                raise BodyError
        self.assertEqual(events, ["close"])

    def test_aclosing_awaits_once_and_does_not_suppress_body_errors(self) -> None:
        async def run() -> list[str]:
            events: list[str] = []
            resource = AsyncClosingResource(events)

            async with contextlib.aclosing(resource):
                pass
            self.assertEqual(events, ["aclose"])

            with self.assertRaises(BodyError):
                async with contextlib.aclosing(resource):
                    raise BodyError
            self.assertEqual(events, ["aclose", "aclose"])
            return events

        self.assertEqual(asyncio.run(run()), ["aclose", "aclose"])

    def test_aclosing_failure_propagates_over_body_failure(self) -> None:
        async def run() -> list[str]:
            events: list[str] = []
            resource = AsyncClosingResource(events, fail=True)

            with self.assertRaises(CleanupError):
                async with contextlib.aclosing(resource):
                    raise BodyError
            return events

        self.assertEqual(asyncio.run(run()), ["aclose"])

    def test_aclosing_runs_when_the_body_is_cancelled(self) -> None:
        async def run() -> list[str]:
            events: list[str] = []
            resource = AsyncClosingResource(events)
            body_started = asyncio.Event()
            never = asyncio.Event()

            async def worker() -> None:
                async with contextlib.aclosing(resource):
                    body_started.set()
                    await never.wait()

            task = asyncio.create_task(worker())
            try:
                await asyncio.wait_for(body_started.wait(), timeout=1)
                task.cancel()
                with self.assertRaises(asyncio.CancelledError):
                    await task
                return events
            finally:
                if not task.done():
                    task.cancel()
                    try:
                        await task
                    except asyncio.CancelledError:
                        pass

        self.assertEqual(asyncio.run(run()), ["aclose"])

    def test_repeated_cancellation_can_interrupt_aclosing(self) -> None:
        async def run() -> list[str]:
            events: list[str] = []
            close_started = asyncio.Event()
            never = asyncio.Event()

            class InterruptibleResource:
                async def aclose(self) -> None:
                    events.append("aclose-start")
                    close_started.set()
                    await never.wait()
                    events.append("aclose-finish")

            body_started = asyncio.Event()

            async def worker() -> None:
                async with contextlib.aclosing(InterruptibleResource()):
                    body_started.set()
                    await never.wait()

            task = asyncio.create_task(worker())
            try:
                await asyncio.wait_for(body_started.wait(), timeout=1)
                task.cancel()
                await asyncio.wait_for(close_started.wait(), timeout=1)
                task.cancel()
                with self.assertRaises(asyncio.CancelledError):
                    await task
                return events
            finally:
                if not task.done():
                    task.cancel()
                    try:
                        await task
                    except asyncio.CancelledError:
                        pass

        self.assertEqual(asyncio.run(run()), ["aclose-start"])


if __name__ == "__main__":
    unittest.main()
