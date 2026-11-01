# `contextlib` model conformance record

Status: **Controlled runtime cases pass on the recorded interpreters; this does
not complete V03-11 or qualify the 0.3 release.**

## Candidate and scope

The cases in [`tests/python/test_contextlib_model_conformance.py`](../../../../tests/python/test_contextlib_model_conformance.py)
exercise `contextlib.closing` and `contextlib.aclosing` directly. They do not
import or execute analyzer input. The static model remains enabled only for
target Python 3.11, 3.12 and 3.13; the 3.14 run below checks runtime behavior
only and does not add 3.14 analyzer support.

The source revisions reviewed by the model catalog are CPython
[`v3.11.0`](https://github.com/python/cpython/blob/v3.11.0/Lib/contextlib.py),
[`v3.12.0`](https://github.com/python/cpython/blob/v3.12.0/Lib/contextlib.py)
and [`v3.13.0`](https://github.com/python/cpython/blob/v3.13.0/Lib/contextlib.py).
The local source hashes below identify the exact patch interpreter files used
for the runtime runs.

## Results

All six tests passed on each listed interpreter on macOS ARM64, 2026-11-01.
The direct invocation was
`python3.X -m unittest discover -s tests/python -p test_contextlib_model_conformance.py -v`.

| Interpreter | `contextlib.py` SHA-256 | `closing` class-source SHA-256 | `aclosing` class-source SHA-256 | Result |
| --- | --- | --- | --- | --- |
| CPython 3.11.14 | `d732c045bc7450997f97b2f79cfdb1546b68d2655e89ab3170f3b19b3930d83c` | `c087c9ce86d670b13006c82234adfff4a107f87e435c2739d2712fb52b0af8a5` | `13d6c99d8f775ad651bc97245eed6e0939f6a4db7b14da99b647dc237ab5a01d` | 6 passed |
| CPython 3.12.13 | `8b7a477f978a8532852fd81e241c78182516bc4975d672d580a5848a76e11eb6` | `c087c9ce86d670b13006c82234adfff4a107f87e435c2739d2712fb52b0af8a5` | `13d6c99d8f775ad651bc97245eed6e0939f6a4db7b14da99b647dc237ab5a01d` | 6 passed |
| CPython 3.13.11 | `c1e0d67b2007de11ae93cd36cf6faf38d9ab32656a832d592a49325eec579f96` | `c087c9ce86d670b13006c82234adfff4a107f87e435c2739d2712fb52b0af8a5` | `13d6c99d8f775ad651bc97245eed6e0939f6a4db7b14da99b647dc237ab5a01d` | 6 passed |
| CPython 3.14.3 | `c1e0d67b2007de11ae93cd36cf6faf38d9ab32656a832d592a49325eec579f96` | `c087c9ce86d670b13006c82234adfff4a107f87e435c2739d2712fb52b0af8a5` | `13d6c99d8f775ad651bc97245eed6e0939f6a4db7b14da99b647dc237ab5a01d` | 6 passed; analyzer model remains disabled |

The `closing` and `aclosing` class-source hashes are identical across the four
runtime versions. The test file SHA-256 is
`fc9ec4286cd3ae3d3e6583782d228506b63b6e99343a54f0d23799b9de62e0cc`.

## Covered behavior

- `closing` and `aclosing` run cleanup once on normal body completion.
- Body exceptions propagate; cleanup failures propagate in place of a body
  exception.
- `aclosing` runs after body cancellation, while a second cancellation can
  interrupt an awaited `aclose`.

This record does not qualify every interpreter build, all manager entry/exit
paths, static cancellation postconditions, or the remaining V03 matrix. Those
remain release-evidence work in the [validation matrix](validation.md).
