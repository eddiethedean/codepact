from codepact.rust import Owned
from broker import BrokerLink, decode_message


async def forget_link() -> None:
    link = BrokerLink()
    link.ack()


async def acknowledge_closed_link() -> None:
    link = BrokerLink()
    link.close()
    link.ack()


async def return_closed_link() -> Owned[BrokerLink]:
    link = BrokerLink()
    link.close()
    return link


async def skip_payload() -> None:
    decode_message()


async def valid_message() -> str:
    link = BrokerLink()
    link.ack()
    link.close()
    return decode_message()
