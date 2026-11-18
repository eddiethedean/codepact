"""Controlled CPython probes for the 0.8 Linux model profile."""

from __future__ import annotations

import asyncio
from collections.abc import Callable
import inspect
import multiprocessing
from multiprocessing.context import SpawnContext
from multiprocessing.pool import MaybeEncodingError
from pathlib import Path
import platform
import signal
import sys
from tempfile import TemporaryDirectory
import time
from typing import Never
import unittest


PROFILE = (
    sys.implementation.name == "cpython"
    and platform.python_version() == "3.13.16"
    and sys.platform == "linux"
    and platform.machine() == "x86_64"
)


def write_local_file(path: str, value: str) -> str:
    with open(path, "w", encoding="utf-8") as file:
        _ = file.write(value)
    return value


def fail_worker() -> Never:
    raise ValueError("controlled worker failure")


def unpicklable_result(value: int) -> Callable[[], int]:
    return lambda: value


def wait_for_path(path: Path) -> None:
    deadline = time.monotonic() + 12
    while not path.exists():
        if time.monotonic() >= deadline:
            raise TimeoutError(f"worker never signalled readiness at {path}")
        time.sleep(0.01)


def gated_worker(started: str, release: str, finished: str) -> None:
    _ = write_local_file(started, "ready")
    wait_for_path(Path(release))
    _ = write_local_file(finished, "settled")


@unittest.skipUnless(PROFILE, "requires CPython 3.13.16 on Linux x86_64")
class Phase08FileConformance(unittest.TestCase):
    def test_open_failure_and_aliased_close(self) -> None:
        with TemporaryDirectory() as directory:
            path = str(Path(directory) / "input.txt")
            with self.assertRaises(FileNotFoundError):
                _ = open(path)
            _ = write_local_file(path, "content")
            handle = open(path)
            try:
                alias = handle
                self.assertEqual(alias.read(), "content")
                handle.close()
                self.assertTrue(alias.closed)
                with self.assertRaises(ValueError):
                    _ = alias.read()
            finally:
                handle.close()

    def test_context_closes_the_same_file_on_body_exception(self) -> None:
        with TemporaryDirectory() as directory:
            path = str(Path(directory) / "output.txt")
            with self.assertRaisesRegex(ValueError, "body"):
                with open(path, "w", encoding="utf-8") as handle:
                    alias = handle
                    _ = handle.write("kept")
                    raise ValueError("body")
            self.assertTrue(alias.closed)
            self.assertEqual(Path(path).read_text(), "kept")


