# TypePact CLI

`typepact-cli` distributes the Rust `typepact` command through Python packaging.
Installing `typepact` installs this matching CLI package automatically.

Run `typepact --help` or `typepact inspect-contracts your_file.py` after installation.
Platform wheels include the executable; source installs require Rust 1.96+ and a linker.
The analyzer reads Python source without importing or executing it.

The release matrix covers Linux glibc/musl, macOS and Windows on x86-64 and ARM64.
Python 3.11+ is required for installation. The Rust crate remains available on crates.io.
