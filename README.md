# nexus-api-gen

Rust CLI for generating language-specific Nexus operation bindings from a WIT definition plus a protobuf descriptor set.

Current status:

- Python generation is implemented
- TypeScript generation is implemented
- request models are write-only
- response and nested models remain bidirectional where generated
- support files, native type substitutions, sourced fields, function/argument pairing, and output transforms are all driven from WIT `@nexus` directives

Examples are organized by authored input WIT plus per-language example suites:

- `examples/inputs/workflow-service.wit`
- `examples/python/workflow-service/output.py`
- `examples/python/workflow-service/test_workflow_service.py`
- `examples/typescript/workflow-service/output.ts`
- `examples/typescript/workflow-service/typecheck.ts`
- `examples/typescript/workflow-service/output.test.ts`

Generate from the shared example input:

```bash
cargo run -- generate \
  --lang python \
  --input examples/inputs/workflow-service.wit \
  --descriptors examples/descriptors/temporal_api.bin \
  --output /tmp/output.py
```

```bash
cargo run -- generate \
  --lang typescript \
  --input examples/inputs/workflow-service.wit \
  --descriptors examples/descriptors/temporal_api.bin \
  --output /tmp/output.ts
```

Add `--format` to run a formatter after generation:

- Python: `ruff format`
- TypeScript: `prettier --write`

```bash
cargo run -- generate \
  --lang python \
  --input examples/inputs/workflow-service.wit \
  --descriptors examples/descriptors/temporal_api.bin \
  --output /tmp/output.py \
  --format
```

Generate WIT for a proto RPC from the descriptor set:

```bash
cargo run -- add-rpc \
  --descriptors examples/descriptors/temporal_api.bin \
  --rpc SignalWithStartExecution
```

Write the standalone WIT scaffold to a file instead of stdout:

```bash
cargo run -- add-rpc \
  --descriptors examples/descriptors/temporal_api.bin \
  --rpc temporal.api.workflowservice.v1.WorkflowService.SignalWithStartWorkflowExecution \
  --output /tmp/add-rpc.wit
```

Extend an existing WIT file with a new RPC:

```bash
cargo run -- add-rpc \
  --descriptors examples/descriptors/temporal_api.bin \
  --rpc SignalWorkflowExecution \
  --input examples/inputs/workflow-service.wit
```

Rewrite the existing WIT file in place by pointing `--output` at the same path:

```bash
cargo run -- add-rpc \
  --descriptors examples/descriptors/temporal_api.bin \
  --rpc SignalWorkflowExecution \
  --input examples/inputs/workflow-service.wit \
  --output examples/inputs/workflow-service.wit
```

Write the prepared WIT workspace the loader actually parses, including repo-provided builtins under `deps/`:

```bash
cargo run -- debug-wit-dir \
  --input examples/inputs/workflow-service.wit \
  --output /tmp/workflow-service-wit
```

The WIT file defines the public surface. `@nexus` directives carry the parts WIT does not express directly:

- support file paths
- proto type and field mapping
- language-native override types
- sourced field expressions
- function and paired-argument metadata
- output transforms

The tool also ships a bundled WIT package of reusable semantic/common types:

- `nexus:temporal-types/model@1.0.0`

That bundled package can also contribute shared support snippets. Input WIT files can add their own extra support with `@nexus.support`, and all selected support fragments are concatenated into the generated output in declaration order.

Example:

```wit
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
    /// @nexus.source
    ///   python="workflow.info().namespace"
    ///   typescript="workflow.workflowInfo().namespace"
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

Validate the Python examples:

```bash
cd examples/python
uv run build_outputs.py
uv run pytest
uv run basedpyright
```

Validate the TypeScript examples:

```bash
cd examples/typescript
npm install
npm run build-outputs
npm run test
npm run typecheck
```
