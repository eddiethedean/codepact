"""Runtime controls for the reviewed Phase 0.10 stdlib models."""

from __future__ import annotations

import asyncio
from collections.abc import Awaitable, Callable
import multiprocessing
from multiprocessing.connection import Connection
from multiprocessing.queues import JoinableQueue, Queue
from multiprocessing.sharedctypes import Synchronized, SynchronizedArray
from multiprocessing.synchronize import Event, RLock
from pathlib import Path
import platform
import queue
import subprocess
import sys
from tempfile import TemporaryDirectory, TemporaryFile
import threading
from typing import TypeVar
import unittest


PROFILE = (
    sys.implementation.name == "cpython"
    and platform.python_version() == "3.13.16"
    and sys.platform == "linux"
    and platform.machine() == "x86_64"
)
TIMEOUT = 10
T = TypeVar("T")


async def started(operation: Awaitable[T]) -> asyncio.Task[T]:
    entered = asyncio.Event()

    async def run() -> T:
        entered.set()
        return await operation

    task = asyncio.create_task(run())
    await asyncio.wait_for(entered.wait(), TIMEOUT)
    return task


def close_process(process: multiprocessing.Process) -> None:
    if process.is_alive():
        process.terminate()
        process.join(TIMEOUT)
        if process.is_alive():
            process.kill()
            process.join(TIMEOUT)
    process.close()


def receive(connection: Connection) -> object:
    if not connection.poll(TIMEOUT):
        raise TimeoutError("control connection never became ready")
    return connection.recv()


def process_items(work: JoinableQueue, reply: Connection, finish: Event) -> None:
    with reply:
        item = work.get(timeout=TIMEOUT)
        with TemporaryFile() as local:
            local.write(b"worker-local")
        work.task_done()
        if not finish.wait(TIMEOUT):
            raise TimeoutError("probe never released child")
        reply.send((item, local.closed))
        work.close()


def receive_after_gate(work: Queue, allowed: Event, reply: Connection) -> None:
    with reply:
        if not allowed.wait(TIMEOUT):
            raise TimeoutError("probe never allowed receipt")
        reply.send(work.get(timeout=TIMEOUT))
        work.close()


def close_writer(writer: Connection) -> None:
    writer.close()


def reject_lock_release(lock: RLock, reply: Connection) -> None:
    with reply:
        try:
            lock.release()
        except AssertionError:
            reply.send("rejected")


def interleave_updates(
    scalar: Synchronized, array: SynchronizedArray, control: Connection
) -> None:
    with control:
        scalar_before = scalar.value
        array_before = array[0]
        control.send("read")
        if receive(control) != "updated":
            raise ValueError("probe did not update shared values")
        scalar.value = scalar_before + 1
        array[0] = array_before + 1
        control.send("unguarded complete")
        if receive(control) != "guarded":
            raise ValueError("probe never enabled guarded update")
        with scalar.get_lock(), array.get_lock():
            scalar.value += 1
            array[0] += 1


def no_op(value: object) -> None:
    pass


def unpickle_failure() -> object:
    raise ValueError("reconstruction failed")


class SerializationFailure:
    def __reduce__(self) -> object:
        raise TypeError("serialization failed")


class ReconstructionFailure:
    def __reduce__(self) -> object:
        return (unpickle_failure, ())


