//! Final assembly: emit -> group by module -> resolve imports -> render -> stitch.
//!
//! Because `refs` are explicit on every emitted file and `module` is computed
//! per symbol by the emitter (surfaced through its
//! [`NameResolver`](crate::NameResolver)), **reachability, placement, import
//! resolution, and dedup are core-owned**: `module` comparison drives both
//! which file a symbol lands in and whether a cross-module reference needs an
//! import (same module => none). Foreign references and same-module exclusion
//! fall out uniformly.

use std::collections::{BTreeMap, BTreeSet};

use crate::emit::{EmittedFile, Import};
use crate::error::Result;
use crate::ir::IR;
use crate::output::GeneratedFiles;
use crate::render::{NameResolver, render_imports};
use crate::traits::Emitter;

/// Drive an emitter to a [`GeneratedFiles`] result.
///
/// Steps: ask the emitter to [`emit`](Emitter::emit) files; for each file
/// resolve its cross-module imports from its `refs` (via the emitter's
/// [`NameResolver`](crate::NameResolver), with same-module exclusion) unioned
/// with the file's `runtime_imports`, deduped; render the import block via
/// [`render_imports`] and stitch it onto the body; collect into
/// [`GeneratedFiles`], choosing the layout from the set of distinct paths.
pub fn assemble<K>(ir: &IR<K>, emitter: &dyn Emitter<K>) -> Result<GeneratedFiles> {
    let language = emitter.language();
    let resolver = emitter.resolver();

    // 1. Emit. The emitter owns layout and produces bodies without import
    //    blocks, declaring per-file `refs` + `runtime_imports`.
    let emitted: Vec<EmittedFile> = emitter.emit(ir)?;

    // 2. For each file, resolve + render its import block and stitch it on.
    //    Resolution is core-owned: walk `refs` through the resolver, drop
    //    same-module refs, union with runtime imports, and dedup.
    let mut files: BTreeMap<std::path::PathBuf, String> = BTreeMap::new();
    for file in emitted {
        let imports = resolve_imports(resolver, &file);
        let import_block = render_imports(language, &imports);
        let stitched = stitch(&import_block, &file.body);
        files.insert(file.path.clone(), stitched);
    }

    // 3. Choose the layout from the distinct output paths the emitter produced:
    //    a single distinct path is a single-file generation, anything else is a
    //    directory tree. This is driven by the emitter's layout, not a guess
    //    about content.
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
/// Driven by `refs` + `module` comparison: each referenced symbol is located
/// via the [`NameResolver`]; a ref placed in the file's *own* module yields no
/// import (same-module exclusion), a ref in a *different* module resolves to
/// the symbol's [`Import`]. Foreign references are just `Type` symbols in a
/// foreign module, so they resolve through the same path. The file's
/// non-symbol `runtime_imports` are unioned in. Dedup is by the structured
/// [`Import`], preserving a stable order.
fn resolve_imports(resolver: &dyn NameResolver, file: &EmittedFile) -> Vec<Import> {
    let mut seen: BTreeSet<Import> = BTreeSet::new();
    let mut imports: Vec<Import> = Vec::new();

    // Cross-module symbol refs.
    for &id in &file.refs {
        if resolver.module_of(id) == file.module {
            continue; // same-module exclusion
        }
        let import = resolver.import_binding(id);
        if seen.insert(import.clone()) {
            imports.push(import);
        }
    }

    // Non-symbol runtime imports (already module-relative; never same-module).
    for import in &file.runtime_imports {
        if import.module == file.module {
            continue;
        }
        if seen.insert(import.clone()) {
            imports.push(import.clone());
        }
    }

    imports
}

/// Stitch a rendered import block in front of a body.
fn stitch(import_block: &str, body: &str) -> String {
    if import_block.is_empty() {
        body.to_string()
    } else {
        format!("{import_block}\n\n{body}")
    }
}
