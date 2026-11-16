from concurrent.futures import ThreadPoolExecutor


def worker() -> int:
    return 1


def run(workers: int) -> None:
    pool = ThreadPoolExecutor(max_workers=workers)
    pool.submit(worker)
