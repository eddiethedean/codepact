from multiprocessing import get_context


def worker() -> None:
    pass


if __name__ == "__main__":
    context = get_context("spawn")
    child = context.Process(target=worker)
    child.start()
    child.join()
    child.close()
