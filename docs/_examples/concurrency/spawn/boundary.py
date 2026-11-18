from multiprocessing import get_context


def worker(queue) -> None:  # noqa: ANN001 - model the queue passed by spawn
    queue.get()
    queue.close()


if __name__ == "__main__":
    context = get_context("spawn")
    pending = context.SimpleQueue()
    child = context.Process(target=worker, args=(pending,))
    child.start()
    child.join()
    child.close()
    pending.close()