@unittest.skipUnless(PROFILE, "requires CPython 3.13.16 on Linux x86_64")
class Phase10AsyncConformance(unittest.IsolatedAsyncioTestCase):
    async def cancel(self, task: asyncio.Task[object]) -> None:
        task.cancel()
        with self.assertRaises(asyncio.CancelledError):
            await asyncio.wait_for(task, TIMEOUT)

    async def test_queue_counter_allows_early_ack_and_rejects_excess(self) -> None:
        work: asyncio.Queue[int] = asyncio.Queue(1)
        await work.put(7)
        with self.assertRaises(asyncio.QueueFull):
            work.put_nowait(8)
        work.task_done()
        await asyncio.wait_for(work.join(), TIMEOUT)
        self.assertEqual(work.get_nowait(), 7)
        with self.assertRaises(ValueError):
            work.task_done()
        with self.assertRaises(asyncio.QueueEmpty):
            work.get_nowait()

    async def test_queue_join_returns_after_readmission(self) -> None:
        work: asyncio.Queue[int] = asyncio.Queue()
        await asyncio.wait_for(work.join(), TIMEOUT)
        work.put_nowait(7)
        waiter = await started(work.join())
        self.assertFalse(waiter.done())
        self.assertEqual(work.get_nowait(), 7)
        work.task_done()
        work.put_nowait(7)
        await asyncio.wait_for(waiter, TIMEOUT)
        later_join = await started(work.join())
        self.assertFalse(later_join.done())
        self.assertEqual(work.get_nowait(), 7)
        work.task_done()
        await asyncio.wait_for(later_join, TIMEOUT)

    async def test_queue_cancelled_waits_preserve_admission_and_receipt(self) -> None:
        work: asyncio.Queue[int] = asyncio.Queue(1)
        work.put_nowait(7)
        putting = await started(work.put(8))
        await self.cancel(putting)
        self.assertEqual(work.get_nowait(), 7)
        work.task_done()
        getting = await started(work.get())
        work.put_nowait(9)
        await self.cancel(getting)
        joining = await started(work.join())
        await self.cancel(joining)
        self.assertEqual(work.get_nowait(), 9)
        work.task_done()
        await asyncio.wait_for(work.join(), TIMEOUT)

    async def test_queue_cancel_after_receipt_needs_actual_cleanup(self) -> None:
        for settle in (False, True):
            with self.subTest(settle=settle):
                work: asyncio.Queue[int] = asyncio.Queue()
                work.put_nowait(7)
                received = asyncio.Event()

                async def consume() -> None:
                    self.assertEqual(await work.get(), 7)
                    received.set()
                    try:
                        await asyncio.Future[None]()
                    finally:
                        if settle:
                            work.task_done()

                task = asyncio.create_task(consume())
                await asyncio.wait_for(received.wait(), TIMEOUT)
                await self.cancel(task)
                waiter = await started(work.join())
                self.assertEqual(waiter.done(), settle)
                if not settle:
                    work.task_done()
                await asyncio.wait_for(waiter, TIMEOUT)

    async def test_queue_shutdown_graceful_and_immediate(self) -> None:
        for immediate in (False, True):
            with self.subTest(immediate=immediate):
                work: asyncio.Queue[int] = asyncio.Queue(1)
                work.put_nowait(7)
                producer = await started(work.put(8))
                joining = await started(work.join())
                work.shutdown(immediate)
                with self.assertRaises(asyncio.QueueShutDown):
                    await asyncio.wait_for(producer, TIMEOUT)
                with self.assertRaises(asyncio.QueueShutDown):
                    work.put_nowait(9)
                if not immediate:
                    self.assertFalse(joining.done())
                    self.assertEqual(work.get_nowait(), 7)
                    work.task_done()
                await asyncio.wait_for(joining, TIMEOUT)
                with self.assertRaises(asyncio.QueueShutDown):
                    work.get_nowait()
                empty: asyncio.Queue[int] = asyncio.Queue()
                timed_get = await started(empty.get())
                empty.shutdown(immediate)
                with self.assertRaises(asyncio.QueueShutDown):
                    await asyncio.wait_for(timed_get, TIMEOUT)

    async def test_immediate_shutdown_keeps_checked_out_item_pending(self) -> None:
        work: asyncio.Queue[int] = asyncio.Queue()
        work.put_nowait(7)
        work.put_nowait(8)
        self.assertEqual(work.get_nowait(), 7)
        joining = await started(work.join())
        work.shutdown(immediate=True)
        self.assertTrue(work.empty())
        self.assertFalse(joining.done())
        work.task_done()
        await asyncio.wait_for(joining, TIMEOUT)
        with self.assertRaises(ValueError):
            work.task_done()

    async def test_lock_cancellation_and_synchronous_exit(self) -> None:
        lock = asyncio.Lock()
        alias = lock
        async with lock:
            pending = await started(alias.acquire())
            await self.cancel(pending)
            self.assertTrue(lock.locked())
        self.assertFalse(alias.locked())
        with self.assertRaises(RuntimeError):
            alias.release()

        async def hold() -> None:
            async with alias:
                await asyncio.Future[None]()

        body = await started(hold())
        self.assertTrue(lock.locked())
        await self.cancel(body)
        self.assertFalse(lock.locked())

    async def test_semaphore_cancellation_before_and_after_reservation(self) -> None:
        for primitive in (asyncio.Semaphore, asyncio.BoundedSemaphore):
            with self.subTest(primitive=primitive.__name__):
                permit = primitive(1)
                await permit.acquire()
                blocked = await started(permit.acquire())
                await self.cancel(blocked)
                self.assertTrue(permit.locked())
                first = await started(permit.acquire())
                second = await started(permit.acquire())
                permit.release()
                await self.cancel(first)
                self.assertTrue(await asyncio.wait_for(second, TIMEOUT))
                self.assertTrue(permit.locked())
                permit.release()
                self.assertFalse(permit.locked())

    async def test_semaphore_and_bounded_semaphore_release_limits(self) -> None:
        permit = asyncio.Semaphore(1)
        permit.release()
        self.assertTrue(await asyncio.wait_for(permit.acquire(), TIMEOUT))
        self.assertTrue(await asyncio.wait_for(permit.acquire(), TIMEOUT))
        bounded = asyncio.BoundedSemaphore(1)
        with self.assertRaises(ValueError):
            bounded.release()

    async def test_condition_reacquires_before_cancelled_exit(self) -> None:
        lock = asyncio.Lock()
        condition = asyncio.Condition(lock)
        with self.assertRaises(RuntimeError):
            await condition.wait()
        with self.assertRaises(RuntimeError):
            condition.notify()

        async def wait_for_condition() -> None:
            async with condition:
                try:
                    await condition.wait()
                except asyncio.CancelledError:
                    self.assertTrue(lock.locked())
                    raise

        waiter = await started(wait_for_condition())
        self.assertFalse(lock.locked())
        async with lock:
            waiter.cancel()
            await asyncio.sleep(0)
            self.assertFalse(waiter.done())
        with self.assertRaises(asyncio.CancelledError):
            await asyncio.wait_for(waiter, TIMEOUT)
        self.assertFalse(lock.locked())

        async def notified() -> tuple[bool, bool]:
            async with condition:
                signaled = await condition.wait()
                return signaled, lock.locked()

        waiting = await started(notified())
        async with lock:
            condition.notify_all()
            self.assertFalse(waiting.done())
        self.assertEqual(await asyncio.wait_for(waiting, TIMEOUT), (True, True))
        self.assertFalse(lock.locked())

    async def test_event_waiter_observes_signal_and_clear_is_separate(self) -> None:
        event = asyncio.Event()
        waiter = await started(event.wait())
        event.set()
        event.clear()
        self.assertTrue(await asyncio.wait_for(waiter, TIMEOUT))
        self.assertFalse(event.is_set())
        new_waiter = await started(event.wait())
        self.assertFalse(new_waiter.done())
        await self.cancel(new_waiter)

    async def test_task_group_queue_accounting_and_worker_result_are_distinct(
        self,
    ) -> None:
        work: asyncio.Queue[object] = asyncio.Queue()
        payload = object()
        finish = asyncio.Event()

        async def consume() -> object:
            item = await work.get()
            work.task_done()
            await finish.wait()
            return item

        work.put_nowait(payload)
        async with asyncio.TaskGroup() as group:
            worker = group.create_task(consume())
            try:
                await asyncio.wait_for(work.join(), TIMEOUT)
                self.assertFalse(worker.done())
            finally:
                finish.set()
        self.assertIs(worker.result(), payload)


