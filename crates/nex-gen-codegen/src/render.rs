//! Base per-language rendering utilities, reused by every emitter.
//!
//! Two functions, symmetric with each other:
//! [`render_service`] renders a service binding (structural logic + per-
//! language formatting), naming I/O types via a [`NameResolver`] the emitter
//! supplies; [`render_imports`] renders a file's import block. Only **type**
//! rendering is per-schema_type and stays in the front-end crate — service and
//! import rendering are written once per language here.

use crate::emit::{Import, ImportBinding, Module};
use crate::ir::{ServiceDef, SymbolId};
use crate::language::Language;

/// How the base names and locates a referenced symbol, supplied by the emitter.
///
/// The base never inspects the schema_type's private type data, so when it
/// renders a service it asks the emitter (via this resolver) how to name and
/// import each operation's I/O type by [`SymbolId`].
pub trait NameResolver {
    /// How a referrer names the symbol in source (its emit-tier `type_ref`),
    /// e.g. the local or qualified type name.
    fn type_ref(&self, id: SymbolId) -> String;

    /// The module the symbol is placed in (its emit-tier `module`). Used to
    /// decide same-module vs. cross-module.
    fn module_of(&self, id: SymbolId) -> Module;

    /// How to import the symbol cross-module (its emit-tier `import_binding`).
    fn import_binding(&self, id: SymbolId) -> Import;
}

/// Render a single service binding for `lang`.
///
/// Structural logic (operations, wire names, docs) is language-agnostic;
/// per-language formatting branches on `lang`. I/O type names come from
/// `names`. Phase 2 pulls the real bodies out of `python::generate` /
/// `typescript::generate`.
pub fn render_service(lang: Language, _svc: &ServiceDef, _names: &dyn NameResolver) -> String {
    match lang {
        // TODO(prototype): import resolution flow — who calls the base
        // resolution/render helpers (emitter while building EmittedFiles vs.
        // base post-processing). See json-schema/integration-plan.md "Open
        // items". Real bodies land in Phase 2 (extract service/operation
        // rendering utilities).
        Language::Python | Language::TypeScript => todo!(
            "Phase 2: extract Nexus service-binding rendering for {lang} \
             out of `{lang}::generate` into this base utility"
        ),
        _ => todo!("service rendering for {lang} is not implemented yet"),
    }
}

/// Render the import block for a file, given its resolved [`Import`]s.
///
/// Symmetric with [`render_service`]: structural over `imports`, formatted per
/// `lang`. This is a base utility, **not** an emitter method — the emitter
/// produces structured imports on its [`EmittedFile`](crate::EmittedFile)s and
/// the base renders + stitches the block in [`assemble`](crate::assemble).
pub fn render_imports(lang: Language, _imports: &[Import]) -> String {
    match lang {
        // TODO(prototype): Module / Import / ImportBinding shapes drive exactly
        // how each ImportBinding (Module/Namespace/Named) renders per language.
        // See json-schema/integration-plan.md "Open items".
        Language::Python | Language::TypeScript => todo!(
            "Phase 2: render import block for {lang} from structured imports"
        ),
        _ => todo!("import rendering for {lang} is not implemented yet"),
    }
}

/// Render a single import line for `lang` (helper used by [`render_imports`]).
///
/// Left as a stub: the per-binding formatting is settled alongside the
/// `Module` / `Import` / `ImportBinding` shapes in the prototype.
#[allow(dead_code)] // wired up by render_imports in Phase 2.
fn render_import_line(_lang: Language, _import: &Import) -> String {
    // TODO(prototype): per-language formatting for each ImportBinding variant.
    let _ = ImportBinding::Named; // referenced so the variant stays exercised.
    todo!("render a single import line once Import/ImportBinding shapes settle")
}
