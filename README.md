# nexus-api-gen

Rust console application for generating language-specific Nexus API bindings from a YAML
definition plus a protobuf descriptor set.

Current status:

- Python generation is implemented
- TypeScript generation is implemented
- `descriptors.bin` is used to validate referenced Python request/response proto types
- `descriptors.bin` is used to validate referenced TypeScript request/response proto types
- generated Python now includes top-level request/response dataclasses with `to_proto` and
  `from_proto` helpers
- generated Python now includes thin service client wrappers that call `start_operation`
- generated TypeScript now includes recursive model interfaces, `fromProto` / `toProto`
  helpers, `nexus.service(...)` definitions, and workflow-side client wrappers
- the generator is structured so additional language backends can be added without changing
  the CLI contract

The checked-in sample fixture lives under
`tests/fixtures/sample/` and includes:

- `input.yaml`
- `python/support.py`
- `python/output.py`
- `typescript/support.ts`
- `typescript/output.ts`

Example using that sample fixture:

```bash
cargo run -- generate \
  --lang python \
  --input tests/fixtures/sample/input.yaml \
  --descriptors descriptors.bin \
  --output /tmp/output.py
```

```bash
cargo run -- generate \
  --lang typescript \
  --input tests/fixtures/sample/input.yaml \
  --descriptors descriptors.bin \
  --output /tmp/output.ts
```

Each service in the YAML can declare its Nexus endpoint, for example:

```yaml
services:
  WorkflowService:
    endpoint: __temporal_system
```

The YAML schema is checked in at [input.schema.json](/Users/tconley/nexus-api-gen/input.schema.json).

If `services.*.apis` are present, declare `support.$pythonFile` and/or `support.$typescriptFile`
in the YAML. Those files are resolved relative to the YAML file and included in the generated
module so the ergonomic client methods can call the converter symbols named by the spec directly.

To validate the sample fixture with a small `uv` sample app:

```bash
cd examples/python-validation
uv run build_output.py
uv run main.py
uv run basedpyright
```

To validate the sample fixture with a small TypeScript typecheck app:

```bash
cd examples/typescript-validation
npm install
npm run build-output
npm run typecheck
```
