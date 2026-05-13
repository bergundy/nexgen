# Python Examples

Shared `uv`-managed Python example suite for generated outputs.

- Authored WIT inputs live in `examples/inputs/*.wit`
- Python-specific support files, checked-in generated outputs, and pytest files
  live in `examples/python/<example-id>/`
- `build_outputs.py` regenerates `output.py` for every Python example by default

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
