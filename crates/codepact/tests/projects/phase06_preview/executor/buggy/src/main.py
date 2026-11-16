from concurrent.futures import ThreadPoolExecutor


def worker() -> int:
    return 1


def run() -> None:
    pool = ThreadPoolExecutor()
    pool.shutdown()
    pool.submit(worker)
