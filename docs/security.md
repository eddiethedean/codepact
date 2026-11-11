# Security and data handling

## What the analyzer reads and does

The command reads Python source as text and parses it locally. It does not
import the analyzed application, evaluate its annotations, execute decorators,
or invoke methods from the source. `inspect-contracts` reads the files named on
the command line. The `check` command scans `.py` files under its selected root;
keep that root limited to the source you intend to analyze. The Rust API receives
source strings from its host application.

The analyzer writes a JSON or text report to standard output and invocation
errors to standard error. It does not send project source to a remote analysis
service. Package installation and updates use the configured Python/Cargo
registries as usual; that package-manager traffic is separate from analysis.

Reports can contain absolute checkout and configuration paths, logical
paths, symbol names, declarations, source locations, diagnostic text, and
source hashes. Keep original reports in an access-controlled artifact store.
For an external issue, share a minimized reproduction and a reviewed excerpt.
[Report fields and retention](ci-reporting.md#preserve-reports-safely)
CodePact is a static analyzer, not a sandbox or a guarantee that malformed input
cannot expose a parser defect. Keep the tool updated and apply the same host and
artifact controls used for other build tools.

## Report a vulnerability

Use the [private GitHub vulnerability reporting form](https://github.com/eddiethedean/codepact/security/advisories/new).
Include the version, platform, impact, and a minimized reproduction. The
[repository security policy](https://github.com/eddiethedean/codepact/blob/main/SECURITY.md)
describes triage and support terms.

For ordinary bugs, use the public [issue tracker](https://github.com/eddiethedean/codepact/issues)
and include a minimized, non-sensitive reproduction.
