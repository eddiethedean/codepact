from gateway import can_dispatch, confirm_address, reserve_slot


def ship() -> bool:
    address_is_valid = confirm_address()
    if address_is_valid:
        return True
    return False


def schedule() -> bool:
    return reserve_slot()


def release() -> bool:
    return can_dispatch()