@unittest.skipUnless(PROFILE, "requires CPython 3.13.16 on Linux x86_64")
class Phase10ProcessConformance(unittest.TestCase):
    def finish_feeder(self, work: Queue) -> None:
        work.close()
        self.wait_for_thread(work.join_thread)

    def wait_for_thread(self, operation: Callable[[], None]) -> None:
        outcome: queue.Queue[BaseException | None] = queue.Queue()

        def run() -> None:
            try:
                operation()
            except BaseException as error:
                outcome.put(error)
            else:
                outcome.put(None)

        threading.Thread(target=run, daemon=True).start()
        completed = outcome.get(timeout=TIMEOUT)
        if completed is not None:
            raise completed

    def test_queue_feeder_failure_and_completion_are_distinct(self) -> None:
        context = multiprocessing.get_context("spawn")
        for fail_to_send in (False, True):
            with self.subTest(fail_to_send=fail_to_send):
                work = context.Queue()
                failures: list[BaseException] = []
                failure_seen = threading.Event()

                def error(error: BaseException, payload: object) -> None:
                    failures.append(error)
                    failure_seen.set()

                def send(payload: bytes) -> None:
                    raise BrokenPipeError("simulated feeder transport failure")

                work._on_queue_feeder_error = error
                if fail_to_send:
                    work._send_bytes = send
                try:
                    with self.assertRaises(AssertionError):
                        work.join_thread()
                    work.put(7 if fail_to_send else SerializationFailure())
                    self.assertTrue(failure_seen.wait(TIMEOUT))
                    with self.assertRaises(queue.Empty):
                        work.get_nowait()
                finally:
                    self.finish_feeder(work)
                self.assertIsInstance(
                    failures[0], BrokenPipeError if fail_to_send else TypeError
                )
                with self.assertRaises(ValueError):
                    work.put(8)
                with self.assertRaises(ValueError):
                    work.get()

        unused = context.Queue()
        unused.cancel_join_thread()
        self.finish_feeder(unused)

    def test_cancel_join_thread_can_return_while_feeder_is_live(self) -> None:
        work = multiprocessing.get_context("spawn").Queue()
        entered = threading.Event()
        allowed = threading.Event()
        original_send = work._send_bytes

        def gated_send(payload: bytes) -> None:
            entered.set()
            if not allowed.wait(TIMEOUT):
                raise TimeoutError("probe never released sender")
            original_send(payload)

        work._send_bytes = gated_send
        try:
            work.put(7)
            self.assertTrue(entered.wait(TIMEOUT))
            work.cancel_join_thread()
            self.finish_feeder(work)
            self.assertTrue(work._thread.is_alive())
        finally:
            allowed.set()
            work.close()
            if work._thread is not None:
                work._thread.join(TIMEOUT)
        self.assertFalse(work._thread.is_alive())

    def test_queue_feeder_can_finish_before_child_receipt(self) -> None:
        context = multiprocessing.get_context("spawn")
        work = context.Queue()
        allowed = context.Event()
        reader, writer = context.Pipe(duplex=False)
        child = context.Process(target=receive_after_gate, args=(work, allowed, writer))
        with reader, writer:
            child.start()
            self.addCleanup(close_process, child)
            writer.close()
            try:
                work.put((7, 8))
                self.finish_feeder(work)
                self.assertTrue(child.is_alive())
                self.assertFalse(reader.poll(0))
            finally:
                allowed.set()
                work.close()
            self.assertEqual(receive(reader), (7, 8))
            child.join(TIMEOUT)
            self.assertEqual(child.exitcode, 0)

    def test_joinable_queue_failed_serialization_keeps_unfinished_count(self) -> None:
        context = multiprocessing.get_context("spawn")
        work = context.JoinableQueue()
        failed = threading.Event()

        def error(error: BaseException, payload: object) -> None:
            failed.set()

        work._on_queue_feeder_error = error
        try:
            work.put(SerializationFailure())
            self.assertTrue(failed.wait(TIMEOUT))
            with self.assertRaises(queue.Empty):
                work.get_nowait()
            self.assertFalse(work._unfinished_tasks._semlock._is_zero())
            work.task_done()
            self.wait_for_thread(work.join)
            with self.assertRaises(ValueError):
                work.task_done()
        finally:
            self.finish_feeder(work)

    def test_joinable_queue_join_returns_after_readmission(self) -> None:
        context = multiprocessing.get_context("spawn")
        work = context.JoinableQueue()
        waiter_entered = threading.Event()
        waiter_returned = threading.Event()
        original_wait = work._cond.wait

        def observed_condition_wait() -> bool:
            waiter_entered.set()
            return original_wait()

        def join() -> None:
            work.join()
            waiter_returned.set()

        work._cond.wait = observed_condition_wait
        try:
            self.wait_for_thread(work.join)
            work.put(7)
            self.assertEqual(work.get(timeout=TIMEOUT), 7)
            threading.Thread(target=join, daemon=True).start()
            self.assertTrue(waiter_entered.wait(TIMEOUT))
            with work._cond:
                work.task_done()
                work.put(7)
            self.assertTrue(waiter_returned.wait(TIMEOUT))
            self.assertFalse(work._unfinished_tasks._semlock._is_zero())
            self.assertEqual(work.get(timeout=TIMEOUT), 7)
            work.task_done()
            self.wait_for_thread(work.join)
        finally:
            self.finish_feeder(work)

    def test_queue_flush_and_accounting_are_distinct_from_receipt_and_exit(
        self,
    ) -> None:
        context = multiprocessing.get_context("spawn")
        work = context.JoinableQueue()
        finishing = context.Event()
        reader, writer = context.Pipe(duplex=False)
        child = context.Process(target=process_items, args=(work, writer, finishing))
        with reader, writer:
            child.start()
            self.addCleanup(close_process, child)
            writer.close()
            try:
                work.put((7, 8))
                self.wait_for_thread(work.join)
                self.assertTrue(child.is_alive())
                self.assertFalse(reader.poll(0))
            finally:
                finishing.set()
                self.finish_feeder(work)
            child.join(TIMEOUT)
            self.assertEqual(child.exitcode, 0)
            self.assertEqual(receive(reader), ((7, 8), True))

    def test_simple_queue_serialization_reconstruction_and_close(self) -> None:
        work = multiprocessing.get_context("spawn").SimpleQueue()
        try:
            with self.assertRaisesRegex(TypeError, "serialization failed"):
                work.put(SerializationFailure())
            self.assertTrue(work.empty())
            work.put(ReconstructionFailure())
            with self.assertRaisesRegex(ValueError, "reconstruction failed"):
                work.get()
            work.put((7, 8))
            self.assertTrue(work._reader.poll(TIMEOUT))
            self.assertEqual(work.get(), (7, 8))
        finally:
            work.close()
        with self.assertRaises(OSError):
            work.put(9)
        with self.assertRaises(OSError):
            work.get()

    def test_pipe_poll_reports_eof_only_after_last_writer_closes(self) -> None:
        context = multiprocessing.get_context("spawn")
        reader, writer = context.Pipe(duplex=False)
        with reader, writer:
            child = context.Process(target=close_writer, args=(writer,))
            child.start()
            self.addCleanup(close_process, child)
            child.join(TIMEOUT)
            self.assertEqual(child.exitcode, 0)
            self.assertFalse(reader.poll(0))
            writer.close()
            self.assertTrue(reader.poll(TIMEOUT))
            with self.assertRaises(EOFError):
                reader.recv()

    def test_pipe_direction_serialization_close_and_broken_write(self) -> None:
        context = multiprocessing.get_context("spawn")
        receive_only, send_only = context.Pipe(duplex=False)
        with receive_only, send_only:
            with self.assertRaises(OSError):
                receive_only.send(7)
            with self.assertRaises(OSError):
                send_only.recv()
            with self.assertRaises(TypeError):
                send_only.send(SerializationFailure())
            self.assertFalse(receive_only.poll(0))
            receive_only.close()
            with self.assertRaises(BrokenPipeError):
                send_only.send(7)
        left, right = context.Pipe()
        with left, right:
            left.send((7, 8))
            self.assertEqual(receive(right), (7, 8))
            right.send((8, 7))
            self.assertEqual(receive(left), (8, 7))
        with self.assertRaises(OSError):
            left.poll(0)

    def test_spawn_direct_script_and_package_main_are_distinct(self) -> None:
        entry = (
            "from multiprocessing import get_context\n"
            "def worker():\n"
            '    print("worker body", flush=True)\n'
            'if __name__ == "__main__":\n'
            '    child = get_context("spawn").Process(target=worker)\n'
            "    child.start()\n"
            '    print("parent start returned", flush=True)\n'
            "    child.join(8)\n"
            "    if child.is_alive():\n"
            "        child.kill()\n"
            "        child.join(8)\n"
            "    raise SystemExit(child.exitcode)\n"
        )
        with TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "script.py").write_text(entry, encoding="utf-8")
            (root / "entry").mkdir()
            (root / "entry" / "__init__.py").touch()
            (root / "entry" / "__main__.py").write_text(entry, encoding="utf-8")
            direct = subprocess.run(
                [sys.executable, "script.py"],
                cwd=root,
                capture_output=True,
                text=True,
                timeout=TIMEOUT,
                check=False,
            )
            packaged = subprocess.run(
                [sys.executable, "-m", "entry"],
                cwd=root,
                capture_output=True,
                text=True,
                timeout=TIMEOUT,
                check=False,
            )
        self.assertEqual(direct.returncode, 0, direct.stderr)
        self.assertIn("worker body", direct.stdout)
        self.assertIn("parent start returned", packaged.stdout)
        self.assertNotIn("worker body", packaged.stdout)
        self.assertEqual(packaged.returncode, 1, packaged.stderr)
        self.assertIn("Can't get attribute 'worker'", packaged.stderr)

    def test_spawn_rejects_local_target_and_incompatible_context_before_launch(
        self,
    ) -> None:
        context = multiprocessing.get_context("spawn")
        local = context.Process(target=lambda: None)
        with self.assertRaises(AttributeError):
            local.start()
        self.assertIsNone(local.pid)
        local.close()
        wrong_context = multiprocessing.get_context("fork").Lock()
        mismatched = context.Process(target=no_op, args=(wrong_context,))
        with self.assertRaisesRegex(RuntimeError, "SemLock.*fork context"):
            mismatched.start()
        self.assertIsNone(mismatched.pid)
        mismatched.close()

    def test_partial_lock_acquisition_and_recursive_owner(self) -> None:
        context = multiprocessing.get_context("spawn")
        first = context.Lock()
        second = context.Lock()
        with second:
            with first:
                self.assertFalse(second.acquire(block=False))
            self.assertTrue(first.acquire(block=False))
            first.release()
            self.assertFalse(second.acquire(block=False))
        with self.assertRaises(ValueError):
            first.release()

        recursive = context.RLock()
        reader, writer = context.Pipe(duplex=False)
        with recursive, reader, writer:
            self.assertTrue(recursive.acquire(block=False))
            recursive.release()
            child = context.Process(
                target=reject_lock_release, args=(recursive, writer)
            )
            child.start()
            self.addCleanup(close_process, child)
            writer.close()
            self.assertEqual(receive(reader), "rejected")
            child.join(TIMEOUT)
            self.assertEqual(child.exitcode, 0)

    def test_process_semaphore_bounds(self) -> None:
        context = multiprocessing.get_context("spawn")
        ordinary = context.Semaphore(1)
        ordinary.release()
        self.assertTrue(ordinary.acquire(block=False))
        self.assertTrue(ordinary.acquire(block=False))
        bounded = context.BoundedSemaphore(1)
        with self.assertRaises(ValueError):
            bounded.release()

    def test_value_and_array_require_a_guard_across_compound_work(self) -> None:
        context = multiprocessing.get_context("spawn")
        shared = context.Value("i", 0)
        guard = context.RLock()
        values = context.Array("i", [0], lock=guard)
        parent, worker = context.Pipe()
        child = context.Process(
            target=interleave_updates, args=(shared, values, worker)
        )
        with parent, worker:
            child.start()
            self.addCleanup(close_process, child)
            worker.close()
            self.assertEqual(receive(parent), "read")
            shared.value += 1
            values[0] += 1
            parent.send("updated")
            self.assertEqual(receive(parent), "unguarded complete")
            self.assertEqual((shared.value, values[0]), (1, 1))
            self.assertIs(values.get_lock(), guard)
            parent.send("guarded")
            with shared.get_lock(), guard:
                shared.value += 1
                values[0] += 1
            child.join(TIMEOUT)
            self.assertEqual(child.exitcode, 0)
        self.assertEqual((shared.value, values[0]), (3, 3))
