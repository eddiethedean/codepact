"""CPython probes for async-generator finalization and asyncio shutdown."""

from __future__ import annotations

import asyncio
from collections.abc import AsyncGenerator
from contextlib import aclosing, asynccontextmanager, suppress
import sys
import unittest
from unittest.mock import patch


class AsyncResourceConformance(unittest.IsolatedAsyncioTestCase):
    async def test_generator_lazy_entry_break_and_exhaustion(self) -> None:
        events: list[str] = []

        async def records() -> AsyncGenerator[int, None]:
            events.append("acquire")
            try:
                yield 1
            finally:
                events.append("release")

        never_started = records()
        await never_started.aclose()
        self.assertEqual(events, [])
        generator = records()
        async for row in generator:
            self.assertEqual(row, 1)
            break
        self.assertEqual(events, ["acquire"])
        await generator.aclose()
        self.assertEqual(events, ["acquire", "release"])
        self.assertEqual([row async for row in records()], [1])
        self.assertEqual(events, ["acquire", "release"] * 2)

    async def test_interrupted_finalizer_and_repeat_close(self) -> None:
        cleanup_started = asyncio.Event()
        release = asyncio.Event()
        events: list[str] = []

        async def records() -> AsyncGenerator[int, None]:
            try:
                yield 1
            finally:
                cleanup_started.set()
                await release.wait()
                events.append("released")

        generator = records()
        self.assertEqual(await anext(generator), 1)
        closing = asyncio.create_task(generator.aclose())
        await asyncio.wait_for(cleanup_started.wait(), 5)
        closing.cancel()
        with self.assertRaises(asyncio.CancelledError):
            await closing
        release.set()
        await generator.aclose()
        self.assertEqual(events, [])

        async with aclosing(records()) as generator:
            self.assertEqual(await anext(generator), 1)
        self.assertEqual(events, ["released"])

    async def test_async_context_entry_failure_and_body_exception(self) -> None:
        events: list[str] = []

        @asynccontextmanager
        async def manager(*, fail_entry: bool) -> AsyncGenerator[None, None]:
            events.append("acquire")
            try:
                if fail_entry:
                    raise RuntimeError("entry")
                yield
            finally:
                events.append("release")

        with self.assertRaisesRegex(RuntimeError, "entry"):
            async with manager(fail_entry=True):
                self.fail("body must not run")
        with self.assertRaisesRegex(ValueError, "body"):
            async with manager(fail_entry=False):
                raise ValueError("body")
        self.assertEqual(events, ["acquire", "release"] * 2)

    @unittest.skipUnless(
        sys.platform == "linux" and sys.version_info >= (3, 13),
        "The selected TCP server model requires Python 3.13+ on Linux",
    )
    async def test_server_transport_and_handler_completion_are_distinct(self) -> None:
        received = asyncio.Event()
        finish_handler = asyncio.Event()
        exited = asyncio.Event()

        async def handler(
            reader: asyncio.StreamReader, writer: asyncio.StreamWriter
        ) -> None:
            try:
                self.assertEqual(await reader.readexactly(3), b"job")
                received.set()
                await finish_handler.wait()
            finally:
                writer.close()
                await writer.wait_closed()
                exited.set()

        server = await asyncio.start_server(handler, "127.0.0.1", 0)
        port = int(server.sockets[0].getsockname()[1])
        reader, writer = await asyncio.open_connection("127.0.0.1", port)
        try:
            writer.write(b"job")
            await writer.drain()
            await asyncio.wait_for(received.wait(), 5)
            server.close()
            self.assertFalse(server.is_serving())
            self.assertFalse(exited.is_set())
            server.close_clients()
            await asyncio.wait_for(server.wait_closed(), 5)
            self.assertEqual(await asyncio.wait_for(reader.read(1), 5), b"")
            self.assertFalse(exited.is_set())
        finally:
            finish_handler.set()
            writer.close()
            await writer.wait_closed()
            server.close()
            await asyncio.wait_for(server.wait_closed(), 5)
            await asyncio.wait_for(exited.wait(), 5)

        # The default constructor starts the listener before its last yield.
        # Capture the server in this controlled probe so it can be reclaimed.
        loop = asyncio.get_running_loop()
        enable_accept = getattr(loop, "_start_serving")
        listeners: list[asyncio.Server] = []

        def cancel_after_listen(*args: object, **kwargs: object) -> None:
            enable_accept(*args, **kwargs)
            listener = args[3]
            assert isinstance(listener, asyncio.Server)
            listeners.append(listener)
            startup.cancel()

        try:
            with patch.object(loop, "_start_serving", side_effect=cancel_after_listen):
                startup = asyncio.create_task(
                    asyncio.start_server(handler, "127.0.0.1", 0)
                )
                with self.assertRaises(asyncio.CancelledError):
                    await asyncio.wait_for(startup, 5)
            self.assertEqual(len(listeners), 1)
            self.assertTrue(listeners[0].is_serving())
        finally:
            for listener in listeners:
                listener.close()
                await asyncio.wait_for(listener.wait_closed(), 5)

    async def test_subprocess_output_wait_and_stop_are_distinct(self) -> None:
        child = await asyncio.create_subprocess_exec(
            sys.executable,
            "-c",
            "import sys; sys.stdout.write('ok'); sys.exit(2)",
            stdout=asyncio.subprocess.PIPE,
        )
        stdout, stderr = await asyncio.wait_for(child.communicate(), 5)
        self.assertEqual((stdout, stderr, child.returncode), (b"ok", None, 2))

        child = await asyncio.create_subprocess_exec(
            sys.executable, "-c", "import time; time.sleep(30)"
        )
        try:
            with self.assertRaises(TimeoutError):
                await asyncio.wait_for(child.wait(), 0)
            self.assertIsNone(child.returncode)
            child.terminate()
            self.assertEqual(await asyncio.wait_for(child.wait(), 5), -15)
        finally:
            if child.returncode is None:
                with suppress(ProcessLookupError):
                    child.kill()
                await asyncio.wait_for(child.wait(), 5)
