//! End-to-end language round-trip tests: generate each target from the
//! canonical `chat.nexusrpc.yaml`, then compile and round-trip the output
//! through that language's real Temporal data path.
//!
//! These are `#[ignore]`d because they require the language toolchains (Go,
//! Node, Python, JDK+Maven) **and** network access to fetch the Temporal /
//! Nexus SDKs. Run them explicitly:
//!
//! ```text
//! cargo test -p nex-gen-json-schema --test lang_roundtrip -- --ignored --nocapture
//! ```
//!
//! Work happens under a persistent per-language dir in the temp dir so fetched
//! dependencies (venv, node_modules, Go module cache, Maven `~/.m2`) are reused
//! across runs.
//!
//! - **Python** round-trips through Temporal's `pydantic_data_converter`.
//! - **Go** round-trips through `encoding/json` (the Go SDK's default converter).
//! - **TypeScript** round-trips through the generated `parse`/`serialize`
//!   helpers (the TS SDK has no type hints, so a converter uses these).
//! - **Java** round-trips through a stock Jackson `ObjectMapper` (the Java SDK's
//!   default converter).

use std::path::{Path, PathBuf};
use std::process::Command;

use nex_gen_core::Language;

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn input() -> PathBuf {
    manifest().join("spec/samples/chat.nexusrpc.yaml")
}

fn lang_asset(rel: &str) -> String {
    let path = manifest().join("tests/lang").join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// A persistent scratch dir for a language (dependency caches survive re-runs).
fn workdir(lang: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("nexgen-lang-roundtrip").join(lang);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Generate `language` from the chat schema into `dir` (directory layout).
fn generate_into(dir: &Path, language: Language) {
    let generated = nex_gen_json_schema::generate(vec![input()], language)
        .unwrap_or_else(|e| panic!("generate {language}: {e}"));
    for (rel, body) in &generated.files {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, body).unwrap();
    }
}

fn write(dir: &Path, rel: &str, body: &str) {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, body).unwrap();
}

/// Run a command in `dir`, returning combined stdout+stderr; panics on spawn
/// failure with a hint that the toolchain may be missing.
fn run(program: &str, args: &[&str], dir: &Path) -> (bool, String) {
    let output = Command::new(program)
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| {
            panic!("failed to spawn `{program}` (is it installed?): {e}")
        });
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    (output.status.success(), combined)
}

#[test]
#[ignore = "requires python3 + network (pydantic, temporalio)"]
fn python_roundtrip() {
    let dir = workdir("python");
    generate_into(&dir, Language::Python);
    write(&dir, "test_roundtrip.py", &lang_asset("python/test_roundtrip.py"));

    let venv = dir.join("venv");
    if !venv.exists() {
        let (ok, out) = run("python3", &["-m", "venv", "venv"], &dir);
        assert!(ok, "venv creation failed:\n{out}");
    }
    let pip = venv.join("bin/pip");
    let py = venv.join("bin/python");
    let (ok, out) = run(
        pip.to_str().unwrap(),
        &["install", "--quiet", "--disable-pip-version-check", "pydantic", "temporalio"],
        &dir,
    );
    assert!(ok, "pip install failed:\n{out}");

    let (ok, out) = run(py.to_str().unwrap(), &["test_roundtrip.py"], &dir);
    assert!(ok && out.contains("PYTHON ROUND-TRIP OK"), "python round-trip failed:\n{out}");
}

#[test]
#[ignore = "requires the go toolchain + network (nexus sdk-go)"]
fn go_roundtrip() {
    let dir = workdir("go");
    generate_into(&dir, Language::Go);
    write(&dir, "go.mod", "module chatgen\n\ngo 1.24\n");
    write(&dir, "roundtrip_test.go", &lang_asset("go/roundtrip_test.go"));

    let (ok, out) = run("go", &["get", "github.com/nexus-rpc/sdk-go/nexus@latest"], &dir);
    assert!(ok, "go get failed:\n{out}");
    let (ok, out) = run("go", &["test", "./..."], &dir);
    assert!(ok, "go round-trip failed:\n{out}");
}

#[test]
#[ignore = "requires node/npm + network (typescript, nexus-rpc)"]
fn typescript_roundtrip() {
    let dir = workdir("typescript");
    generate_into(&dir, Language::TypeScript);
    write(&dir, "roundtrip.ts", &lang_asset("typescript/roundtrip.ts"));
    write(
        &dir,
        "package.json",
        r#"{ "name": "chatgen", "version": "1.0.0", "private": true }"#,
    );
    write(
        &dir,
        "tsconfig.json",
        r#"{
  "compilerOptions": {
    "strict": true,
    "target": "ES2020",
    "module": "commonjs",
    "moduleResolution": "node",
    "ignoreDeprecations": "6.0",
    "noEmit": true,
    "skipLibCheck": true
  },
  "files": ["index.ts", "roundtrip.ts"]
}"#,
    );

    let (ok, out) = run(
        "npm",
        &["install", "--silent", "typescript", "ts-node", "nexus-rpc", "@types/node"],
        &dir,
    );
    assert!(ok, "npm install failed:\n{out}");
    // Type-check the generated module + harness under strict mode.
    let (ok, out) = run("npx", &["tsc"], &dir);
    assert!(ok, "tsc type-check failed:\n{out}");
    // Execute the round-trip.
    let (ok, out) = run("npx", &["ts-node", "roundtrip.ts"], &dir);
    assert!(ok && out.contains("TS ROUND-TRIP OK"), "ts round-trip failed:\n{out}");
}

#[test]
#[ignore = "requires JDK + Maven + network (jackson, jspecify, nexus-sdk)"]
fn java_roundtrip() {
    let dir = workdir("java");
    // Generated model classes go under src/main/java/<pkg>/...
    let main = dir.join("src/main/java");
    std::fs::create_dir_all(&main).unwrap();
    generate_into(&main, Language::Java);
    write(&dir, "pom.xml", &lang_asset("java/pom.xml"));
    write(
        &dir,
        "src/test/java/com/example/chat/RoundTripTest.java",
        &lang_asset("java/RoundTripTest.java"),
    );

    let (ok, out) = run("mvn", &["-q", "-B", "test"], &dir);
    assert!(ok, "java round-trip failed:\n{out}");
}
