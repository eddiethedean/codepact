"""Controlled runtime probes for the pinned default CPython TaskGroup model."""

from __future__ import annotations

import asyncio
import inspect
import sys
import unittest


@unittest.skipUnless(
    sys.implementation.name == "cpython" and (3, 11) <= sys.version_info[:2] <= (3, 13),
    "TaskGroup model targets CPython 3.11 through 3.13",
)
class TaskGroupModelConformance(unittest.IsolatedAsyncioTestCase):
    async def test_default_factory_waits_and_does_not_start_eagerly(self) -> None:
        events: list[str] = []

        async def child() -> None:
            events.append("entered")
            await asyncio.sleep(0)
            events.append("finished")

        async with asyncio.TaskGroup() as group:
            task = group.create_task(child())
            self.assertEqual(events, [])
            self.assertIsNone(asyncio.get_running_loop().get_task_factory())
        self.assertTrue(task.done())
        self.assertEqual(events, ["entered", "finished"])

    async def test_pre_start_cancellation_skips_body_and_finally(self) -> None:
        events: list[str] = []

        async def child() -> None:
            try:
                events.append("entered")
                await asyncio.sleep(0)
            finally:
                events.append("closed")

        async with asyncio.TaskGroup() as group:
            task = group.create_task(child())
            task.cancel()
            self.assertFalse(task.done())
        self.assertTrue(task.cancelled())
        self.assertEqual(events, [])

    async def test_rejected_submission_has_version_specific_coroutine_state(self) -> None:
        async def child() -> None:
            self.fail("an inactive group must not start a worker")

        coroutine = child()
        try:
            group = asyncio.TaskGroup()
            async with group:
                pass
            with self.assertRaises(RuntimeError):
                group.create_task(coroutine)
            expected = inspect.CORO_CLOSED if sys.version_info >= (3, 13) else inspect.CORO_CREATED
            self.assertEqual(inspect.getcoroutinestate(coroutine), expected)
        finally:
            coroutine.close()

    async def test_child_failure_waits_for_sibling_cleanup(self) -> None:
        started = asyncio.Event()
        cleaned = asyncio.Event()

        async def sibling() -> None:
            try:
                started.set()
                await asyncio.Event().wait()
            finally:
                await asyncio.sleep(0)
                cleaned.set()

        async def fail() -> None:
            await started.wait()
            raise ValueError("test child failure")

        with self.assertRaises(ExceptionGroup):
            async with asyncio.TaskGroup() as group:
                task = group.create_task(sibling())
                group.create_task(fail())
        self.assertTrue(task.done())
        self.assertTrue(cleaned.is_set())

    async def test_repeated_parent_cancellation_waits_for_child_finally(self) -> None:
        started = asyncio.Event()
        cleaning = asyncio.Event()
        release = asyncio.Event()
        cleaned = asyncio.Event()

        async def child() -> None:
            try:
                started.set()
                await asyncio.Event().wait()
            finally:
                cleaning.set()
                await release.wait()
                cleaned.set()

        async def parent() -> None:
            async with asyncio.TaskGroup() as group:
                group.create_task(child())

        task = asyncio.create_task(parent())
        await asyncio.wait_for(started.wait(), 1)
        task.cancel()
        await asyncio.wait_for(cleaning.wait(), 1)
        task.cancel()
        await asyncio.sleep(0)
        self.assertFalse(task.done())
        release.set()
        with self.assertRaises(asyncio.CancelledError):
            await asyncio.wait_for(task, 1)
        self.assertTrue(cleaned.is_set())

    async def test_cancellation_can_interrupt_awaited_cleanup(self) -> None:
        cleaning = asyncio.Event()
        cleaned = asyncio.Event()

        async def child() -> None:
            cleaning.set()
            await asyncio.Event().wait()
            cleaned.set()

        async with asyncio.TaskGroup() as group:
            task = group.create_task(child())
            await asyncio.wait_for(cleaning.wait(), 1)
            task.cancel()
        self.assertTrue(task.cancelled())
        self.assertFalse(cleaned.is_set())
