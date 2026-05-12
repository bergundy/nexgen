# Python Validation Sample

Small `uv`-managed app for validating the generated Python output.

The build step runs `nexus-api-gen` against the shared sample spec in
`examples/input.wit` and writes a fresh `output.py` into this directory so the
application and `basedpyright` both validate the same generated module. The
support helpers for that shared spec live alongside this example in
`model_overrides.py`.

The sample validates:

- generated operation metadata and registry contents
- required-field validation in generated dataclasses
- language-specific whole-message overrides for `RetryPolicy`
- support-file helpers appended into the generated module
- low-level service client wrappers calling `start_operation`
- round-trip proto conversion for generated models and override helpers

Run it with:

```bash
cd examples/python-validation
uv run build_output.py
uv run main.py
uv run basedpyright
```
