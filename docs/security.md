# Security and data handling

## What the analyzer reads and does

The command reads Python source as text and parses it locally. It does not
import the analyzed application, evaluate its annotations, execute decorators,
or invoke methods from the source. `inspect-contracts` reads the files named on
the command line. The `check` preview scans `.py` files under its selected root;
keep that root limited to the source you intend to analyze. The Rust API receives
source strings from its host application.

The analyzer writes a JSON or text report to standard output and invocation
errors to standard error. It does not send project source to a remote analysis
service. Package installation and updates use the configured Python/Cargo
registries as usual; that package-manager traffic is separate from analysis.

Reports can contain logical paths, symbol names, declarations, source locations,
and diagnostic text. Review reports before publishing them as CI artifacts.
TypePact is a static analyzer, not a sandbox or a guarantee that malformed input
cannot expose a parser defect. Keep the tool updated and apply the same host and
artifact controls used for other build tools.

## Report a vulnerability

The repository does not publish a security response-time commitment or a
separate contact address. If GitHub's **Report a vulnerability** option is
available in the repository's Security tab, use that private reporting path.
If it is not available, open a GitHub issue containing only a request for a
private reporting route; do not include vulnerability details, exploit code,
or sensitive source in the public issue. No response-time SLA is promised.

For ordinary bugs, use the public [issue tracker](https://github.com/eddiethedean/typepact/issues)
and include a minimized, non-sensitive reproduction.
