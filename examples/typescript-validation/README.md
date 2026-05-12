# TypeScript Validation App

This example builds the shared sample spec in `examples/input.wit` into the
checked-in `output.ts` in this directory and then typechecks a small consumer
against the generated code. The TypeScript support helpers for that shared spec
live alongside this example in `model_overrides.ts`.

The sample focuses on low-level operation wrappers, required generated fields,
support-file helpers appended into the generated output, and the `RetryPolicy`
whole-message override.

Usage:

```bash
npm install
npm run build-output
npm run typecheck
```
