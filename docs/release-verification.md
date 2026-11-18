# Verify a release

Verify the distribution for the runner that will use it and record the
checker version, operating system, architecture, source target, and artifact
digest in the pilot or release record. Use the
[platform matrix](availability.md#platform-and-runtime-evidence) for wheel
minimums and [release availability](availability.md) to choose a release.

## Verify and install wheels

On a machine that matches the runner's Python, OS, and architecture, set
`CODEPACT_VERSION` to the published version you intend to install. A
wheel-only download fails if the index offers no compatible wheel.

```sh
python -m pip download --only-binary=:all: --dest wheelhouse "codepact[cli]==$CODEPACT_VERSION"
python -m pip hash wheelhouse/*.whl
```

Compare the helper and CLI digests with the corresponding
[release evidence](availability.md#releases) and verify transitive artifacts
against your approved registry or mirror. For a standalone CLI wheelhouse,
download `"codepact-cli==$CODEPACT_VERSION"`.
Record every resolved package as an exact pin with its SHA-256 hash in
`approved-requirements.txt`. Transfer that file and the verified wheelhouse
to the runner. Install into an activated virtual environment.

```sh
python -m pip install --no-index --find-links wheelhouse \
  --only-binary=:all: --require-hashes -r approved-requirements.txt
codepact --version
```

Expect the selected CodePact version. Registry hashes check artifact integrity.
Use the tagged revision and [release evidence](availability.md#releases)
for the source and build record. Apply your organization's trust policy to
the source, registry, and CI artifacts. Review [report data handling](security.md)
before retaining artifacts from a pilot.

## Verify hosted docs after publication

Use the published release tag and the expected exact source commit from the
release record. In Read the Docs, activate the version for the release tag,
build it, and copy its public version URL from the dashboard. Use the permanent
commit-based source link in [release availability](availability.md) when
sharing archived instructions independently of hosted aliases.

Run the verifier from the maintenance checkout. Replace `HOSTED_VERSION_URL`
and `RELEASE_COMMIT` with the activated URL and full commit. `--channel Released`
checks that the install examples pin the same version as the release.

```sh
python scripts/verify_docs_release.py HOSTED_VERSION_URL \
  --version 0.10.0 --channel Released --revision RELEASE_COMMIT
```

The verifier checks the docs version and revision, the installation pin,
example downloads, report schemas, and the search index. For older builds
such as 0.9.0, add `--legacy` to check the commit in the source links and
validate the original downloads. Verify the configured stable alias too.

After publication, replay the documented commands with
`python scripts/check_docs_examples.py` from the checkout that contains the
matching published recipes. Promote the published pin and immutable source
revision in `docs/versions.toml` after verifying the new artifacts. Refresh
the guides and [generated reference tables](documentation.md#maintain-the-site)
on the maintained branch. Record the hosted URLs, source revision, command
results, and registry digests in the release record. Review the
[deployment steps](documentation.md#deploy-a-documentation-revision)
and [release gates](https://github.com/eddiethedean/codepact/blob/main/docs/planning/release-gates.md).
