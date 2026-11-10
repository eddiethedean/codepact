from codepact.rust import Borrowed
from service import Session, authorization_decision

def close_caller_session(session: Borrowed[Session]):
    session.release()

def lose_connection():
    session = Session()

def respond_after_release():
    session = Session()
    session.release()
    return session.query()

def ignore_authorization():
    authorization_decision()

def valid_request():
    session = Session()
    result = session.query()
    session.release()
    return result
