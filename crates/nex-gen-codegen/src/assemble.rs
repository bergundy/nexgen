//! Final assembly: emit -> group by module -> resolve imports -> render -> stitch.
//!
//! Because `refs` are explicit on every symbol and `module` is computed per
//! symbol by the emitter, **reachability, placement, import resolution, and
//! dedup are base-owned**: `module` comparison drives both which file a symbol
//! lands in and whether a cross-module reference needs an import (same module
//! => none). Foreign references and same-module exclusion fall out uniformly.

use std::collections::{BTreeMap, BTreeSet};

use crate::emit::{EmittedFile, Import, Module};
use crate::error::Result;
use crate::ir::SymbolTable;
use crate::output::GeneratedFiles;
use crate::render::render_imports;
use crate::traits::Emitter;

/// Drive an emitter to a [`GeneratedFiles`] result.
///
/// Steps: ask the emitter to [`emit`](Emitter::emit) files; group them by
/// [`Module`]; for each file resolve its cross-module imports (from symbol
/// `refs` + module comparison, with same-module exclusion); render the import
/// block via [`render_imports`] and stitch it onto the body; collect into
/// [`GeneratedFiles`].
pub fn assemble(symbols: &SymbolTable, emitter: &dyn Emitter) -> Result<GeneratedFiles> {
    let language = emitter.language();

    // 1. Emit. The emitter owns layout and produces bodies without import blocks.
    let emitted: Vec<EmittedFile> = emitter.emit(symbols);

    // 2. Group emitted files by module (placement). This is where multiple
    //    emitted fragments for the same module would coalesce.
    // TODO(prototype): import resolution flow — the emitter may already attach
    //    fully-resolved `imports` to each EmittedFile, in which case grouping
    //    is the identity and step 3 is a no-op. The alternative is the base
    //    resolving imports here from `refs` + `module`. Both honor "resolution
    //    outside the emitter"; pick by what reads cleanly. See
    //    json-schema/integration-plan.md "Open items".
    let mut by_module: BTreeMap<Module, Vec<EmittedFile>> = BTreeMap::new();
    for file in emitted {
        by_module.entry(file.module.clone()).or_default().push(file);
    }

    // 3. For each file, resolve + render its import block and stitch it on.
    let mut files: BTreeMap<std::path::PathBuf, String> = BTreeMap::new();
    for (module, group) in by_module {
        for file in group {
            let imports = resolve_imports(symbols, &module, &file);
            let import_block = render_imports(language, &imports);
            let stitched = stitch(&import_block, &file.body);
            files.insert(file.path.clone(), stitched);
        }
    }

    // TODO(prototype): choose layout (single-file vs. directory) from the
    //    emitter's output. For now infer from file count; emitters that always
    //    want one shape can override once the prototype settles. See
    //    json-schema/integration-plan.md "Open items".
    let generated = if files.len() == 1 {
        let body = files.into_values().next().expect("one file");
        GeneratedFiles::single_file(body)
    } else {
        GeneratedFiles::directory(files)
    };

    Ok(generated)
}

/// Resolve the cross-module imports a file needs.
///
/// Driven entirely by `refs` + `module` comparison: a referenced symbol placed
/// in a *different* module yields an import; a same-module reference yields
/// none (same-module exclusion). Foreign references are just `Type` symbols in
/// a foreign module, so they resolve through the same path. Dedup is by the
/// structured [`Import`].
fn resolve_imports(_symbols: &SymbolTable, _own_module: &Module, file: &EmittedFile) -> Vec<Import> {
    // TODO(prototype): import resolution flow — if emitters pre-resolve imports
    //    onto EmittedFile, this just dedups `file.imports`. If the base
    //    resolves, walk each emitted symbol's `refs`, look up each ref's
    //    module via the emitter's NameResolver, skip same-module refs, and
    //    collect the cross-module bindings. See json-schema/integration-plan.md
    //    "Open items". Placeholder: dedup whatever the emitter already attached.
    let mut seen = BTreeSet::new();
    file.imports
        .iter()
        .filter(|import| import.module != *_own_module) // same-module exclusion
        .filter(|import| seen.insert((*import).clone())) // dedup
        .cloned()
        .collect()
}

/// Stitch a rendered import block in front of a body.
fn stitch(import_block: &str, body: &str) -> String {
    if import_block.is_empty() {
        body.to_string()
    } else {
        format!("{import_block}\n\n{body}")
    }
}
