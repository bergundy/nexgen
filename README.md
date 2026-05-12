# nexus-api-gen

Rust CLI for generating language-specific Nexus operation bindings from a WIT definition plus a protobuf descriptor set.

Current status:

- Python generation is implemented
- TypeScript generation is implemented
- request models are write-only
- response and nested models remain bidirectional where generated
- support files, native type substitutions, sourced fields, function/argument pairing, and output transforms are all driven from WIT `@nexus` directives

The shared sample lives under `examples/`:

- `input.wit`
- `python-validation/model_overrides.py`
- `python-validation/output.py`
- `typescript-validation/model_overrides.ts`
- `typescript-validation/output.ts`

Generate from the sample:

```bash
cargo run -- generate \
  --lang python \
  --input examples/input.wit \
  --descriptors descriptors.bin \
  --output /tmp/output.py
```

```bash
cargo run -- generate \
  --lang typescript \
  --input examples/input.wit \
  --descriptors descriptors.bin \
  --output /tmp/output.ts
```

Add `--format` to run a formatter after generation:

- Python: `ruff format`
- TypeScript: `prettier --write`

```bash
cargo run -- generate \
  --lang python \
  --input examples/input.wit \
  --descriptors descriptors.bin \
  --output /tmp/output.py \
  --format
```

The WIT file defines the public surface. `@nexus` directives carry the parts WIT does not express directly:

- support file paths
- proto type and field mapping
- language-native override types
- sourced field expressions
- function and paired-argument metadata
- operation output refs
- output transforms

The tool also ships a bundled WIT package of reusable semantic/common types:

- `nexus:temporal-types/model@1.0.0`

Example:

```wit
/// @nexus.support python="python-validation/model_overrides.py" typescript="typescript-validation/model_overrides.ts"
package temporal:nexus@1.0.0;

world system {
  export workflow-service;
}

/// @nexus.endpoint "__temporal_system"
interface workflow-service {
  use nexus:temporal-types/model@1.0.0.{retry-policy, signal-function, workflow-function};

  /// @nexus.proto "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest"
  record signal-with-start-workflow-execution-request {
    /// @nexus.proto-field "workflow_type"
    workflow: workflow-function,
    workflow-id: string,
    task-queue: string,
    /// @nexus.proto-field "signal_name"
    signal: signal-function,
    /// @nexus.source python="workflow.info().namespace" typescript="workflow.workflowInfo().namespace"
    namespace: option<string>,
  }

  /// @nexus.proto "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse"
  record signal-with-start-workflow-execution-response {
    run-id: option<string>,
  }

  /// @nexus.output-transform
  ///   python-type="workflow.ExternalWorkflowHandle[typing.Any]"
  ///   python="workflow.get_external_workflow_handle(request.workflow_id, run_id=result.run_id)"
  ///   typescript-type="workflow.ExternalWorkflowHandle"
  ///   typescript="workflow.getExternalWorkflowHandle(request.workflowId, result.runId ?? undefined)"
  signal-with-start-workflow-execution: func(
    request: signal-with-start-workflow-execution-request,
  ) -> signal-with-start-workflow-execution-response;
}
```

Validate the sample apps:

```bash
cd examples/python-validation
uv run build_output.py
uv run main.py
uv run basedpyright
```

```bash
cd examples/typescript-validation
npm install
npm run build-output
npm run typecheck
```
