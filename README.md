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
- operation input/output refs
- output transforms

Example:

```wit
/// @nexus.support python="python-validation/model_overrides.py" typescript="typescript-validation/model_overrides.ts"
package temporal:nexus@1.0.0;

world system {
  export workflow-service;
}

/// @nexus.endpoint "__temporal_system"
interface workflow-service {
  /// @nexus.proto "temporal.api.common.v1.RetryPolicy"
  /// @nexus.type python="temporalio.common.RetryPolicy" typescript="common.RetryPolicy"
  type retry-policy = string;

  /// @nexus.proto "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest"
  record signal-with-start-workflow-execution-request {
    /// @nexus.proto-field "workflow_type"
    /// @nexus.function primary=true args-field="input" python-result="collections.abc.Awaitable[typing.Any]" typescript-result="Promise<any>"
    workflow: string,
    input: option<string>,
    workflow-id: string,
    task-queue: string,
    /// @nexus.proto-field "signal_name"
    /// @nexus.function args-field="signal-input" python-result="None | collections.abc.Awaitable[None]"
    /// @nexus.with-arguments args-field="signal-input" value-type="workflow.SignalDefinition<any[]>" args-type="Value extends workflow.SignalDefinition<infer Args, any> ? Args : never" name-expr="value.name"
    signal: string,
    signal-input: option<string>,
    /// @nexus.source python="workflow.info().namespace" typescript="workflow.workflowInfo().namespace"
    namespace: option<string>,
  }

  /// @nexus.input-ref python="temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest" typescript="@temporalio/api/workflowservice/v1.SignalWithStartWorkflowExecutionRequest"
  /// @nexus.output-ref python="temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse" typescript="@temporalio/api/workflowservice/v1.SignalWithStartWorkflowExecutionResponse"
  /// @nexus.output-transform python-type="workflow.ExternalWorkflowHandle[typing.Any]" python="workflow.get_external_workflow_handle(request.workflow_id, run_id=result.run_id)" typescript-type="workflow.ExternalWorkflowHandle" typescript="workflow.getExternalWorkflowHandle(request.workflowId, result.runId ?? undefined)"
  signal-with-start-workflow-execution: func(
    request: signal-with-start-workflow-execution-request,
  ) -> string;
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
