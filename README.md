# nexus-api-gen

Rust console application for generating language-specific Nexus operation bindings from a YAML
definition plus a protobuf descriptor set.

Current status:

- Python generation is implemented
- TypeScript generation is implemented
- `descriptors.bin` validates referenced Python request/response proto types
- `descriptors.bin` validates referenced TypeScript request/response proto types
- generated Python includes descriptor-driven dataclasses; request models are write-only with
  `to_proto`, while other models remain bidirectional with `from_proto`
- generated TypeScript includes descriptor-driven model interfaces; request models are write-only
  with `toProto`, while other models remain bidirectional with `fromProto`
- generated service clients expose low-level operation wrappers that call `start_operation` /
  `startOperation`
- generated Python clients can call generated request-model operations either with the request
  dataclass directly or through a separate `*_args` helper typed by a generated companion
  `TypedDict`
- the generator supports top-level `types` overrides for required and omitted fields,
  language-specific whole-type substitutions such as `RetryPolicy`, `WorkflowType`, and
  `TaskQueue`, per-language sourced fields, operation output transforms, and generic model
  annotation metadata

The checked-in shared sample lives under `examples/` and includes:

- `input.yaml`
- `python-validation/model_overrides.py`
- `python-validation/output.py`
- `typescript-validation/model_overrides.ts`
- `typescript-validation/output.ts`

Example using that sample fixture:

```bash
cargo run -- generate \
  --lang python \
  --input examples/input.yaml \
  --descriptors descriptors.bin \
  --output /tmp/output.py
```

```bash
cargo run -- generate \
  --lang typescript \
  --input examples/input.yaml \
  --descriptors descriptors.bin \
  --output /tmp/output.ts
```

Each service in the YAML declares its Nexus endpoint and low-level operations:

```yaml
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      SignalWithStartWorkflowExecution:
        input:
          $python: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
          $typescript: "@temporalio/api/workflowservice/v1.SignalWithStartWorkflowExecutionRequest"
        output:
          ref:
            $python: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
            $typescript: "@temporalio/api/workflowservice/v1.SignalWithStartWorkflowExecutionResponse"
          $python:
            type: workflow.ExternalWorkflowHandle[typing.Any]
            transform: workflow.get_external_workflow_handle(request.workflow_id, run_id=result.run_id)
          $typescript:
            type: workflow.ExternalWorkflowHandle
            transform: workflow.getExternalWorkflowHandle(request.workflowId, result.runId ?? undefined)
```

Use `support` plus `types` to customize generated message and enum types. Support files are
appended into the generated output for language-specific helper code:

```yaml
support:
  $python: python-validation/model_overrides.py
  $typescript: typescript-validation/model_overrides.ts

types:
  temporal.api.common.v1.RetryPolicy:
    $python:
      type: temporalio.common.RetryPolicy
    $typescript:
      type: common.RetryPolicy
  temporal.api.common.v1.WorkflowType:
    $python:
      type: str | collections.abc.Callable[..., collections.abc.Awaitable[typing.Any]]
  temporal.api.taskqueue.v1.TaskQueue:
    $python:
      type: str
  temporal.api.common.v1.Payloads:
    $python:
      type: collections.abc.Sequence[typing.Any]
  temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest:
    omit:
      - header
      - links
    fields:
      workflow_type:
        name: workflow
      signal_name:
        name: signal
    $python:
      fields:
        namespace:
          source: workflow.info().namespace
        workflow_type:
          function:
            primary: true
            result: collections.abc.Awaitable[typing.Any]
            argsField: input
        signal_name:
          function:
            result: None | collections.abc.Awaitable[None]
            argsField: signal_input
    $typescript:
      fields:
        namespace:
          source: workflow.workflowInfo().namespace
  temporal.api.activity.v1.ActivityOptions:
    required:
      - retry_policy
  temporal.api.enums.v1.WorkflowIdReusePolicy:
    $python:
      type: temporalio.common.WorkflowIDReusePolicy
```

`fromProto` and `toProto` are optional. When omitted, the generator derives them from the proto
message or enum name:

- Python: `retry_policy_from_proto` / `retry_policy_to_proto`
- TypeScript: `retryPolicyFromProto` / `retryPolicyToProto`
- Python `Payloads` example: `payloads_from_proto` / `payloads_to_proto`

Use `required` like JSON Schema and list the proto field names that must be present in generated
message types. Presence-bearing fields validate through protobuf presence checks; string and bytes
fields validate as non-empty values. Enum type overrides reject `required` and `omit`.

Use `omit` to remove proto fields from the generated model surface entirely. Omitted fields are not
rendered into generated Python or TypeScript models, and generated `to_proto` / `from_proto`
implementations do not read or write them.

Projected `fields.<name>.name`, `fields.<name>.type`, and `fields.<name>.function` customize the
generated model metadata without changing the descriptor-driven proto conversion logic. The Python
backend currently consumes these to render generic model annotations and structured callable
metadata:

- `fields.<name>.name` renames the emitted field in generated language models and request APIs
- `$python.fields.<name>.function` marks a field as a callable/string name field and ties it to an
  args field on the same message via `argsField`
- One function may be marked `primary: true`; the generator derives a variadic type parameter for
  that function from the field name
- Additional function fields use a fixed bounded arity of 6 and omit `args`

This is useful for request-only models such as `SignalWithStartWorkflowExecutionRequest`, where
`workflow_type` / `input` and `signal_name` / `signal_input` can each describe callable-argument
relationships in YAML while still letting the generator emit stricter overloads for the unpacked
args API.

Language selectors such as `$python` and `$typescript` are now valid anywhere in the YAML. The
generator first projects the document for the selected language, deep-merging matching object
overlays and ignoring the rest, then parses that projected YAML into one ordinary spec. That means
per-language refs, support paths, transforms, and field sources all use the same `$language`
syntax.

Use `$python.fields.<name>.source` or `$typescript.fields.<name>.source` to populate a field from
the runtime instead of exposing it in the generated model for that language. Sourced fields are
serialized with the configured expression and are only supported on input-only generated models in
that language.

Use `services.<service>.operations.<operation>.output.$python` or `.$typescript` to transform an
operation result into a more native language-level handle or value. Output transforms require both
`type` and `transform`. The generated client awaits the raw operation handle, then evaluates the
transform with `request` bound to the generated request value and `result` bound to the raw
operation result for that language.

When an operation input uses a generated Python request dataclass, the generated Python client
emits the normal request-taking method plus a separate `*_args` helper that accepts unpacked
keyword arguments typed by a generated `<RequestName>Args` `TypedDict`. Whole-type overrides such
as `RetryPolicy` continue to use the direct request value only.

The YAML schema is checked in at [input.schema.json](/Users/tconley/nexus-api-gen/input.schema.json).

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
