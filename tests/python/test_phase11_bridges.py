"""Runtime controls for the pinned executor bridge profiles."""

from __future__ import annotations

import asyncio
from concurrent.futures import ProcessPoolExecutor, ThreadPoolExecutor, TimeoutError
from contextvars import ContextVar
import multiprocessing
from pathlib import Path
from tempfile import TemporaryDirectory
from threading import Event, Thread
import time
import unittest


TIMEOUT = 10


def process_gate(directory: str) -> int:
    root = Path(directory)
    (root / "entered").touch()
    deadline = time.monotonic() + TIMEOUT
    while not (root / "release").exists():
        if time.monotonic() >= deadline:
            raise TimeoutError("process work was not released")
        time.sleep(0.01)
    return 42


def ready(path: Path) -> None:
    deadline = time.monotonic() + TIMEOUT
    while not path.exists():
        if time.monotonic() >= deadline:
            raise TimeoutError("process worker did not enter")
        time.sleep(0.01)


class Phase11BridgeConformance(unittest.IsolatedAsyncioTestCase):
    async def test_to_thread_is_lazy_and_copies_context_at_execution(self) -> None:
        label = ContextVar("label", default="default")
        calls: list[str] = []

        def work() -> str:
            value = label.get()
            calls.append(value)
            return value

        label.set("construction")
        pending = asyncio.to_thread(work)
        self.assertEqual(calls, [])
        label.set("execution")
        self.assertEqual(await pending, "execution")
        self.assertEqual(calls, ["execution"])
        never_started = asyncio.create_task(asyncio.to_thread(work))
        never_started.cancel()
        with self.assertRaises(asyncio.CancelledError):
            await never_started
        self.assertEqual(calls, ["execution"])

    async def test_run_in_executor_submits_immediately_without_copying_context(
        self,
    ) -> None:
        label = ContextVar("label", default="default")
        entered = Event()
        release = Event()

        def work() -> str:
            entered.set()
            if not release.wait(TIMEOUT):
                raise TimeoutError("thread work was not released")
            return label.get()

        label.set("loop")
        future = asyncio.get_running_loop().run_in_executor(None, work)
        try:
            self.assertTrue(await asyncio.to_thread(entered.wait, TIMEOUT))
            self.assertFalse(future.done())
        finally:
            release.set()
        self.assertEqual(await asyncio.wait_for(future, TIMEOUT), "default")

    async def test_wrapper_cancel_keeps_running_work_and_can_cancel_queued_work(
        self,
    ) -> None:
        entered = Event()
        release = Event()
        skipped = Event()

        def active() -> int:
            entered.set()
            if not release.wait(TIMEOUT):
                raise TimeoutError("worker was not released")
            return 42

        with ThreadPoolExecutor(max_workers=1) as executor:
            source = executor.submit(active)
            queued = executor.submit(skipped.set)
            running_wrapper = asyncio.wrap_future(source)
            queued_wrapper = asyncio.wrap_future(queued)
            try:
                self.assertTrue(await asyncio.to_thread(entered.wait, TIMEOUT))
                self.assertTrue(running_wrapper.cancel())
                self.assertTrue(queued_wrapper.cancel())
                # The queued job can be cancelled before the running job returns.
                await asyncio.sleep(0)
                self.assertTrue(queued.cancelled())
                self.assertFalse(source.done())
                self.assertFalse(source.cancel())
                self.assertFalse(skipped.is_set())
                with self.assertRaises(asyncio.CancelledError):
                    await running_wrapper
            finally:
                release.set()
            self.assertEqual(source.result(timeout=TIMEOUT), 42)
            self.assertFalse(skipped.is_set())

    async def test_cross_thread_cancel_starts_finalizer_before_completion(self) -> None:
        loop = asyncio.get_running_loop()
        entered = Event()
        finalizing = Event()
        cleaned = Event()
        release = asyncio.Event()

        async def work() -> int:
            try:
                entered.set()
                await asyncio.Event().wait()
            finally:
                finalizing.set()
                await release.wait()
                cleaned.set()
            return 42

        def submit() -> None:
            future = asyncio.run_coroutine_threadsafe(work(), loop)
            try:
                self.assertTrue(entered.wait(TIMEOUT))
                self.assertTrue(future.cancel())
                self.assertTrue(finalizing.wait(TIMEOUT))
                self.assertTrue(future.cancelled())
                self.assertFalse(cleaned.is_set())
            finally:
                loop.call_soon_threadsafe(release.set)

        await asyncio.to_thread(submit)
        self.assertTrue(await asyncio.to_thread(cleaned.wait, TIMEOUT))

    async def test_cross_thread_result_and_exception_delivery(self) -> None:
        loop = asyncio.get_running_loop()

        async def valid() -> int:
            return 42

        async def invalid() -> int:
            raise ValueError("worker failure")

        def submit() -> None:
            self.assertEqual(
                asyncio.run_coroutine_threadsafe(valid(), loop).result(TIMEOUT), 42
            )
            error = asyncio.run_coroutine_threadsafe(invalid(), loop).exception(TIMEOUT)
            self.assertIsInstance(error, ValueError)

        await asyncio.to_thread(submit)


class Phase11ShutdownConformance(unittest.TestCase):
    def test_process_pool_repeated_shutdown_cannot_join_detached_manager(self) -> None:
        for explicit_wait in (True, False):
            with (
                self.subTest(explicit_wait=explicit_wait),
                TemporaryDirectory() as directory,
            ):
                root = Path(directory)
                executor = ProcessPoolExecutor(
                    max_workers=1,
                    mp_context=multiprocessing.get_context("spawn"),
                )
                future = executor.submit(process_gate, directory)
                try:
                    ready(root / "entered")
                    with executor:
                        executor.shutdown(wait=False)
                        if explicit_wait:
                            executor.shutdown(wait=True)
                    self.assertFalse(future.done())
                finally:
                    (root / "release").touch()
                    self.assertEqual(future.result(timeout=TIMEOUT), 42)
                    executor.shutdown(wait=True)

    def test_thread_pool_waiting_shutdown_can_follow_nonwaiting_shutdown(self) -> None:
        started = Event()
        release = Event()
        joining = Event()
        done = Event()

        def work() -> None:
            started.set()
            if not release.wait(TIMEOUT):
                raise TimeoutError("thread work was not released")

        def shutdown(executor: ThreadPoolExecutor) -> None:
            joining.set()
            executor.shutdown(wait=True)
            done.set()

        with ThreadPoolExecutor(max_workers=1) as executor:
            future = executor.submit(work)
            supervisor = Thread(target=shutdown, args=(executor,))
            try:
                self.assertTrue(started.wait(TIMEOUT))
                executor.shutdown(wait=False)
                supervisor.start()
                self.assertTrue(joining.wait(TIMEOUT))
                self.assertFalse(done.is_set())
            finally:
                release.set()
                if supervisor.ident is not None:
                    supervisor.join(TIMEOUT)
            self.assertTrue(done.is_set())
            self.assertIsNone(future.result(timeout=TIMEOUT))
