//! Base per-language rendering utilities, reused by every emitter.
//!
//! Two entry points, symmetric with each other: [`render_service`] renders a
//! service binding (structural logic + per-language formatting), naming I/O
//! types via a [`NameResolver`] the emitter supplies; [`render_imports`] renders
//! a file's import block. Both dispatch on [`Language`] into the per-language
//! submodules ([`python`], [`typescript`], [`dotnet`]). Only **type** rendering
//! is per-frontend and stays in the front-end crate — service and import
//! rendering are written once per language here. (Import rendering is not yet
//! implemented for [`dotnet`], which inlines its `using` block.)

mod dotnet;
mod python;
mod typescript;

use crate::emit::{Import, Module};
use crate::ir::{Service, SymbolId};
use crate::language::Language;

/// How the base names and locates a referenced symbol, supplied by the emitter.
///
/// The base never inspects the frontend's private type data, so when it
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

/// Interns service-binding I/O type-reference *strings*, minting a sequential
/// [`SymbolId`] per string, and doubles as the [`NameResolver`] for
/// [`render_service`].
///
/// The base reasons over [`SymbolId`]s, but a frontend that renders a service
/// already holds each operation's I/O type as a resolved source string rather
/// than as a symbol-graph node. It [`intern`](Self::intern)s those strings while
/// building the [`Operation`]s — each returned id simply indexes the string —
/// then passes the same value as the resolver. Only [`type_ref`] is ever called
/// during service rendering; `module_of` / `import_binding` are unreachable.
///
/// [`type_ref`]: NameResolver::type_ref
#[derive(Debug, Default)]
pub struct ServiceTypeRefs {
    refs: Vec<String>,
}

impl ServiceTypeRefs {
    /// Intern a type-reference string, returning the [`SymbolId`] that resolves
    /// back to it. Ids are handed out in call order.
    pub fn intern(&mut self, type_ref: impl Into<String>) -> SymbolId {
        let id = SymbolId(self.refs.len() as u32);
        self.refs.push(type_ref.into());
        id
    }
}

impl NameResolver for ServiceTypeRefs {
    fn type_ref(&self, id: SymbolId) -> String {
        self.refs[id.0 as usize].clone()
    }

    fn module_of(&self, _id: SymbolId) -> Module {
        unreachable!("render_service only resolves type_ref, never module_of")
    }

    fn import_binding(&self, _id: SymbolId) -> Import {
        unreachable!("render_service only resolves type_ref, never import_binding")
    }
}

/// The experimental-warning text emitted as the `@experimental` doc tag. Shared
/// by the per-language submodules.
const EXPERIMENTAL_WARNING: &str = "This API is experimental and subject to change.";

/// Render a single service binding for `lang`.
///
/// Structural logic (operations, wire names, docs) is language-agnostic;
/// per-language formatting lives in the language submodule. I/O type names come
/// from `names`. Type rendering, foreign-type conversion, and resources stay in
/// the front-end crate; only the service/operation binding is rendered here.
pub fn render_service(lang: Language, svc: &Service, names: &dyn NameResolver) -> String {
    match lang {
        Language::TypeScript => typescript::render_service(svc, names),
        Language::Python => python::render_service(svc, names),
        Language::Dotnet => dotnet::render_service(svc, names),
        _ => todo!("service rendering for {lang} is not implemented yet"),
    }
}

/// Render the import block for a file, given its resolved [`Import`]s.
///
/// Symmetric with [`render_service`]: structural over `imports`, formatted per
/// `lang` in the language submodule. This is a base utility, **not** an emitter
/// method — the emitter produces structured imports on its
/// [`EmittedFile`](crate::EmittedFile)s and the base renders + stitches the
/// block in [`assemble`](crate::assemble).
///
/// Output is canonical (sorted, grouped); the per-language formatter
/// (`ruff` / `prettier`) reflows it afterwards. [`ImportBinding::Named`]
/// imports to the same module are merged into one statement.
///
/// [`ImportBinding::Named`]: crate::emit::ImportBinding::Named
pub fn render_imports(lang: Language, imports: &[Import]) -> String {
    // No imports to render is an empty block in every language — short-circuit
    // before the per-language rendering so emitters that inline their own
    // imports (and declare no refs) never require language-specific support.
    if imports.is_empty() {
        return String::new();
    }
    match lang {
        Language::Python => python::render_imports(imports),
        Language::TypeScript => typescript::render_imports(imports),
        _ => todo!("import rendering for {lang} is not implemented yet"),
    }
}
