# Language round-trip tests

These validate that the **generated code actually works** in each target: it
compiles against the real Nexus/Temporal SDKs and round-trips values through the
same data path the Temporal data converter uses.

| Language | Data path exercised |
|---|---|
| Python | Temporal's `pydantic_data_converter` (type hints) |
| Go | `encoding/json` (the Go SDK's default payload converter) |
| TypeScript | the generated `parse`/`serialize` helpers (the TS SDK has no type hints, so a converter drives these) |
| Java | a stock Jackson `ObjectMapper` (the Java SDK's default converter) |

They are driven by `tests/lang_roundtrip.rs`, which generates `chat.nexusrpc.yaml`
into a temp dir, drops the harness file from here alongside it, and runs the
toolchain. Each asserts the same feature matrix: `const` discriminator,
off-the-wire scalar `default`, optional-vs-nullable-vs-required serialization,
open-struct extra preservation, closed-struct rejection, typed maps, and
missing-required / const-violation rejection.

## Running

They are `#[ignore]`d because they need the language toolchains **and** network
access to fetch the SDKs (Nexus SDK, Temporal, Jackson, …):

```bash
cargo test -p nex-gen-json-schema --test lang_roundtrip -- --ignored --nocapture
```

Prerequisites: `python3`, `go`, `node`/`npm`, and a JDK + `mvn` on `PATH`.
Dependency downloads are cached under `$TMPDIR/nexgen-lang-roundtrip/<lang>` so
re-runs are fast.
