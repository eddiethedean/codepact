from typing import assert_type

from codepact import Err, Nothing, Ok, Option, Result, Some, from_nullable, to_nullable


def use_result(result: Result[int, ValueError]) -> int:
    match result:
        case Ok(value):
            assert_type(value, int)
            return value
        case Err(error):
            assert_type(error, ValueError)
            raise error


def use_option(optional: Option[int]) -> int | None:
    match optional:
        case Some(value):
            assert_type(value, int)
            return value
        case Nothing():
            return None


def narrow_result(result: Result[int, ValueError]) -> int | ValueError:
    if isinstance(result, Ok):
        assert_type(result.value, int)
        return result.value
    assert_type(result.error, ValueError)
    return result.error


def narrow_option(optional: Option[int]) -> int | None:
    if isinstance(optional, Some):
        assert_type(optional.value, int)
        return optional.value
    assert_type(optional, Nothing)
    return None


ok: Ok[int] = Ok(1)
ok_widened: Ok[float] = ok
error: Err[ValueError] = Err(ValueError())
error_widened: Err[Exception] = error
result: Result[int, ValueError] = ok
result_widened: Result[float, Exception] = result
some: Some[int] = Some(2)
some_widened: Some[float] = some
option: Option[int] = some
option_widened: Option[float] = option
none_option: Option[int] = from_nullable(None)
present_option: Option[int] = from_nullable(3)
nullable: int | None = to_nullable(present_option)

assert_type(Ok(1), Ok[int])
assert_type(Err(ValueError()), Err[ValueError])
assert_type(Some(1), Some[int])
assert_type(use_result(result), int)
assert_type(use_option(option), int | None)
assert_type(narrow_result(result), int | ValueError)
assert_type(narrow_option(option), int | None)
assert_type(none_option, Option[int])
assert_type(nullable, int | None)
