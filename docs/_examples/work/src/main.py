from asyncio import create_task
from codepact import MustHandle, Ok, Result, discard_outcome


async def export_receipt() -> MustHandle[Result[str, str]]:
    return Ok("receipt-001")


async def check_status() -> None:
    task = create_task(export_receipt())
    task.done()


async def observe_only() -> None:
    task = create_task(export_receipt())
    _result = await task


async def acknowledge() -> None:
    task = create_task(export_receipt())
    result = await task
    discard_outcome(result, reason="demo receipt has no downstream consumer")
