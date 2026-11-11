def invoice_timeout(use_extended_timeout: bool) -> int:
    if use_extended_timeout:
        return 259200
    return 86400


def delivery_timeout(use_extended_timeout: bool) -> int:
    if use_extended_timeout:
        return 259200
    return 86400
