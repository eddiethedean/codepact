# Examples

Each example has a narrow purpose. Run the inspection and lifecycle examples
with the published 0.2.0 release.

| Example | What it demonstrates | How to run |
| --- | --- | --- |
| [`examples/contracts.py`](https://github.com/eddiethedean/typepact/blob/main/examples/contracts.py) | Resource, `Borrowed`, and `Owned` declarations | `uv run typepact inspect-contracts --root . examples/contracts.py` |
| [`docs/_examples/connection.py`](https://github.com/eddiethedean/typepact/blob/main/docs/_examples/connection.py) | Quick-start contract facts and JSON snapshot | `uv run typepact inspect-contracts --root docs/_examples docs/_examples/connection.py` |
| [`docs/_examples/project/`](https://github.com/eddiethedean/typepact/tree/main/docs/_examples/project) | Direct absolute imports across supplied modules | See the [multi-file tutorial](project-guide.md) |
| [`examples/lifecycle.py`](https://github.com/eddiethedean/typepact/blob/main/examples/lifecycle.py) | Supported use-after-close finding with the related close location | `uv run typepact check --root examples` |

The lifecycle example intentionally exits with status `1`. Removing its final
use produces a valid counterpart with no lifecycle finding or coverage boundary
for that supported source. It is a static-analysis fixture, not a demonstration
that the sample `close` method releases an actual resource.

## Keep examples useful

Examples should state the exact release required, use imports
and commands that match the current reference, and include expected findings and
exit behavior. They must not rely on running the inspected Python source. The
[development guide](development.md#maintain-the-documentation) describes how
the quick-start result and lifecycle line locations are maintained.
