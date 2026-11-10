from codepact.rust import Owned
from broker import BrokerLink, decode_message

async def forget_link():
    link = BrokerLink()

async def acknowledge_closed_link():
    link = BrokerLink()
    link.close()
    link.ack()

async def return_closed_link() -> Owned[BrokerLink]:
    link = BrokerLink()
    link.close()
    return link

async def skip_payload():
    decode_message()

async def valid_message():
    link = BrokerLink()
    link.ack()
    link.close()
    return decode_message()
