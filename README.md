# nexus-api-gen

Rust console application for generating language-specific Nexus API bindings from a YAML
definition plus a protobuf descriptor set.

Current status:

- Python generation is implemented
- `descriptors.bin` is used to validate referenced Python request/response proto types
- generated Python now includes top-level request/response dataclasses with `to_proto` and
  `from_proto` helpers
- generated Python now includes thin service client wrappers that call `start_operation`
- the generator is structured so additional language backends can be added without changing
  the CLI contract

The checked-in sample fixture lives under
`tests/fixtures/python_sample/` and includes:

- `input.yaml`
- `python_support.py`
- golden `output.py`

Example using that sample fixture:

```bash
cargo run -- generate \
  --lang python \
  --input tests/fixtures/python_sample/input.yaml \
  --descriptors descriptors.bin \
  --python-support tests/fixtures/python_sample/python_support.py \
  --output /tmp/output.py
```

Each service in the YAML can declare its Nexus endpoint, for example:

```yaml
services:
  WorkflowService:
    endpoint: __temporal_system
```

The YAML schema is checked in at [input.schema.json](/Users/tconley/nexus-api-gen/input.schema.json).

If `services.*.apis` are present, pass a Python support file whose converter symbols are named by
the YAML. That file is appended to the generated module so the ergonomic client methods can call
those converters directly.

To validate the sample fixture with a small `uv` sample app:

```bash
cd examples/python-validation
uv run build_output.py
uv run main.py
uv run basedpyright
```
