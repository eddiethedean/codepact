# Examples

Each example has a narrow purpose. Install CodePact 0.5.0 with
`uv add codepact==0.5.0`, then run the examples with `uv run --locked codepact`.

| Example | What it demonstrates | How to run |
| --- | --- | --- |
| [`examples/contracts.py`](https://github.com/eddiethedean/codepact/blob/v0.5.0/examples/contracts.py) | Resource, `Borrowed`, and `Owned` declarations | `uv run --locked codepact inspect-contracts --root . examples/contracts.py` |
| [`docs/_examples/connection.py`](https://github.com/eddiethedean/codepact/blob/v0.5.0/docs/_examples/connection.py) | Inspector contract facts and JSON snapshot | `uv run --locked codepact inspect-contracts --root docs/_examples docs/_examples/connection.py` |
| [`docs/_examples/project/`](https://github.com/eddiethedean/codepact/tree/v0.5.0/docs/_examples/project) | Direct absolute imports across supplied modules | See the [multi-file tutorial](project-guide.md) |
| [`examples/lifecycle.py`](https://github.com/eddiethedean/codepact/blob/v0.5.0/examples/lifecycle.py) | Supported use-after-close finding with the related close location | `uv run --locked codepact check --root examples` |
| [Synthetic bug examples](bugs-caught.md) | Corpus-backed use-after-close, cleanup, ownership, borrowed access, must-use, and cross-module examples | See [synthetic project controls](synthetic-projects.md) |
| [Catalog-inspired projects](https://github.com/eddiethedean/codepact/tree/v0.5.0/crates/codepact/tests/projects/code_smells) | Annotated bug/repair pairs and coverage controls for 0.5 ownership checks | See [the corpus guide](synthetic-projects.md#catalog-inspired-examples) |

The lifecycle example intentionally exits with status `1`. Removing its final
use produces a valid counterpart with no lifecycle finding or coverage boundary
for that supported source. It is a static-analysis fixture, not a demonstration
that the sample `close` method releases an actual resource.

Try the [rule cookbook](rules.md#run-a-bug-and-its-repair) for complete
projects with pinned settings and expected exits. Continue from the first
check with the [multi-file starter](project-guide.md).

## Keep examples useful

Examples should state the exact release required, use imports
and commands that match the current reference, and include expected findings and
exit behavior. They must not rely on running the inspected Python source. The
[development guide](development.md#maintain-the-documentation) describes how
the quick-start result and lifecycle line locations are maintained.
