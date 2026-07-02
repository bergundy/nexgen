//! Snapshot tests: generate all four languages from the canonical
//! `chat.nexusrpc.yaml` and assert the output matches the locked samples under
//! `spec/samples/`.
//!
//! Run with `UPDATE_SNAPSHOTS=1 cargo test -p nex-gen-json-schema --test snapshot`
//! to (re)write the samples to the generator's current output — used once to
//! lock the first-round output, then the samples must not change.

use std::path::{Path, PathBuf};

use nex_gen_core::Language;

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn input() -> PathBuf {
    crate_dir().join("spec/samples/chat.nexusrpc.yaml")
}

fn samples_dir() -> PathBuf {
    crate_dir().join("spec/samples")
}

fn updating() -> bool {
    std::env::var("UPDATE_SNAPSHOTS").is_ok()
}

/// Compare `actual` against the sample file at `rel` (relative to samples/),
/// or rewrite it when updating.
fn check_file(rel: &str, actual: &str) {
    let path = samples_dir().join(rel);
    if updating() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("missing snapshot {}: {e}", path.display()));
    assert_eq!(
        expected, actual,
        "snapshot mismatch for {rel} (run with UPDATE_SNAPSHOTS=1 to relock)"
    );
}

fn generate(language: Language) -> nex_gen_core::GeneratedFiles {
    nex_gen_json_schema::generate(vec![input()], language)
        .unwrap_or_else(|e| panic!("generation failed for {language}: {e}"))
}

/// Assert every generated file matches its snapshot under `samples/<subdir>/`.
fn check_lang(language: Language, subdir: &str) {
    let generated = generate(language);
    for (rel, body) in &generated.files {
        let snap = Path::new(subdir).join(rel);
        check_file(snap.to_str().unwrap(), body);
    }
}

#[test]
fn go_snapshot() {
    check_lang(Language::Go, "go");
}

#[test]
fn typescript_snapshot() {
    check_lang(Language::TypeScript, "typescript");
}

#[test]
fn python_snapshot() {
    check_lang(Language::Python, "python");
}

#[test]
fn java_snapshot() {
    check_lang(Language::Java, "java");
}
