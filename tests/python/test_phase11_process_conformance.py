"""CPython conformance for bounded process shutdown and shared memory."""

from __future__ import annotations

import errno
import multiprocessing
from multiprocessing import resource_tracker, shared_memory
from multiprocessing.connection import Connection
from multiprocessing.process import BaseProcess
from multiprocessing.queues import JoinableQueue
import os
from pathlib import Path
import platform
import signal
import subprocess
import sys
import unittest
from unittest.mock import patch


PROFILE = (
    sys.implementation.name == "cpython"
    and platform.python_version() == "3.13.16"
    and sys.platform == "linux"
    and platform.machine() == "x86_64"
)


def _respond_to_stop(command: Connection, reply: Connection) -> None:
    try:
        if command.poll(10) and command.recv() == "stop":
            reply.send("clean exit")
    finally:
        command.close()
        reply.close()


def _ignore_terminate(started: Connection, release: Connection) -> None:
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
    with started, release:
        started.send("ready")
        release.poll(10)


def _hold_shared_mapping(name: str, parent: Connection, release: Connection) -> None:
    with parent, release, _mapped(name) as mapping:
        parent.send(mapping.buf[0])
        if not release.poll(10) or release.recv() != "release":
            raise TimeoutError("shared-memory probe did not receive release")
        parent.send(mapping.buf[0])


def _exit_without_reply() -> None:
    os._exit(7)


def _read_without_acknowledging(
    queue: JoinableQueue, reached: Connection, release: Connection
) -> None:
    with reached, release:
        queue.get(timeout=5)
        reached.send("received")
        release.poll(10)
        # Leave task_done outstanding even if the worker times out.


def _work_result(value: int) -> int:
    return value + 1


def _pool_drain_result() -> int:
    pool = multiprocessing.get_context("spawn").Pool(1)
    try:
        result = pool.apply_async(_work_result, (41,))
        pool.close()
        result.wait(5)
        if not result.ready():
            raise TimeoutError("pool did not finish its admitted job")
        pool.join()
        return result.get(timeout=0)
    finally:
        pool.terminate()


class _mapped:
    def __init__(self, name: str) -> None:
        self.mapping = shared_memory.SharedMemory(name)

    def __enter__(self) -> shared_memory.SharedMemory:
        return self.mapping

    def __exit__(self, *_: object) -> None:
        self.mapping.close()


def _clean_process(child: BaseProcess) -> None:
    if child.pid is not None:
        if child.is_alive():
            child.kill()
        child.join(5)
    child.close()


def _clean_segment(mapping: shared_memory.SharedMemory) -> None:
    try:
        mapping.close()
    finally:
        try:
            mapping.unlink()
        except FileNotFoundError:
            pass


