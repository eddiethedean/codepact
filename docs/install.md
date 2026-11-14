# Installation

Install CodePact 0.5.1. The Python installer requires Python 3.11 or later,
including when installing only the CLI wheel. The analyzer accepts Python
3.11, 3.12, and 3.13 source targets and defaults to 3.11. Choose your target
with `--python-version` or `python-version` in project configuration.
[Verified platforms and release artifacts](availability.md#platform-and-runtime-evidence)

| Goal | Install route |
| --- | --- |
| Add helper imports and the checker to a uv project | [uv project](#uv-project) |
| Use the helpers and checker in an existing pip workflow | [pip environment](#pip-environment) |
| Run the checker against source or in CI | [Standalone command](#standalone-command) |
| Embed the analyzer or install the executable with Cargo | [Rust library and Cargo command](#rust-library-and-cargo-command) |
| Reproduce the release from its tag | [Build from source](#build-from-source) |

(python-helpers-and-matching-cli)=
## uv project

Install [uv](https://docs.astral.sh/uv/getting-started/installation/). In an
existing uv project, add the pinned dependency.

```sh
uv add codepact==0.5.1
uv run --locked codepact --version
uv run --locked python -c "from codepact import resource"
```

Expect `codepact 0.5.1` and a successful import. The helper package pins the
matching CLI. Follow the [quick start](quickstart.md) for an empty directory.

## pip environment

Use a virtual environment to keep the command and helper imports together.
On macOS and Linux, activate the environment before installing.

```sh
python3 -m venv .venv
. .venv/bin/activate
python -m pip install 'codepact==0.5.1'
codepact --version
python -c "from codepact import resource"
```

In Windows PowerShell, call the virtual environment's executables directly.

```powershell
py -3 -m venv .venv
.\.venv\Scripts\python.exe -m pip install 'codepact==0.5.1'
.\.venv\Scripts\codepact.exe --version
.\.venv\Scripts\python.exe -c "from codepact import resource"
```

Expect `codepact 0.5.1`. Save the [quick-start sample](quickstart.md#save-the-example)
as `src/connection.py`, then run `codepact check --root src` in the active
environment or use the direct PowerShell executable path. Follow the
[repair steps](quickstart.md#find-and-fix-the-bug) with that same executable.

## Standalone command

Use uv to run the pinned native CLI without changing the project.

```sh
uv tool run --from codepact-cli==0.5.1 codepact --version
```

Expect `codepact 0.5.1`. To try the analyzer, save the
[quick-start sample](quickstart.md#save-the-example) as `src/connection.py` and
run the check.

```sh
uv tool run --from codepact-cli==0.5.1 codepact check --root src
```

Expect TP1001 and exit `1` before the [repair](quickstart.md#find-and-fix-the-bug).
The analyzer parses the imports in the sample as text. Install the helper in
your project's Python environment if your application imports it at runtime.

For a command on your PATH, use `uv tool install codepact-cli==0.5.1` followed
by `codepact --version`. If the shell cannot find it, run
`uv tool update-shell` and open a new shell.
[Installation recovery](troubleshooting.md#the-command-is-unavailable-after-installation)

(rust-library)=
## Rust library and Cargo command

With Rust 1.96 or later and a linker, add an exact library dependency.

```sh
cargo add codepact@=0.5.1
cargo check --locked
```

Continue with the [Rust API reference](rust-api.md). To install the CLI from
crates.io, use Cargo.

```sh
cargo install codepact --version 0.5.1 --locked
codepact --version
```

Expect `codepact 0.5.1`. Use the same source sample and repair in the
[quick start](quickstart.md), invoking `codepact` directly.

## Build from source

Install Git, uv, Python 3.11 or later, Rust 1.96 or later, and a platform linker.

```sh
git clone --branch v0.5.1 https://github.com/eddiethedean/codepact.git
cd codepact
uv sync --locked
uv run --locked codepact --version
uv run --locked codepact capabilities --json
git rev-parse HEAD
```

Record the revision when sharing reports. The first version check should
print `codepact 0.5.1`. To build just the Rust command from the checkout,
run `cargo build --release --locked` and verify
`./target/release/codepact --version` (or
`.\target\release\codepact.exe --version` in PowerShell).

## Compatibility and wheels

The universal helper wheel includes `py.typed`. Native CLI wheels cover
Linux glibc and musl, macOS, and Windows on x86-64 and ARM64. Native wheels
need no Rust compiler. A CLI source archive build needs Rust 1.96 or later,
a platform linker, and access to locked Cargo dependencies. Python 3.14 can
run the helpers and install the CLI, while the checker supports source
grammar through Python 3.13.
[Platform minimums and qualification](availability.md#platform-and-runtime-evidence)
