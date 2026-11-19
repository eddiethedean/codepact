"""Conformance and source-analysis cases for the pinned async stream providers."""

import asyncio
import importlib
import importlib.metadata
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import sqlalchemy as sa
from sqlalchemy.ext import asyncio as async_sa
from sqlmodel.ext.asyncio.session import AsyncSession as ModelSession

from phase09_provider_probe import Item, serving_http


class HttpStreaming(unittest.IsolatedAsyncioTestCase):
    async def test_buffered_body_and_lexical_stream(self) -> None:
        with serving_http() as url:
            for name in ("httpx", "httpx2"):
                with self.subTest(provider=name):
                    api = importlib.import_module(name)
                    async with api.AsyncClient(trust_env=False) as client:
                        async with client.stream("GET", url) as response:
                            self.assertEqual(await response.aread(), b"phase 0.9")
                        self.assertEqual(await response.aread(), b"phase 0.9")
                        async with client.stream("GET", url) as response:
                            async for chunk in response.aiter_bytes():
                                self.assertTrue(chunk)
                                break
                        self.assertTrue(response.is_closed)
                        with self.assertRaises((api.StreamClosed, api.StreamConsumed)):
                            await response.aread()

    async def test_failed_or_cancelled_close_skips_retry_and_context_exit(self) -> None:
        for name in ("httpx", "httpx2"):
            for interrupted in (False, True):
                with self.subTest(provider=name, interrupted=interrupted):
                    api = importlib.import_module(name)
                    started = asyncio.Event()
                    release = asyncio.Event()

                    class GatedStream(api.AsyncByteStream):
                        attempts = 0
                        released = False

                        async def aclose(self) -> None:
                            self.attempts += 1
                            started.set()
                            if interrupted:
                                await release.wait()
                                self.released = True
                            else:
                                raise OSError("injected transport close failure")

                    stream = GatedStream()
                    transport = api.MockTransport(
                        lambda _: api.Response(200, stream=stream)
                    )
                    async with api.AsyncClient(transport=transport) as client:
                        context = client.stream("GET", "http://fixture.invalid")
                        async with context as response:
                            closing = asyncio.create_task(response.aclose())
                            try:
                                await asyncio.wait_for(started.wait(), timeout=5)
                                self.assertTrue(response.is_closed)
                                if interrupted:
                                    closing.cancel()
                                failure = (
                                    asyncio.CancelledError if interrupted else OSError
                                )
                                with self.assertRaises(failure):
                                    await closing
                                await response.aclose()
                            finally:
                                release.set()
                        self.assertEqual(stream.attempts, 1)
                        self.assertFalse(stream.released)


