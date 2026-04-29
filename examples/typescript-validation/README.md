# TypeScript Validation App

This example builds the checked-in sample fixture into a local `output.ts` and then typechecks a
small consumer against the generated code. The fixture keeps TypeScript-specific files under
`tests/fixtures/sample/typescript/`.

Usage:

```bash
npm install
npm run build-output
npm run typecheck
```
