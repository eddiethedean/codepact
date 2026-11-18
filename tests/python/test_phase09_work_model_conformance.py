"""Runtime controls for modeled work observation at the pinned runtime."""

from __future__ import annotations

import asyncio
from collections.abc import AsyncIterator, Callable
import concurrent.futures
import multiprocessing
from pathlib import Path
import platform
import sys
from tempfile import TemporaryDirectory
import threading
import time
from typing import cast
import unittest


PROFILE = (
    sys.implementation.name == "cpython"
    and platform.python_version() == "3.13.16"
    and sys.platform == "linux"
    and platform.machine() == "x86_64"
)


def worker_value() -> int:
    return 7


def worker_error() -> int:
    raise ValueError("worker failed")


def worker_unserializable() -> Callable[[], int]:
    return lambda: worker_value()


def worker_after_signal(signal: str) -> int:
    deadline = time.monotonic() + 10
    while not Path(signal).exists():
        if time.monotonic() > deadline:
            raise TimeoutError("probe failed to release worker")
        time.sleep(0.01)
    return worker_value()


@unittest.skipUnless(PROFILE, "requires CPython 3.13.16 on Linux x86_64")
class Phase09AsyncWorkConformance(unittest.IsolatedAsyncioTestCase):
    async def test_gather_early_failure_and_internal_retrieval(self) -> None:
        release = asyncio.Event()
        late_finished = asyncio.Event()

        async def first() -> None:
            raise ValueError("first")

        async def later() -> None:
            await release.wait()
            late_finished.set()
            raise RuntimeError("sibling")

        initial = asyncio.create_task(first())
        sibling = asyncio.create_task(later())
        aggregate = asyncio.gather(initial, sibling)
        with self.assertRaisesRegex(ValueError, "first"):
            await aggregate
        self.assertFalse(sibling.done())
        release.set()
        await late_finished.wait()
        await asyncio.sleep(0)
        # The pinned implementation retrieves later exceptions internally.
        self.assertIs(getattr(sibling, "_log_traceback", None), False)
        with self.assertRaisesRegex(RuntimeError, "sibling"):
            sibling.result()

    async def test_gather_fixed_slots_and_wait_members(self) -> None:
        loop = asyncio.get_running_loop()
        value: asyncio.Future[object] = loop.create_future()
        failure: asyncio.Future[object] = loop.create_future()
        finished, pending = await asyncio.wait((value, failure), timeout=0)
        self.assertEqual(finished, set())
        self.assertEqual(pending, {value, failure})
        item = object()
        error = ValueError("worker")
        value.set_result(item)
        failure.set_exception(error)
        delivered, raised = await asyncio.gather(value, failure, return_exceptions=True)
        self.assertIs(delivered, item)
        self.assertIs(raised, error)
        self.assertIs(value.result(), item)
        self.assertIs(failure.exception(), error)
        completion_order = cast(
            AsyncIterator[asyncio.Future[object]],
            asyncio.as_completed((value, failure)),
        )
        self.assertCountEqual(
            [member async for member in completion_order], [value, failure]
        )

    async def test_shielded_timeout_leaves_underlying_task_pending(self) -> None:
        release = asyncio.Event()

        async def worker() -> int:
            await release.wait()
            return 11

        underlying = asyncio.create_task(worker())
        try:
            with self.assertRaises(TimeoutError):
                async with asyncio.timeout(0):
                    await asyncio.shield(underlying)
            self.assertFalse(underlying.done())
        finally:
            release.set()
        self.assertEqual(await underlying, 11)


@unittest.skipUnless(PROFILE, "requires CPython 3.13.16 on Linux x86_64")
class Phase09ProcessWorkConformance(unittest.TestCase):
    def test_pool_wait_and_get_have_distinct_effects(self) -> None:
        with TemporaryDirectory() as directory:
            release = Path(directory) / "release"
            with multiprocessing.get_context("spawn").Pool(1) as pool:
                pending = pool.apply_async(worker_after_signal, (str(release),))
                alias = pending
                self.assertFalse(pending.ready())
                with self.assertRaises(ValueError):
                    pending.successful()
                wait = cast(Callable[[float], object], pending.wait)
                self.assertIsNone(wait(0))
                with self.assertRaises(multiprocessing.TimeoutError):
                    pending.get(0)
                release.touch()
                self.assertIsNone(cast(Callable[[float], object], alias.wait)(8))
                self.assertTrue(pending.ready())
                self.assertTrue(pending.successful())
                self.assertEqual(alias.get(), 7)
                failure = pool.apply_async(worker_error)
                with self.assertRaisesRegex(ValueError, "worker failed"):
                    failure.get(8)
                self.assertFalse(failure.successful())

    def test_pool_reports_failed_result_delivery(self) -> None:
        with multiprocessing.get_context("spawn").Pool(1) as pool:
            result = pool.apply_async(worker_unserializable)
            with self.assertRaises(Exception) as raised:
                result.get(8)
            error_type = type(raised.exception)
            self.assertEqual(
                (error_type.__module__, error_type.__name__),
                ("multiprocessing.pool", "MaybeEncodingError"),
            )


@unittest.skipUnless(PROFILE, "requires CPython 3.13.16 on Linux x86_64")
class Phase09ThreadWorkConformance(unittest.TestCase):
    def test_thread_timeout_cancel_rejection_and_context_join(self) -> None:
        entered = threading.Event()
        release = threading.Event()

        def worker() -> int:
            entered.set()
            if not release.wait(8):
                raise TimeoutError("probe failed to release worker")
            return 7

        with concurrent.futures.ThreadPoolExecutor() as executor:
            pending = executor.submit(worker)
            try:
                self.assertTrue(entered.wait(5))
                self.assertTrue(pending.running())
                self.assertFalse(pending.cancel())
                with self.assertRaises(TimeoutError):
                    pending.result(0)
            finally:
                release.set()
        self.assertTrue(pending.done())
        self.assertIsNone(pending.exception())
        self.assertEqual(pending.result(), 7)
        with self.assertRaises(RuntimeError):
            executor.submit(worker)

    def test_thread_worker_error_retains_identity(self) -> None:
        with concurrent.futures.ThreadPoolExecutor() as executor:
            pending = executor.submit(worker_error)
            alias = pending
            observed = pending.exception(5)
            self.assertIsInstance(observed, ValueError)
            with self.assertRaises(ValueError) as raised:
                alias.result()
            self.assertIs(raised.exception, observed)
