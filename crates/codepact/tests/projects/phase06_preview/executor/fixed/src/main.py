from concurrent.futures import ThreadPoolExecutor


def worker() -> int:
    return 1


def run() -> None:
    pool = ThreadPoolExecutor()
    pool.submit(worker)
    pool.shutdown()
