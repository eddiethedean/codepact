from codepact.rust import Borrowed
from service import Session, authorization_decision


def close_caller_session(session: Borrowed[Session]) -> int:
    return session.query()


def lose_connection() -> int:
    session = Session()
    try:
        return session.query()
    finally:
        session.release()


def respond_after_release() -> int:
    session = Session()
    result = session.query()
    session.release()
    return result


def ignore_authorization() -> bool:
    return authorization_decision()


def valid_request() -> int:
    session = Session()
    result = session.query()
    session.release()
    return result
