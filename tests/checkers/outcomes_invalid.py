from codepact import Err, Ok, Option, Result, Some


ok_float: Ok[float] = Ok(1.0)
bad_ok: Ok[int] = ok_float  # expected type error

error_general: Err[Exception] = Err(Exception())
bad_error: Err[ValueError] = error_general  # expected type error

result_general: Result[float, Exception] = Err(Exception())
bad_result: Result[int, ValueError] = result_general  # expected type error

option_float: Option[float] = Some(1.0)
bad_option: Option[int] = option_float  # expected type error

immutable = Ok(1)
immutable.value = 2  # expected type error


class Subclass(Ok[int]):  # expected type error
    pass
