from gateway import can_dispatch, confirm_address, reserve_slot


def ship() -> bool:
    confirm_address()
    return True


def schedule() -> bool:
    reserve_slot()
    return True


def release() -> bool:
    can_dispatch()
    return True
