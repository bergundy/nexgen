# Python Examples

Shared `uv`-managed Python example suite for generated outputs.

- Authored WIT inputs live in `examples/inputs/*.wit`
- Python-specific support files, checked-in generated outputs, and pytest files
  live in `examples/python/<package_name>/`
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
