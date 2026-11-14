"""Controlled probes for the pinned CPython 3.13.16 pool model."""

from __future__ import annotations

import concurrent.futures
import sys
import threading
import unittest
from unittest.mock import patch


@unittest.skipUnless(
    sys.implementation.name == "cpython" and sys.version_info[:3] == (3, 13, 16),
    "executor model is pinned to CPython 3.13.16",
)
class ThreadPoolExecutorModelConformance(unittest.TestCase):
    def test_submit_runs_work_and_waiting_shutdown_joins_workers(self) -> None:
        started = threading.Event()

        def worker() -> int:
            started.set()
            return 7

        pool = concurrent.futures.ThreadPoolExecutor()
        future = pool.submit(worker)
        self.assertTrue(started.wait(2))
        self.assertEqual(future.result(timeout=2), 7)
        pool.shutdown(wait=True)
        self.assertTrue(all(thread.is_alive() is False for thread in pool._threads))

    def test_repeated_shutdown_is_accepted_and_submit_after_shutdown_raises(
        self,
    ) -> None:
        pool = concurrent.futures.ThreadPoolExecutor()
        pool.shutdown()
        pool.shutdown()
        with self.assertRaises(RuntimeError):
            pool.submit(lambda: None)

    def test_nonwaiting_shutdown_returns_while_worker_is_still_running(self) -> None:
        started = threading.Event()
        release = threading.Event()

        def worker() -> str:
            started.set()
            if not release.wait(2):
                raise TimeoutError("probe did not release the worker")
            return "finished"

        pool = concurrent.futures.ThreadPoolExecutor()
        future = pool.submit(worker)
        self.assertTrue(started.wait(2))
        pool.shutdown(wait=False)
        self.assertFalse(future.done())
        release.set()
        self.assertEqual(future.result(timeout=2), "finished")
        pool.shutdown(wait=True)

    def test_thread_start_failure_can_leave_work_queued_for_later_execution(
        self,
    ) -> None:
        ran = threading.Event()
        pool = concurrent.futures.ThreadPoolExecutor()

        def worker() -> None:
            ran.set()

        with patch.object(
            threading.Thread, "start", side_effect=RuntimeError("start failed")
        ):
            with self.assertRaisesRegex(RuntimeError, "start failed"):
                pool.submit(worker)

        self.assertGreater(pool._work_queue.qsize(), 0)
        self.assertEqual(pool.submit(lambda: 1).result(timeout=2), 1)
        pool.shutdown(wait=True)
        self.assertTrue(ran.is_set())

    def test_join_failure_after_shutdown_flag_preserves_shutdown_state(self) -> None:
        started = threading.Event()
        release = threading.Event()

        def worker() -> None:
            started.set()
            release.wait(2)

        pool = concurrent.futures.ThreadPoolExecutor()
        future = pool.submit(worker)
        self.assertTrue(started.wait(2))
        thread = next(iter(pool._threads))
        try:
            with patch.object(thread, "join", side_effect=RuntimeError("join failed")):
                with self.assertRaisesRegex(RuntimeError, "join failed"):
                    pool.shutdown(wait=True)
            self.assertTrue(pool._shutdown)
        finally:
            release.set()
        future.result(timeout=2)
        pool.shutdown(wait=True)


if __name__ == "__main__":
    unittest.main()
