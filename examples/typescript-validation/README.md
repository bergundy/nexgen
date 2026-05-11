# TypeScript Validation App

This example builds the checked-in sample fixture into a local `output.ts` and then typechecks a
small consumer against the generated code. The sample focuses on low-level operation wrappers,
required generated fields, support-file helpers appended into the generated output, and the
`RetryPolicy` whole-message override.

Usage:

```bash
npm install
npm run build-output
npm run typecheck
```
