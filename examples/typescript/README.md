# TypeScript Examples

Shared Node/TypeScript example suite for generated outputs.

- Authored WIT inputs live in `examples/inputs/*.wit`
- Checked-in generated outputs live in `examples/typescript/<example-id>/`
- Generated support fragments are emitted as `support.ts` next to the
  generated `index.ts`
- Vitest files live in `examples/typescript/tests/`
- `build_outputs.mjs` is a thin wrapper around
  `cargo build-examples --lang typescript`

Top-level rebuild command:

```bash
cargo build-examples --lang typescript
```

Current workflow:

```bash
cd examples/typescript
npm install
npm run build-outputs
npm run test
npm run typecheck
```

To rebuild one example only:

```bash
cd examples/typescript
node build_outputs.mjs workflow-service
```

Set `NEXUS_API_GEN_BIN=/path/to/nexus-api-gen` to make `build_outputs.mjs` use an already-built binary instead of the cargo alias.
