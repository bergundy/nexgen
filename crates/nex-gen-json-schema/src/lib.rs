//! `nex-gen-json-schema` — a JSON Schema 2020-12 → Nexus code generator.
//!
//! A front-end over [`nex_gen_core`]: the [`loader`] parses YAML/JSON definition
//! files, enforces the strict JSON Schema subset, and lowers them into the core
//! IR; the per-language [`emit`]ters render Go, Java, Python, and TypeScript
//! models, runtime validators, and Nexus service bindings.

pub mod emit;
pub mod ir;
pub mod loader;
pub mod naming;
pub mod schema;

use std::collections::BTreeMap;
use std::path::PathBuf;

pub use ir::Kind;
pub use loader::SchemaLoader;

use nex_gen_core::{Emitter, GeneratedFiles, Language, Loader, Result};

pub use emit::PackageInfo;

/// Build the emitter for a language over the derived package info.
fn emitter_for(language: Language, pkg: PackageInfo) -> Box<dyn Emitter<Kind>> {
    match language {
        Language::Go => emit::go::emitter(pkg),
        Language::Java => emit::java::emitter(pkg),
        Language::Python => emit::python::emitter(pkg),
        Language::TypeScript => emit::typescript::emitter(pkg),
        _ => emit::go::emitter(pkg),
    }
}

/// Load `inputs`, lower them for `language`, and render the output files.
///
/// The single implementation the CLI and in-process API both use (P16). Output
/// is always a directory layout keyed by each emitter's output-relative path
/// (a package-dir language like Python needs `chat/__init__.py`, which a
/// flat single-file layout could not express).
pub fn generate(inputs: Vec<PathBuf>, language: Language) -> Result<GeneratedFiles> {
    let pkg = inputs
        .first()
        .map(|p| PackageInfo::from_input(p))
        .unwrap_or_else(|| PackageInfo {
            name: "schema".to_string(),
            java_package: "com.example.schema".to_string(),
        });
    let loader = SchemaLoader::new(inputs);
    let loaded = loader.load(language)?;
    let emitter = emitter_for(language, pkg);
    let files = emitter.emit(&loaded.ir)?;

    let mut map: BTreeMap<PathBuf, String> = BTreeMap::new();
    for file in files {
        map.insert(file.path, file.body);
    }
    let mut generated = GeneratedFiles::directory(map);
    generated.warnings = loaded.warnings;
    Ok(generated)
}
