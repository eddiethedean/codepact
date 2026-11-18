from multiprocessing import get_context


if __name__ == "__main__":
    context = get_context("spawn")
    child = context.Process(target=lambda: None)
    try:
        child.start()
    except AttributeError:
        child.close()
