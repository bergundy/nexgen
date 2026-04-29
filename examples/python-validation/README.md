# Python Validation Sample

Small `uv`-managed app for validating the generated Python output.

The build step runs `nexus-api-gen` against the sample fixture in
`tests/fixtures/sample/` and writes a fresh local `output.py` build artifact into this
example directory so the application and `basedpyright` both validate the same generated module.
The fixture keeps Python-specific files under `tests/fixtures/sample/python/`.

The sample validates:

- the generated `Operation` metadata and registry
- request dataclass `to_proto()` conversion
- the generated service client wrapper calling `start_operation`
- response dataclass conversion from the returned proto
- the generated ergonomic API method and appended converter support code

Run it with:

```bash
cd examples/python-validation
uv run build_output.py
uv run main.py
uv run basedpyright
```