class SqlStreaming(unittest.IsolatedAsyncioTestCase):
    async def test_buffered_rows_and_both_async_result_families(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "profiles.sqlite"
            engine = async_sa.create_async_engine(f"sqlite+aiosqlite:///{path}")
            try:
                async with engine.begin() as connection:
                    await connection.run_sync(Item.metadata.create_all)
                    await connection.execute(
                        sa.insert(Item),
                        [{"id": 1, "value": "a"}, {"id": 2, "value": "b"}],
                    )
                query = sa.select(Item.value).order_by(Item.id)
                for session_type in (async_sa.AsyncSession, ModelSession):
                    with self.subTest(session=session_type):
                        async with session_type(engine) as session:
                            buffered = await session.scalars(query)
                            self.assertEqual(buffered.all(), ["a", "b"])

                            rows = await session.stream(query)
                            self.assertIsInstance(rows, async_sa.AsyncResult)
                            self.assertEqual(
                                [tuple(row) async for row in rows], [("a",), ("b",)]
                            )
                            self.assertEqual([row async for row in rows], [])

                            values = await session.stream_scalars(query)
                            self.assertIsInstance(values, async_sa.AsyncScalarResult)
                            async for value in values:
                                self.assertEqual(value, "a")
                                break
                            self.assertFalse(values.closed)
                            await values.close()
                            self.assertTrue(values.closed)
            finally:
                await engine.dispose()

    async def test_cancelled_direct_result_close_can_be_retried(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "profiles.sqlite"
            engine = async_sa.create_async_engine(f"sqlite+aiosqlite:///{path}")
            try:
                for session_type in (async_sa.AsyncSession, ModelSession):
                    with self.subTest(session=session_type):
                        async with session_type(engine) as session:
                            result = await session.stream(sa.text("select 1"))
                            started = asyncio.Event()
                            release = asyncio.Event()
                            spawn = async_sa.result.greenlet_spawn

                            async def pause_close(
                                *args: object, **kwargs: object
                            ) -> object:
                                started.set()
                                await release.wait()
                                return await spawn(*args, **kwargs)

                            with patch.object(
                                async_sa.result, "greenlet_spawn", pause_close
                            ):
                                closing = asyncio.create_task(result.close())
                                try:
                                    await asyncio.wait_for(started.wait(), timeout=5)
                                    closing.cancel()
                                    with self.assertRaises(asyncio.CancelledError):
                                        await closing
                                finally:
                                    release.set()
                            self.assertFalse(result.closed)
                            await result.close()
                            self.assertTrue(result.closed)
            finally:
                await engine.dispose()


SOURCE = """from asyncio import timeout
from httpx import AsyncClient
from httpx2 import AsyncClient as OtherClient
from sqlalchemy import text
from sqlalchemy.ext.asyncio import AsyncSession, create_async_engine
from sqlmodel.ext.asyncio.session import AsyncSession as ModelSession

async def buffered_http():
    async with AsyncClient(trust_env=False) as client:
        async with client.stream("GET", "http://127.0.0.1") as response:
            await response.aread()
        await response.aread()

async def http_close_retry():
    async with OtherClient(trust_env=False) as client:
        async with client.stream("GET", "http://127.0.0.1") as response:
            try:
                await response.aclose()
            except BaseException:
                await response.aclose()

async def http_iterated():
    async with OtherClient(trust_env=False) as client:
        async with client.stream("GET", "http://127.0.0.1") as response:
            async for chunk in response.aiter_bytes():
                break

async def timed_out_close():
    async with AsyncClient(trust_env=False) as client:
        async with client.stream("GET", "http://127.0.0.1") as response:
            try:
                async with timeout(1):
                    await response.aclose()
            except TimeoutError:
                await response.aclose()  # timed out transport close

async def read_retry_after_failure():
    async with OtherClient(trust_env=False) as client:
        async with client.stream("GET", "http://127.0.0.1") as response:
            try:
                await response.aread()
            except BaseException:
                await response.aread()  # failed read consumed the stream

async def sql_rows():
    engine = create_async_engine("sqlite+aiosqlite:///fixture.sqlite")
    try:
        async with AsyncSession(engine) as session:
            rows = await session.stream(text("select 1"))
            try:
                async for row in rows:
                    pass
            finally:
                await rows.close()
    finally:
        await engine.dispose()

async def sql_concurrent_active():
    engine = create_async_engine("sqlite+aiosqlite:///fixture.sqlite")
    try:
        async with ModelSession(engine) as session:
            values = await session.stream_scalars(text("select 1"))
            try:
                async for value in values:
                    break
                await session.execute(text("select 2"))
            finally:
                await values.close()
    finally:
        await engine.dispose()
"""


def run_checker(executable: str, dependencies: Path | None = None) -> None:
    root = (
        dependencies
        or Path(importlib.metadata.distribution("httpx").locate_file("")).resolve()
    )
    with tempfile.TemporaryDirectory() as directory:
        project = Path(directory)
        (project / "pyproject.toml").write_text(
            f"""[tool.codepact]
config-version = 1
python-version = "3.13"
include = ["caller.py"]
experimental-foundations = true
experimental-ownership = true
experimental-advanced-contracts = true
assume-default-task-factory = true
[tool.codepact.dependencies]
roots = [{json.dumps(str(root))}]
[tool.codepact.runtime-profile]
implementation = "cpython"
python-version = "3.13.16"
os = "linux"
architecture = "x86_64"
event-loop = "default"
""",
            encoding="utf-8",
        )
        (project / "caller.py").write_text(SOURCE, encoding="utf-8")
        result = subprocess.run(
            [executable, "check", "--json"],
            cwd=project,
            capture_output=True,
            text=True,
            timeout=60,
        )
        if result.returncode not in (0, 1):
            raise AssertionError(result.stdout or result.stderr)
        report = json.loads(result.stdout)
        applicable = {
            use["symbol"]
            for use in report["foundations"]["model_uses"]
            if use["applicable"]
        }
        expected = {
            "httpx.AsyncClient.stream",
            "httpx.Response.aread",
            "httpx2.Response.aclose",
            "httpx2.Response.aiter_bytes",
            "sqlalchemy.AsyncSession.stream",
            "sqlalchemy.AsyncResult.__anext__",
            "sqlalchemy.AsyncResult.close",
            "sqlmodel.AsyncSession.stream_scalars",
            "sqlmodel.AsyncScalarResult.__anext__",
            "sqlmodel.AsyncScalarResult.close",
        }
        if missing := expected - applicable:
            raise AssertionError((missing, report["foundations"]["provider_issues"]))
        if not any(
            item["code"] == "TP2006" and "live async stream" in item["message"]
            for item in report["diagnostics"]
        ):
            raise AssertionError(report["diagnostics"])
        for marker, message in (
            ("timed out transport close", "transport release remains unresolved"),
            (
                "failed read consumed the stream",
                "streaming response may already be consumed",
            ),
        ):
            line = next(
                n
                for n, source in enumerate(SOURCE.splitlines(), 1)
                if source.endswith(marker)
            )
            if not any(
                item["primary"]["line"] == line and message in item["message"]
                for item in report["diagnostics"]
            ):
                raise AssertionError((marker, report["coverage"]))
        print(
            json.dumps({"phase11_reviewed_cli_symbols": sorted(applicable)}),
            flush=True,
        )


if __name__ == "__main__":
    if checker := os.environ.get("CODEPACT_PHASE11_CHECKER"):
        run_checker(checker)
    unittest.main(verbosity=2)
