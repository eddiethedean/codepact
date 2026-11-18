"""Controlled runtime probes for the installed Phase 0.9 provider profiles."""

import asyncio
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import importlib
import importlib.metadata
import json
import os
from pathlib import Path
import platform
import sqlite3
import shutil
import subprocess
import sysconfig
import tempfile
import threading
import unittest
from unittest.mock import patch

import sqlalchemy as sa
from sqlalchemy.ext import asyncio as async_sa
import sqlmodel
from sqlmodel.ext.asyncio import session as async_model


@contextmanager
def serving_http():
    parallel = threading.Barrier(2, timeout=5)

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            if self.path == "/parallel":
                parallel.wait()
            payload = b"phase 0.9"
            self.send_response(200)
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def log_message(self, *args):
            pass

    with ThreadingHTTPServer(("127.0.0.1", 0), Handler) as server:
        worker = threading.Thread(target=server.serve_forever, daemon=True)
        worker.start()
        try:
            yield f"http://127.0.0.1:{server.server_port}"
        finally:
            server.shutdown()
            worker.join(timeout=5)


class HttpProfiles(unittest.IsolatedAsyncioTestCase):
    async def test_independent_sync_buffers_and_stream_exit(self):
        with serving_http() as url:
            for package in ("httpx", "httpx2"):
                with self.subTest(package=package):
                    api = importlib.import_module(package)
                    with api.Client(trust_env=False) as client:
                        buffered = client.get(url)
                        with client.stream("GET", url) as stream:
                            self.assertFalse(stream.is_closed)
                        self.assertTrue(stream.is_closed)
                        with self.assertRaises(api.StreamClosed):
                            stream.read()
                    self.assertEqual(buffered.read(), b"phase 0.9")
                    with self.assertRaises(RuntimeError):
                        client.get(url)

    async def test_both_clients_permit_live_shared_requests(self):
        with serving_http() as url:
            for package in ("httpx", "httpx2"):
                with self.subTest(package=package):
                    api = importlib.import_module(package)
                    async with api.AsyncClient(trust_env=False) as client:
                        results = await asyncio.wait_for(
                            asyncio.gather(
                                client.get(url + "/parallel"),
                                client.get(url + "/parallel"),
                            ),
                            timeout=8,
                        )
                    self.assertTrue(client.is_closed)
                    self.assertEqual(
                        [result.content for result in results], [b"phase 0.9"] * 2
                    )


class Item(sqlmodel.SQLModel, table=True):
    id: int = sqlmodel.Field(primary_key=True)
    value: str


