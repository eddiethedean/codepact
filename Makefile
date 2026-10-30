PYTHON ?= python
CARGO ?= cargo
RUSTUP ?= rustup
UV ?= uv

.DEFAULT_GOAL := check
.PHONY: check check-python check-python-package check-rust check-msrv check-dependencies check-rust-package check-inspector

check: check-rust check-python check-python-package check-dependencies check-msrv check-rust-package check-inspector

check-rust:
	$(CARGO) fmt --all -- --check
	$(CARGO) check --workspace --locked
	$(CARGO) clippy --workspace --all-targets --locked -- -D warnings
	$(CARGO) test --workspace --locked

check-python:
	PYTHONPATH=python $(PYTHON) -m unittest discover -s tests/python -v
	basedpyright
	ruff check python/typepact scripts
	ruff format --check python/typepact scripts
	MYPYPATH=python mypy --strict tests/checkers/valid.py
	@diagnostics="$$(mktemp)"; \
	if MYPYPATH=python mypy --strict tests/checkers/invalid.py >"$$diagnostics" 2>&1; then \
		cat "$$diagnostics"; rm -f "$$diagnostics"; echo "mypy accepted the invalid typing fixture" >&2; exit 1; \
	fi; \
	if ! grep -q 'Incompatible types in assignment' "$$diagnostics"; then \
		cat "$$diagnostics"; rm -f "$$diagnostics"; exit 1; \
	fi; \
	rm -f "$$diagnostics"
	PYTHONPATH=python pyright tests/checkers/valid.py
	@diagnostics="$$(mktemp)"; \
	if PYTHONPATH=python pyright tests/checkers/invalid.py >"$$diagnostics" 2>&1; then \
		cat "$$diagnostics"; rm -f "$$diagnostics"; echo "pyright accepted the invalid typing fixture" >&2; exit 1; \
	fi; \
	if ! grep -q 'reportAssignmentType' "$$diagnostics"; then \
		cat "$$diagnostics"; rm -f "$$diagnostics"; exit 1; \
	fi; \
	rm -f "$$diagnostics"

check-python-package:
	$(PYTHON) scripts/check_python_package.py

check-dependencies:
	cargo audit
	cargo deny check advisories bans licenses sources

check-msrv:
	$(RUSTUP) toolchain install 1.96.0 --profile minimal
	$(CARGO) +1.96.0 check --workspace --locked

check-rust-package:
	@set -eu; \
	$(CARGO) package -p typepact --locked --allow-dirty; \
	tmp="$$(mktemp -d)"; \
	trap 'rm -rf "$$tmp"' EXIT HUP INT TERM; \
	crate="$$(find target/package -maxdepth 1 -name 'typepact-*.crate' -print -quit)"; \
	test -n "$$crate"; \
	mkdir -p "$$tmp/archive"; \
	tar -xzf "$$crate" -C "$$tmp/archive"; \
	package_dir="$$(find "$$tmp/archive" -mindepth 1 -maxdepth 1 -type d -print -quit)"; \
	$(CARGO) test --manifest-path "$$package_dir/Cargo.toml" --locked; \
	$(CARGO) install --path "$$package_dir" --root "$$tmp/install" --locked --force; \
	"$$tmp/install/bin/typepact" --version; \
	(cd "$$package_dir" && "$$tmp/install/bin/typepact" inspect-contracts tests/contracts.py)

check-inspector:
	@set -eu; \
	tmp="$$(mktemp -d)"; \
	trap 'rm -rf "$$tmp"' EXIT HUP INT TERM; \
	if TYPEPACT_SIDE_EFFECT_MARKER="$$tmp/marker" $(CARGO) run -q -p typepact -- inspect-contracts tests/fixtures/side_effect.py >"$$tmp/inspection.json"; then \
		echo "expected the unsupported fixture to report incomplete coverage" >&2; exit 1; \
	else \
		status="$$?"; \
		if test "$$status" -ne 1; then cat "$$tmp/inspection.json"; exit "$$status"; fi; \
	fi; \
	grep -Fq '"status": "incomplete"' "$$tmp/inspection.json"; \
	if test -e "$$tmp/marker"; then echo "inspected Python source was executed" >&2; exit 1; fi