@unittest.skipUnless(PROFILE, "requires CPython 3.13.16 on Linux x86_64")
class Phase08AsyncConformance(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self) -> None:
        loop = asyncio.get_running_loop()
        self.assertIsInstance(loop, asyncio.SelectorEventLoop)
        self.assertIsNone(loop.get_task_factory())

    def test_rejected_scheduling_leaves_the_created_coroutine_pending(self) -> None:
        async def worker() -> None:
            self.fail("rejected scheduling entered the coroutine")

        coroutine = worker()
        try:
            with self.assertRaisesRegex(RuntimeError, "no running event loop"):
                _ = asyncio.create_task(coroutine)
            self.assertEqual(inspect.getcoroutinestate(coroutine), inspect.CORO_CREATED)
        finally:
            coroutine.close()

    async def test_prestart_cancellation_skips_body_and_cleanup(self) -> None:
        effects: list[str] = []

        async def worker() -> None:
            try:
                effects.append("body")
            finally:
                effects.append("cleanup")

        task = asyncio.create_task(worker())
        self.assertEqual(effects, [])
        self.assertTrue(task.cancel())
        self.assertFalse(task.done())
        with self.assertRaises(asyncio.CancelledError):
            await task
        self.assertTrue(task.cancelled())
        self.assertEqual(effects, [])

    async def test_pre_cancelled_child_does_not_expire_the_timeout(self) -> None:
        async def worker() -> int:
            return 1

        task = asyncio.create_task(worker())
        task.cancel()
        deadline = asyncio.timeout(60)
        with self.assertRaises(asyncio.CancelledError):
            async with deadline:
                await task
        self.assertFalse(deadline.expired())

    async def test_cancelled_child_stays_cancelled_after_task_group_exit(self) -> None:
        async def worker() -> int:
            return 1

        async with asyncio.TaskGroup() as group:
            task = group.create_task(worker())
            task.cancel()
        self.assertTrue(task.cancelled())
        with self.assertRaises(asyncio.CancelledError):
            await task

    async def test_chained_eager_factory_call_runs_worker_before_cancel(self) -> None:
        async def worker() -> int:
            return 1

        asyncio.get_running_loop().set_task_factory(asyncio.eager_task_factory)
        try:
            task = asyncio.create_task(worker())
            self.assertTrue(task.done())
            self.assertFalse(task.cancel())
            self.assertEqual(await task, 1)
        finally:
            asyncio.get_running_loop().set_task_factory(None)

    async def test_shield_timeout_keeps_task_running(self) -> None:
        started = asyncio.Event()
        release = asyncio.Event()

        async def worker() -> int:
            started.set()
            await release.wait()
            return 7

        task = asyncio.create_task(worker())
        try:
            await asyncio.wait_for(started.wait(), 5)
            with self.assertRaises(TimeoutError):
                async with asyncio.timeout(0):
                    _ = await asyncio.shield(task)
            self.assertFalse(task.done())
        finally:
            release.set()
            self.assertEqual(await asyncio.wait_for(task, 5), 7)

    async def test_repeated_cancellation_can_interrupt_awaited_cleanup(self) -> None:
        started = asyncio.Event()
        cleaning = asyncio.Event()
        cleaned = asyncio.Event()
        release = asyncio.Event()

        async def worker() -> None:
            try:
                started.set()
                await asyncio.Event().wait()
            finally:
                cleaning.set()
                await release.wait()
                cleaned.set()

        task = asyncio.create_task(worker())
        try:
            await asyncio.wait_for(started.wait(), 5)
            self.assertTrue(task.cancel())
            await asyncio.wait_for(cleaning.wait(), 5)
            self.assertFalse(task.done())
            self.assertTrue(task.cancel())
            with self.assertRaises(asyncio.CancelledError):
                await asyncio.wait_for(task, 5)
            self.assertTrue(task.done())
            self.assertFalse(cleaned.is_set())
        finally:
            release.set()
            _ = await asyncio.gather(task, return_exceptions=True)

    async def test_direct_timeout_waits_for_cancellation_and_can_be_suppressed(
        self,
    ) -> None:
        started = asyncio.Event()
        continued = asyncio.Event()

        async def worker() -> int:
            started.set()
            try:
                await asyncio.Event().wait()
            except asyncio.CancelledError:
                await asyncio.sleep(0)
                continued.set()
                return 5
            raise AssertionError("unexpected release")

        task = asyncio.create_task(worker())
        try:
            await asyncio.wait_for(started.wait(), 5)
            async with asyncio.timeout(0):
                self.assertEqual(await task, 5)
            self.assertTrue(task.done())
            self.assertTrue(continued.is_set())
        finally:
            if not task.done():
                _ = task.cancel()
                _ = await asyncio.gather(task, return_exceptions=True)