class SqlProfiles(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        workspace = tempfile.TemporaryDirectory()
        self.addCleanup(workspace.cleanup)
        self.path = str(Path(workspace.name) / "profiles.sqlite")
        self.engine = sa.create_engine("sqlite:///" + self.path)
        self.addCleanup(self.engine.dispose)
        sqlmodel.SQLModel.metadata.create_all(self.engine)
        with self.engine.begin() as connection:
            connection.execute(sa.insert(Item).values(id=1, value="initial"))

    def test_sync_sqlmodel_rows_and_reset_on_close(self):
        for session_type in (sa.orm.Session, sqlmodel.Session):
            with self.subTest(session_type=session_type):
                with session_type(self.engine) as session:
                    session.execute(sa.update(Item).values(value="rolled back"))
                    session.rollback()
                    session.execute(sa.update(Item).values(value="committed"))
                    session.commit()
                self.assertEqual(
                    session.execute(sa.select(Item.value)).scalar_one(), "committed"
                )
                session.close()
        with sqlmodel.Session(self.engine) as session:
            self.assertEqual(
                session.exec(sqlmodel.select(Item.value)).all(), ["committed"]
            )
            self.assertEqual(
                session.exec(sqlmodel.select(Item.id, Item.value)).all(),
                [(1, "committed")],
            )
            result = session.exec(sqlmodel.select(Item.value))
            self.assertEqual(result.all(), ["committed"])
            self.assertEqual(result.all(), [])
            result.close()
            with self.assertRaises(sa.exc.ResourceClosedError):
                result.all()

    def test_etlantic_scope_commit_rollback_base_exception_and_failures(self):
        module = importlib.import_module("etlantic_sqlmodel.control_plane.session")
        with module.session_scope(self.engine) as session:
            session.execute(sa.update(Item).values(value="saved"))
        with self.assertRaises(ValueError):
            with module.session_scope(self.engine) as session:
                session.execute(sa.update(Item).values(value="lost"))
                raise ValueError("body")
        with self.engine.connect() as connection:
            self.assertEqual(
                connection.execute(sa.select(Item.value)).scalar_one(), "saved"
            )

        for failure, expected in (
            ("commit", ["commit", "rollback", "close"]),
            ("rollback", ["rollback", "close"]),
            ("close", ["commit", "close"]),
            ("base", ["close"]),
        ):
            with self.subTest(failure=failure):
                actions = []

                class TracedSession(sqlmodel.Session):
                    def commit(self):
                        actions.append("commit")
                        if failure == "commit":
                            raise RuntimeError("commit")
                        super().commit()

                    def rollback(self):
                        actions.append("rollback")
                        if failure == "rollback":
                            raise RuntimeError("rollback")
                        super().rollback()

                    def close(self):
                        actions.append("close")
                        super().close()
                        if failure == "close":
                            raise RuntimeError("close")

                with patch.object(module, "Session", TracedSession):
                    with self.assertRaises(
                        KeyboardInterrupt if failure == "base" else RuntimeError
                    ):
                        with module.session_scope(self.engine):
                            if failure == "rollback":
                                raise ValueError("body")
                            if failure == "base":
                                raise KeyboardInterrupt()
                self.assertEqual(actions, expected)

    async def test_async_buffered_rows_task_local_sessions_and_overlap(self):
        engine = async_sa.create_async_engine("sqlite+aiosqlite:///" + self.path)
        self.addAsyncCleanup(engine.dispose)

        async def read(session_type):
            async with session_type(engine) as session:
                if session_type is async_model.AsyncSession:
                    rows = await session.exec(sqlmodel.select(Item.id, Item.value))
                    values = await session.exec(sqlmodel.select(Item.value))
                    self.assertEqual(values.all(), ["initial"])
                else:
                    rows = await session.execute(sa.select(Item.id, Item.value))
                    streamed = await session.stream(sa.select(Item.value))
                    self.assertEqual([row async for row in streamed], [("initial",)])
                return rows.all()

        for session_type in (async_sa.AsyncSession, async_model.AsyncSession):
            self.assertEqual(
                await asyncio.gather(read(session_type), read(session_type)),
                [[(1, "initial")]] * 2,
            )

        provision_entered = asyncio.Event()
        provision_release = asyncio.Event()
        blocked_engine = async_sa.create_async_engine(
            "sqlite+aiosqlite:///" + self.path
        )
        self.addAsyncCleanup(blocked_engine.dispose)

        @sa.event.listens_for(blocked_engine.sync_engine, "connect")
        def pause_provisioning(connection, _record):
            provision_entered.set()
            connection.run_async(lambda _driver: provision_release.wait())

        async with async_sa.AsyncSession(blocked_engine) as session:
            query = asyncio.create_task(session.execute(sa.select(Item.value)))
            try:
                await asyncio.wait_for(provision_entered.wait(), timeout=5)
                with self.assertRaises(sa.exc.InvalidRequestError):
                    await session.execute(sa.select(Item.value))
            finally:
                provision_release.set()
                await query

    async def test_direct_and_context_managed_close_cancel_at_different_points(self):
        engine = async_sa.create_async_engine("sqlite+aiosqlite:///" + self.path)
        self.addAsyncCleanup(engine.dispose)
        checkins = []

        @sa.event.listens_for(engine.sync_engine, "checkin")
        def checked_in(_connection, _record):
            checkins.append("check-in")

        for base in (async_sa.AsyncSession, async_model.AsyncSession):
            for context_managed in (False, True):
                with self.subTest(base=base, context_managed=context_managed):
                    entered = asyncio.Event()
                    release = asyncio.Event()

                    class ControlledSession(base):
                        async def close(self):
                            entered.set()
                            await release.wait()
                            await super().close()

                    session = ControlledSession(engine)
                    await session.execute(sa.select(Item.value))
                    before = len(checkins)

                    async def context_exit():
                        async with session:
                            pass

                    operation = asyncio.create_task(
                        context_exit() if context_managed else session.close()
                    )
                    await asyncio.wait_for(entered.wait(), timeout=5)
                    operation.cancel()
                    await asyncio.sleep(0)
                    if context_managed:
                        self.assertFalse(operation.done())
                        release.set()
                    with self.assertRaises(asyncio.CancelledError):
                        await operation
                    self.assertEqual(len(checkins) - before, int(context_managed))
                    release.set()
                    await session.close()


def run_checker(executable):
    with tempfile.TemporaryDirectory() as directory:
        project = Path(directory)
        dependencies = json.dumps(sysconfig.get_path("purelib"))
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
roots = [{dependencies}]
[tool.codepact.runtime-profile]
implementation = "cpython"
python-version = "3.13.16"
os = "linux"
architecture = "x86_64"
event-loop = "default"
""",
            encoding="utf-8",
        )
        (project / "caller.py").write_text(
            """from asyncio import TaskGroup, create_task
from httpx import Client, AsyncClient
from httpx2 import Client as OtherClient, AsyncClient as OtherAsyncClient
from sqlalchemy import create_engine, text
from sqlalchemy.orm import Session
from sqlalchemy.ext.asyncio import create_async_engine, AsyncSession
from sqlmodel import Session as ModelSession, select
from sqlmodel.ext.asyncio.session import AsyncSession as AsyncModelSession
from etlantic_sqlmodel.control_plane.session import session_scope

def clients():
    with Client(trust_env=False) as client:
        buffered = client.get("http://127.0.0.1")
    buffered.read()
    client.get("http://127.0.0.1")

def stream():
    with OtherClient(trust_env=False) as client:
        with client.stream("GET", "http://127.0.0.1") as response:
            pass
        response.read()

def database():
    engine = create_engine("sqlite:///fixture.sqlite")
    try:
        with Session(engine) as session:
            session.execute(text("select 1")).all()
        with ModelSession(engine) as session:
            session.exec(select(1)).all()
        with session_scope(engine) as session:
            session.exec(select(1)).all()
    finally:
        engine.dispose()

async def shared_http():
    async with AsyncClient(trust_env=False) as client:
        async with TaskGroup() as group:
            first = group.create_task(client.get("http://127.0.0.1/one"))
            second = group.create_task(client.get("http://127.0.0.1/two"))
        first.result()
        second.result()

async def shared_other_http():
    async with OtherAsyncClient(trust_env=False) as client:
        async with TaskGroup() as group:
            first = group.create_task(client.get("http://127.0.0.1/one"))
            second = group.create_task(client.get("http://127.0.0.1/two"))
        first.result()
        second.result()

async def closed_client_scheduled_later():
    client = AsyncClient(trust_env=False)
    await client.aclose()
    request = create_task(client.get("http://127.0.0.1/closed"))
    await request

async def shared_session():
    engine = create_async_engine("sqlite+aiosqlite:///fixture.sqlite")
    try:
        async with AsyncSession(engine) as session:
            async with TaskGroup() as group:
                first = group.create_task(session.execute(text("select 1")))
                second = group.create_task(session.execute(text("select 2")))
            first.result().all()
            second.result().all()
    finally:
        await engine.dispose()

async def separate_sessions():
    engine = create_async_engine("sqlite+aiosqlite:///fixture.sqlite")
    try:
        async with AsyncModelSession(engine) as left, AsyncModelSession(engine) as right:
            async with TaskGroup() as group:
                first = group.create_task(left.exec(select(1)))
                second = group.create_task(right.exec(select(2)))
            first.result().all()
            second.result().all()
    finally:
        await engine.dispose()

async def direct_while_scheduled():
    engine = create_async_engine("sqlite+aiosqlite:///fixture.sqlite")
    try:
        async with AsyncSession(engine) as session:
            async with TaskGroup() as group:
                first = group.create_task(session.execute(text("select 1")))
                await session.commit()
            first.result().all()
    finally:
        await engine.dispose()
""",
            encoding="utf-8",
        )
        result = subprocess.run(
            [executable, "check", "--json"], cwd=project, capture_output=True, text=True
        )
        if result.returncode != 1:
            raise AssertionError((result.returncode, result.stdout, result.stderr))
        report = json.loads(result.stdout)
        uses = {
            use["symbol"]
            for use in report["foundations"]["model_uses"]
            if use["applicable"]
        }
        expected = {
            "httpx.Client.get",
            "httpx.AsyncClient.get",
            "httpx2.Client.stream",
            "httpx2.AsyncClient.get",
            "sqlalchemy.Session.execute",
            "sqlalchemy.AsyncSession.execute",
            "sqlmodel.Session.exec",
            "sqlmodel.AsyncSession.exec",
            "etlantic_sqlmodel.control_plane.session.session_scope",
        }
        if not expected <= uses:
            raise AssertionError(
                f"missing applicable reviewed effects: {expected - uses}"
            )
        closed_use = [
            item for item in report["diagnostics"] if item["code"] == "TP1001"
        ]
        if len(closed_use) != 3 or any(
            item["certainty"] != "definite" for item in closed_use
        ):
            raise AssertionError(closed_use)
        overlaps = [
            item
            for item in report["diagnostics"]
            if item["code"] == "TP2006" and "one session" in item["message"]
        ]
        if len(overlaps) != 2:
            raise AssertionError(
                "expected one scheduled and one direct session overlap, and no task-local overlap"
            )
        if (project / "fixture.sqlite").exists():
            raise AssertionError("the checker executed provider or project code")
        print(
            json.dumps(
                {
                    "reviewed_cli_symbols": sorted(uses),
                    "providers": report["foundations"]["providers"],
                },
                sort_keys=True,
            )
        )

        # A second root shadows only HTTPX with modified installed source. The
        # selected files no longer verify against that distribution's RECORD.
        shadow = project / "shadow"
        shadow.mkdir()
        purelib = Path(sysconfig.get_path("purelib"))
        shutil.copytree(
            purelib / "httpx",
            shadow / "httpx",
            ignore=shutil.ignore_patterns("__pycache__"),
        )
        httpx_info = next(purelib.glob("httpx-*.dist-info"))
        shutil.copytree(httpx_info, shadow / httpx_info.name)
        source = shadow / "httpx" / "_client.py"
        source.write_bytes(source.read_bytes() + b"\n# altered installed artifact\n")
        config = project / "pyproject.toml"
        config.write_text(
            config.read_text().replace(
                f"roots = [{dependencies}]",
                f"roots = [{json.dumps(str(shadow))}, {dependencies}]\n",
            ),
            encoding="utf-8",
        )
        checked = subprocess.run(
            [executable, "check", "--json"], cwd=project, capture_output=True, text=True
        )
        modified = json.loads(checked.stdout)["foundations"]
        if not any(
            issue["import_scope"] == "httpx" for issue in modified["provider_issues"]
        ):
            raise AssertionError("the checker accepted the altered HTTPX installation")
        if not any(
            use["symbol"] == "httpx2.Client.stream" and use["applicable"]
            for use in modified["model_uses"]
        ):
            raise AssertionError(
                "changing HTTPX invalidated the independent HTTPX2 profile"
            )


if __name__ == "__main__":
    print(
        json.dumps(
            {
                "python": platform.python_version(),
                "runtime": platform.platform(),
                "sqlite": sqlite3.sqlite_version,
                "artifacts": sorted(
                    (item.metadata["Name"], item.version)
                    for item in importlib.metadata.distributions()
                ),
            },
            sort_keys=True,
        ),
        flush=True,
    )
    if checker := os.environ.get("CODEPACT_PHASE09_CHECKER"):
        run_checker(checker)
    unittest.main(verbosity=2)
