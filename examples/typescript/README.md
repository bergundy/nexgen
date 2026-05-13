# TypeScript Examples

Shared Node/TypeScript example suite for generated outputs.

- Authored WIT inputs live in `examples/inputs/*.wit`
- TypeScript-specific support files, checked-in generated outputs, vitest files,
  and typecheck files live in `examples/typescript/<example-id>/`
- `build_outputs.mjs` regenerates `output.ts` for every TypeScript example by
  default

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
