<!-- release-recipes -->

# Publish a provider wheel

Use the published 0.9.0 CLI to export and verify the sample wheel. The project
uses a regular pure-Python package under `src/acme_storage`, with an empty
`py.codepact` marker and a Hatchling build that includes package data. The
sample publishes a must-use contract for `acme_storage.export_receipt`.

```{raw} html
<p><a href="_static/codepact-provider.zip" download>Download the provider and caller projects</a></p>
```

Extract the archive and open its root, which contains `pyproject.toml`,
`src/acme_storage/`, and `caller/`. Install the [standalone CLI](install.md#standalone-command)
and put it on your PATH. Run the following commands from the provider root.

<!-- smoke: export-provider -->
```sh
codepact contracts export --root src --distribution acme-storage \
  --version 1.2.0 --import-scope acme_storage --python-version 3.13 \
  > src/acme_storage/codepact.contracts.json
```

<!-- smoke: build-provider -->
```sh
uv build --wheel --out-dir dist
codepact contracts verify dist/acme_storage-1.2.0-py3-none-any.whl
```

The verification command exits `0` and reports `status: "verified"`,
the distribution and import scope, and the number of published symbols.
Each supported manifest entry points to a public export and the source
it was derived from. The wheel's `RECORD` verifies the files in the archive.
After editing the provider, re-export its manifest and rebuild the wheel.
Verification rejects a stale manifest.

Installed manifest facts can establish a caller requirement such as
`@must_use`. Cleanup and ownership effects require matching checked source
or an applicable reviewed model. Native extensions and dynamic import
layouts can leave coverage incomplete.

[Install and check against the sample wheel](consume-providers.md#try-an-installed-provider) ·
[Compare two wheel publications](cli.md#compare-published-wheel-contracts)
