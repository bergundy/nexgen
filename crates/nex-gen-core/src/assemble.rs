//! Final assembly: emit -> collect by path -> choose layout.
//!
//! Each emitter renders complete file bodies (including import blocks) via the
//! per-language [`render`](crate::render) helpers, so assembly is purely
//! structural: collect the emitted files by path and pick the output layout.

use std::collections::BTreeMap;

use crate::emit::EmittedFile;
use crate::error::Result;
use crate::ir::IR;
use crate::output::GeneratedFiles;
use crate::traits::Emitter;

/// Drive an emitter to a [`GeneratedFiles`] result.
///
/// Ask the emitter to [`emit`](Emitter::emit) its files (bodies rendered in
/// full, including import blocks), collect them by path, and choose the layout
/// from the set of distinct paths: a single path is a single-file generation,
/// anything else is a directory tree.
pub fn assemble<K>(ir: &IR<K>, emitter: &dyn Emitter<K>) -> Result<GeneratedFiles> {
    let emitted: Vec<EmittedFile> = emitter.emit(ir)?;

    let mut files: BTreeMap<std::path::PathBuf, String> = BTreeMap::new();
    for file in emitted {
        files.insert(file.path, file.body);
    }

    let generated = if files.len() == 1 {
        let body = files.into_values().next().expect("one file");
        GeneratedFiles::single_file(body)
    } else {
        GeneratedFiles::directory(files)
    };

    Ok(generated)
}