@unittest.skipUnless(PROFILE, "requires CPython 3.13.16 on Linux x86_64")
class Phase11ProcessConformance(unittest.TestCase):
    def setUp(self) -> None:
        self.ctx = multiprocessing.get_context("spawn")

    def pipe(self) -> tuple[Connection, Connection]:
        receiver, sender = self.ctx.Pipe(duplex=False)
        self.addCleanup(receiver.close)
        self.addCleanup(sender.close)
        return receiver, sender

    def test_partial_start_preserves_first_worker_and_reply(self) -> None:
        receive_cmd, send_cmd = self.pipe()
        receive_reply, send_reply = self.pipe()
        child = self.ctx.Process(
            target=_respond_to_stop, args=(receive_cmd, send_reply)
        )
        invalid = self.ctx.Process(target=lambda: None)
        self.addCleanup(_clean_process, child)
        self.addCleanup(_clean_process, invalid)
        child.start()
        receive_cmd.close()
        send_reply.close()
        with self.assertRaises((AttributeError, TypeError)):
            invalid.start()
        self.assertIsNone(invalid.pid)
        send_cmd.send("stop")
        self.assertTrue(receive_reply.poll(5))
        self.assertEqual(receive_reply.recv(), "clean exit")
        child.join(5)
        self.assertEqual(child.exitcode, 0)

    def test_start_return_and_crashed_worker_exit_are_distinct(self) -> None:
        child = self.ctx.Process(target=_exit_without_reply)
        self.addCleanup(_clean_process, child)
        child.start()
        self.assertIsNotNone(child.pid)
        child.join(5)
        self.assertEqual(child.exitcode, 7)

    def test_terminate_request_and_reaped_exit_are_distinct(self) -> None:
        started, send_started = self.pipe()
        release, _send_release = self.pipe()
        child = self.ctx.Process(target=_ignore_terminate, args=(send_started, release))
        self.addCleanup(_clean_process, child)
        child.start()
        send_started.close()
        release.close()
        self.assertTrue(started.poll(5))
        self.assertEqual(started.recv(), "ready")
        child.terminate()
        child.join(0)
        self.assertIsNone(child.exitcode)
        child.kill()
        child.join(5)
        self.assertEqual(child.exitcode, -signal.SIGKILL)

    def test_forced_worker_exit_does_not_acknowledge_ipc(self) -> None:
        queue = self.ctx.JoinableQueue()
        reached, send_reached = self.pipe()
        release, _send_release = self.pipe()
        child = self.ctx.Process(
            target=_read_without_acknowledging, args=(queue, send_reached, release)
        )
        self.addCleanup(queue.close)
        # The single bounded item is already consumed on the asserted path.
        # A failed probe must not try to join an unresolved feeder.
        self.addCleanup(queue.cancel_join_thread)
        self.addCleanup(_clean_process, child)
        child.start()
        send_reached.close()
        release.close()
        queue.put(b"item")
        self.assertTrue(reached.poll(5))
        self.assertEqual(reached.recv(), "received")
        child.terminate()
        child.join(5)
        self.assertEqual(child.exitcode, -signal.SIGTERM)
        # The pinned implementation exposes a shared semaphore for unfinished
        # items. Check it directly to avoid an indefinitely blocked join.
        self.assertFalse(queue._unfinished_tasks._semlock._is_zero())
        queue.task_done()
        self.assertTrue(queue._unfinished_tasks._semlock._is_zero())

    def test_pool_drain_and_result_observation_are_distinct(self) -> None:
        # Pool.join and Pool.terminate have no timeout. An isolated process
        # group lets this probe bound the calls and reap the workers on failure.
        probe = subprocess.Popen(
            [
                sys.executable,
                "-c",
                "from test_phase11_process_conformance import _pool_drain_result; "
                "print(_pool_drain_result())",
            ],
            cwd=Path(__file__).resolve().parent,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            start_new_session=True,
        )
        try:
            stdout, stderr = probe.communicate(timeout=20)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(probe.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            probe.communicate(timeout=5)
            self.fail("pool shutdown exceeded the probe deadline")
        self.assertEqual(probe.returncode, 0, stderr)
        self.assertEqual(stdout.strip(), "42")

    def test_unlink_rejects_later_attach_and_preserves_existing_spawn_mapping(
        self,
    ) -> None:
        creator = shared_memory.SharedMemory(create=True, size=8)
        self.addCleanup(_clean_segment, creator)
        creator.buf[0] = 42
        receive, send = self.pipe()
        release, send_release = self.pipe()
        child = self.ctx.Process(
            target=_hold_shared_mapping, args=(creator.name, send, release)
        )
        self.addCleanup(_clean_process, child)
        child.start()
        send.close()
        release.close()
        self.assertTrue(receive.poll(5))
        self.assertEqual(receive.recv(), 42)
        creator.unlink()
        with self.assertRaises(FileNotFoundError):
            shared_memory.SharedMemory(creator.name)
        creator.buf[0] = 43
        send_release.send("release")
        self.assertTrue(receive.poll(5))
        self.assertEqual(receive.recv(), 43)
        child.join(5)
        self.assertEqual(child.exitcode, 0)

    def test_partial_close_releases_base_and_can_be_retried(self) -> None:
        creator = shared_memory.SharedMemory(create=True, size=8)
        self.addCleanup(_clean_segment, creator)
        base = creator.buf
        exported = base[:1]
        self.addCleanup(exported.release)
        with self.assertRaises(BufferError):
            creator.close()
        self.assertIsNone(creator.buf)
        with self.assertRaises(ValueError):
            _ = base[0]
        self.assertEqual(exported[0], 0)
        exported.release()
        creator.close()
        creator.unlink()

    def test_open_rejection_and_post_open_failure_have_different_name_effects(
        self,
    ) -> None:
        for failed_stage in ("shm_open", "fstat", "mmap"):
            with self.subTest(failed_stage=failed_stage):
                creator = shared_memory.SharedMemory(create=True, size=8)
                try:
                    creator.buf[0] = 42
                    owner, name = {
                        "shm_open": (shared_memory._posixshmem, "shm_open"),
                        "fstat": (shared_memory.os, "fstat"),
                        "mmap": (shared_memory.mmap, "mmap"),
                    }[failed_stage]
                    with (
                        patch.object(
                            resource_tracker,
                            "unregister",
                            wraps=resource_tracker.unregister,
                        ) as unregister,
                        patch.object(
                            owner,
                            name,
                            side_effect=OSError(errno.ENOMEM, "injected mapping error"),
                        ),
                        self.assertRaises(OSError) as error,
                    ):
                        shared_memory.SharedMemory(creator.name)
                    self.assertEqual(error.exception.errno, errno.ENOMEM)
                    self.assertEqual(creator.buf[0], 42)
                    if failed_stage == "shm_open":
                        unregister.assert_not_called()
                        with _mapped(creator.name) as mapping:
                            self.assertEqual(mapping.buf[0], 42)
                    else:
                        unregister.assert_called_once_with(
                            creator._name, "shared_memory"
                        )
                        with self.assertRaises(FileNotFoundError):
                            shared_memory.SharedMemory(creator.name)
                finally:
                    _clean_segment(creator)
