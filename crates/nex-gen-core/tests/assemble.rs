//! Assembly test.
//!
//! Each emitter renders complete file bodies (import blocks included), so
//! [`assemble`] is purely structural: it collects the emitted files by path and
//! picks the output layout. (Per-language `render_imports` shaping is unit-tested
//! inside each `render/*.rs` module.)

use std::path::PathBuf;

use nex_gen_core::emit::EmittedFile;
use nex_gen_core::ir::{IR, SymbolTable};
use nex_gen_core::{GeneratedOutputLayout, Language, assemble, traits::Emitter};

/// A trivial frontend kind — symbol kinds are frontend-defined, so the core's
/// own tests define their own. Empty here: these tests drive `assemble` from an
/// emitter that ignores the IR.
struct TestKind;

/// An emitter that returns two prebuilt files, exercising the collect + layout
/// path without touching the IR.
struct TwoFileEmitter;

impl Emitter<TestKind> for TwoFileEmitter {
    fn language(&self) -> Language {
        Language::TypeScript
    }

    fn emit(&self, _ir: &IR<TestKind>) -> nex_gen_core::Result<Vec<EmittedFile>> {
        Ok(vec![
            EmittedFile {
                path: PathBuf::from("models.ts"),
                body: "export interface Extra {}".to_string(),
            },
            EmittedFile {
                path: PathBuf::from("service.ts"),
                body: "export const svc = {};".to_string(),
            },
        ])
    }
}

/// A single-file emitter, to exercise the single-file layout branch.
struct OneFileEmitter;

impl Emitter<TestKind> for OneFileEmitter {
    fn language(&self) -> Language {
        Language::TypeScript
    }

    fn emit(&self, _ir: &IR<TestKind>) -> nex_gen_core::Result<Vec<EmittedFile>> {
        Ok(vec![EmittedFile {
            path: PathBuf::from("out.ts"),
            body: "export const x = 1;".to_string(),
        }])
    }
}

#[test]
fn assemble_collects_files_and_picks_directory_layout() {
    let ir = IR::new(SymbolTable::<TestKind>::new());
    let generated = assemble(&ir, &TwoFileEmitter).expect("assemble");

    // Two distinct paths -> directory layout, bodies passed through verbatim.
    assert_eq!(generated.layout, GeneratedOutputLayout::Directory);
    assert_eq!(
        generated.files.get(&PathBuf::from("models.ts")).unwrap(),
        "export interface Extra {}"
    );
    assert_eq!(
        generated.files.get(&PathBuf::from("service.ts")).unwrap(),
        "export const svc = {};"
    );
}

#[test]
fn assemble_picks_single_file_layout() {
    let ir = IR::new(SymbolTable::<TestKind>::new());
    let generated = assemble(&ir, &OneFileEmitter).expect("assemble");
    assert_eq!(generated.layout, GeneratedOutputLayout::SingleFile);
}
