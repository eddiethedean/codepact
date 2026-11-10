from codepact.rust import Borrowed
from service import Session, authorization_decision

def close_caller_session(session: Borrowed[Session]):
    return session.query()

def lose_connection():
    session = Session()
    try:
        return session.query()
    finally:
        session.release()

def respond_after_release():
    session = Session()
    result = session.query()
    session.release()
    return result

def ignore_authorization():
    return authorization_decision()

def valid_request():
    session = Session()
    result = session.query()
    session.release()
    return result
