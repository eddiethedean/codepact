from codepact.rust import Borrowed
from service import Session, authorization_decision


def close_caller_session(session: Borrowed[Session]) -> None:
    session.release()


def lose_connection() -> None:
    session = Session()
    session.query()


def respond_after_release() -> int:
    session = Session()
    session.release()
    return session.query()


def ignore_authorization() -> None:
    authorization_decision()


def valid_request() -> int:
    session = Session()
    result = session.query()
    session.release()
    return result
