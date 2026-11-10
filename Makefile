PYTHON ?= python
CARGO ?= cargo
RUSTUP ?= rustup
UV ?= uv
DOCS_BUILD_DIR ?= target/docs/html
CARGO_AUDIT_ARGS ?=
CARGO_DENY_ARGS ?=

.DEFAULT_GOAL := check
.PHONY: check check-python check-python-package check-rust check-msrv check-dependencies check-rust-package check-inspector check-docs check-schemas

check: check-rust check-python check-python-package check-dependencies check-msrv check-rust-package check-inspector check-docs

check-rust:
	$(CARGO) fmt --all -- --check
	$(CARGO) check --workspace --locked
	$(CARGO) clippy --workspace --all-targets --locked -- -D warnings
	$(CARGO) test --workspace --locked

check-python:
	$(PYTHON) scripts/generate_identifier_tables.py --check
	$(PYTHON) scripts/check_phase03_validation.py
	PYTHONPATH=python $(PYTHON) -m unittest discover -s tests/python -v
	basedpyright
	ruff check python/codepact scripts
	ruff check crates/codepact/tests/projects
	ruff format --check python/codepact scripts crates/codepact/tests/projects
	$(PYTHON) scripts/check_synthetic_static_tools.py
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
	$(CARGO) audit $(CARGO_AUDIT_ARGS)
	$(CARGO) deny check $(CARGO_DENY_ARGS) advisories bans licenses sources

check-msrv:
	@if ! $(RUSTUP) toolchain list | grep -Eq '^1[.]96[.]0([ -]|$$)'; then \
		$(RUSTUP) toolchain install 1.96.0 --profile minimal; \
	fi
	$(CARGO) +1.96.0 check --workspace --locked

check-rust-package:
	@set -eu; \
	package_metadata="$$($(CARGO) metadata --format-version 1 --no-deps | python3 -c 'import json, sys; metadata = json.load(sys.stdin); package = next(item for item in metadata["packages"] if item["name"] == "codepact"); print(metadata["target_directory"]); print(package["version"])')"; \
	target_directory="$$(printf '%s\n' "$$package_metadata" | sed -n '1p')"; \
	package_version="$$(printf '%s\n' "$$package_metadata" | sed -n '2p')"; \
	$(CARGO) package -p codepact --locked --allow-dirty; \
	tmp="$$(mktemp -d)"; \
	trap 'rm -rf "$$tmp"' EXIT HUP INT TERM; \
	crate="$$target_directory/package/codepact-$$package_version.crate"; \
	test -f "$$crate"; \
	mkdir -p "$$tmp/archive"; \
	tar -xzf "$$crate" -C "$$tmp/archive"; \
	package_dir="$$tmp/archive/codepact-$$package_version"; \
	test -f "$$package_dir/Cargo.toml"; \
	$(CARGO) test --manifest-path "$$package_dir/Cargo.toml" --locked; \
	$(CARGO) install --path "$$package_dir" --root "$$tmp/install" --locked --force; \
	version_output="$$($$tmp/install/bin/codepact --version)"; \
	test "$$version_output" = "codepact $$package_version"; \
	printf '%s\n' "$$version_output"; \
	(cd "$$package_dir" && "$$tmp/install/bin/codepact" inspect-contracts tests/contracts.py); \
	mkdir "$$tmp/lifecycle"; \
	printf '%s\n' \
		"open('$$tmp/lifecycle/executed', 'w').close()" \
		"from codepact import resource" \
		"@resource(closed_by='close')" \
		"class Connection:" \
		"    def close(self): pass" \
		"    def status(self): return 'ready'" \
		"connection = Connection()" \
		"connection.close()" \
		"connection.status()" >"$$tmp/lifecycle/connection.py"; \
	if "$$tmp/install/bin/codepact" check --root "$$tmp/lifecycle" --json >"$$tmp/lifecycle.json"; then status=0; else status="$$?"; fi; \
	test "$$status" -eq 1; \
	grep -Fq '"code": "TP1001"' "$$tmp/lifecycle.json"; \
	grep -Fq '"certainty": "definite"' "$$tmp/lifecycle.json"; \
	grep -Fq '"related": [' "$$tmp/lifecycle.json"; \
	test ! -e "$$tmp/lifecycle/executed"

check-inspector:
	@set -eu; \
	tmp="$$(mktemp -d)"; \
	trap 'rm -rf "$$tmp"' EXIT HUP INT TERM; \
	if CODEPACT_SIDE_EFFECT_MARKER="$$tmp/marker" $(CARGO) run -q -p codepact -- inspect-contracts tests/fixtures/side_effect.py >"$$tmp/inspection.json"; then \
		echo "expected the unsupported fixture to report incomplete coverage" >&2; exit 1; \
	else \
		status="$$?"; \
		if test "$$status" -ne 1; then cat "$$tmp/inspection.json"; exit "$$status"; fi; \
	fi; \
	grep -Fq '"status": "incomplete"' "$$tmp/inspection.json"; \
	if test -e "$$tmp/marker"; then echo "inspected Python source was executed" >&2; exit 1; fi

check-docs:
	$(UV) run --no-project --with-requirements docs/requirements.txt python scripts/check_schema_fixtures.py
	$(PYTHON) scripts/check_docs_links.py
	$(UV) run --no-project --with-requirements docs/requirements.txt sphinx-build -E -a -b html -n -W --keep-going docs "$(DOCS_BUILD_DIR)"
