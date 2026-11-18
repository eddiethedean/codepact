# Examples

Each example has a narrow purpose. Install CodePact 0.9.0 with
`uv add 'codepact[cli]==0.9.0'`, then run the examples with `uv run --locked codepact`.

| Example | What it demonstrates | How to run |
| --- | --- | --- |
| [`examples/contracts.py`](https://github.com/eddiethedean/codepact/blob/v0.9.0/examples/contracts.py) | Resource, `Borrowed`, and `Owned` declarations | `uv run --locked codepact inspect-contracts --root . examples/contracts.py` |
| [`docs/_examples/connection.py`](https://github.com/eddiethedean/codepact/blob/v0.9.0/docs/_examples/connection.py) | Inspector contract facts and JSON snapshot | `uv run --locked codepact inspect-contracts --root docs/_examples docs/_examples/connection.py` |
| [`docs/_examples/project/`](https://github.com/eddiethedean/codepact/tree/v0.9.0/docs/_examples/project) | Direct absolute imports across supplied modules | See the [multi-file tutorial](project-guide.md) |
| [`examples/lifecycle.py`](https://github.com/eddiethedean/codepact/blob/v0.9.0/examples/lifecycle.py) | Supported use-after-close finding with the related close location | `uv run --locked codepact check --root examples` |
| [Synthetic bug examples](bugs-caught.md) | Corpus-backed use-after-close, cleanup, ownership, borrowed access, must-use, and cross-module examples | See [synthetic project controls](synthetic-projects.md) |
| [First-package pilot](pilot.md) | Runnable finding, repair, and coverage boundary under one source-only policy | Download the ZIP and use the published CLI |
| [Work and outcomes](runtime-models.md#try-result-observation-and-outcome-handling) | Work retrieval and persistent outcome acknowledgment | Download the example and check its runtime profile |
| [Provider and caller](publish-provider.md) | Export, verify, and consume an installed contract | Build a pure Python wheel and check the paired caller |
| [Queues, guards, and spawn](concurrency.md) | Each protocol's valid, finding, and incomplete-control cases | Build the **0.10 candidate** and use its pinned profiles |
| [Catalog-inspired projects](https://github.com/eddiethedean/codepact/tree/v0.9.0/crates/codepact/tests/projects/code_smells) | Annotated bug/repair pairs and coverage controls for 0.5 ownership checks | See [the corpus guide](synthetic-projects.md#catalog-inspired-examples) |

The lifecycle example intentionally exits with status `1`. Removing its final
use produces a valid counterpart with no lifecycle finding or coverage boundary
for that supported source. It is a static-analysis fixture, not a demonstration
that the sample `close` method releases an actual resource.

Try the [rule cookbook](rules.md#run-a-bug-and-its-repair) for complete
projects with pinned settings and expected exits. Continue from the first
check with the [multi-file starter](project-guide.md).

Run static checks on the sample files. The downloadable projects name the
required binary, the command's working directory, and the expected findings
and exit status. See [documentation maintenance](documentation.md#maintain-the-site)
for the checks that replay these examples.
