from codepact import Err, Ok, Option, Result, Some


ok_float: Ok[float] = Ok(1.0)
bad_ok: Ok[int] = ok_float  # typing-case: ok-covariance

error_general: Err[Exception] = Err(Exception())
bad_error: Err[ValueError] = error_general  # typing-case: err-covariance

result_general: Result[float, Exception] = Err(Exception())
bad_result: Result[int, ValueError] = result_general  # typing-case: result-covariance

option_float: Option[float] = Some(1.0)
bad_option: Option[int] = option_float  # typing-case: option-covariance

immutable = Ok(1)
immutable.value = 2  # typing-case: frozen-payload


class Subclass(Ok[int]):  # typing-case: final-outcome
    pass
