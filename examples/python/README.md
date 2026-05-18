# Python Examples

Shared `uv`-managed Python 3.10+ example suite for generated outputs.

- Authored WIT inputs live in `examples/inputs/*.wit`
- Checked-in generated packages live in `examples/python/<package_name>/`,
  where `<package_name>` is the snake_case WIT input name
- Generated support fragments live under each package's private `_support/`
  package, and generated models are exposed as the public `models` module
- Pytest files live in `examples/python/tests/`
- `build_outputs.py` is a thin wrapper around `cargo build-examples --lang python`
- `cargo test` validates the checked-in generated packages and does not rebuild them

Top-level rebuild command:

```bash
cargo build-examples --lang python
```

Current workflow:

```bash
cd examples/python
uv run build_outputs.py
uv run pytest
uv run basedpyright
```

To rebuild one example only:

```bash
cd examples/python
uv run build_outputs.py workflow-service
```

Set `NEXUS_API_GEN_BIN=/path/to/nexus-api-gen` to make `build_outputs.py` use an already-built binary instead of the cargo alias.