@unittest.skipUnless(PROFILE, "requires CPython 3.13.16 on Linux x86_64")
class Phase08SpawnConformance(unittest.TestCase):
    def spawn(self) -> SpawnContext:
        context = multiprocessing.get_context("spawn")
        self.assertIsInstance(context, SpawnContext)
        return context

    def test_start_join_close_with_worker_local_file_and_plain_arguments(self) -> None:
        with TemporaryDirectory() as directory:
            path = str(Path(directory) / "worker.txt")
            child = self.spawn().Process(target=write_local_file, args=(path, "item"))
            self.assertFalse(child.is_alive())
            self.assertIsNone(child.exitcode)
            with self.assertRaises(AssertionError):
                child.join()
            child.start()
            try:
                with self.assertRaises(AssertionError):
                    child.start()
                child.join(12)
                self.assertEqual(child.exitcode, 0)
                self.assertEqual(Path(path).read_text(), "item")
            finally:
                if child.is_alive():
                    child.kill()
                    child.join(5)
                child.close()
            with self.assertRaises(ValueError):
                _ = child.is_alive()

    def test_timed_and_interrupted_untimed_join_can_leave_the_child_live(self) -> None:
        def interrupt(_signum: int, _frame: object) -> Never:
            raise OSError("controlled wait interruption")

        with TemporaryDirectory() as directory:
            start, release, finish = (
                Path(directory) / name for name in ("started", "release", "finished")
            )
            child = self.spawn().Process(
                target=gated_worker, args=(str(start), str(release), str(finish))
            )
            child.start()
            try:
                wait_for_path(start)
                self.assertIsNone(child.join(0))
                self.assertTrue(child.is_alive())
                with self.assertRaises(ValueError):
                    child.close()
                previous = signal.signal(signal.SIGALRM, interrupt)
                try:
                    _ = signal.setitimer(signal.ITIMER_REAL, 0.2)
                    self.assertIsNone(child.join())
                finally:
                    _ = signal.setitimer(signal.ITIMER_REAL, 0)
                    _ = signal.signal(signal.SIGALRM, previous)
                self.assertTrue(child.is_alive())
                _ = write_local_file(str(release), "go")
                child.join(12)
                self.assertEqual(child.exitcode, 0)
            finally:
                if child.is_alive():
                    child.kill()
                    child.join(5)
                child.close()

    def test_worker_failure_can_terminate_a_process(self) -> None:
        child = self.spawn().Process(target=fail_worker)
        child.start()
        try:
            child.join(12)
            self.assertFalse(child.is_alive())
            self.assertEqual(child.exitcode, 1)
        finally:
            if child.is_alive():
                child.kill()
                child.join(5)
            child.close()

    def test_spawn_rejects_a_local_lambda_before_launch(self) -> None:
        child = self.spawn().Process(target=print, args=(lambda: 1,))
        try:
            with self.assertRaises(AttributeError):
                child.start()
            self.assertIsNone(child.pid)
            self.assertIsNone(child.exitcode)
            self.assertFalse(child.is_alive())
        finally:
            if child.is_alive():
                child.kill()
                child.join(5)
            child.close()

    def test_pool_submission_drain_and_failed_result_delivery(self) -> None:
        with TemporaryDirectory() as directory:
            path = str(Path(directory) / "pool.txt")
            pool = self.spawn().Pool(1)
            try:
                with self.assertRaises(ValueError):
                    pool.join()
                with self.assertRaises(TypeError):
                    _ = pool.apply_async()
                result = pool.apply_async(write_local_file, (path, "value"))
                failed = pool.apply_async(unpicklable_result, (2,))
                pool.close()
                with self.assertRaises(TypeError):
                    _ = pool.apply_async(args=(1,))
                with self.assertRaises(ValueError):
                    _ = pool.apply_async(write_local_file, (path, "later"))
                self.assertEqual(result.get(timeout=12), "value")
                with self.assertRaises(MaybeEncodingError):
                    _ = failed.get(timeout=12)
                pool.join()
                self.assertEqual(Path(path).read_text(), "value")
            finally:
                pool.terminate()
                pool.join()

    def test_pool_context_forces_shutdown_on_exception(self) -> None:
        with TemporaryDirectory() as directory:
            start, release, finish = (
                Path(directory) / name for name in ("started", "release", "finished")
            )
            with self.assertRaisesRegex(RuntimeError, "context body"):
                with self.spawn().Pool(1) as pool:
                    result = pool.apply_async(
                        gated_worker, (str(start), str(release), str(finish))
                    )
                    wait_for_path(start)
                    raise RuntimeError("context body")
            self.assertFalse(finish.exists())
            self.assertFalse(result.ready())
            with self.assertRaises(ValueError):
                _ = pool.apply_async(write_local_file, (str(finish), "late"))
