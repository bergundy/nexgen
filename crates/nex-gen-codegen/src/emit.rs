//! Emit-tier types: the placement key, structured imports, and emitted files.
//!
//! These are computed per `(lang, schema_type)` by an
//! [`Emitter`](crate::Emitter). `module` is the placement key that drives both
//! which file a symbol lands in and whether a cross-module reference needs an
//! import; `Import` / `ImportBinding` describe a resolved import; `EmittedFile`
//! is one rendered file (body without its import block — the base renders and
//! stitches the import block via [`render_imports`](crate::render_imports)).
//!
//! Shapes start from the existing crate's `LanguageImportSpec` /
//! `LanguageImportStyle` (grep `src/spec.rs`).

use std::path::PathBuf;

// `Module` is defined here (not in `ir`) because placement is an emit-tier
// concern — it is computed by the emitter, not produced by the loader.

/// A placement key: which logical module/file group a symbol belongs to.
///
/// `module` comparison drives placement (same module => same file) and import
/// resolution (cross-module => import; same module => none). For a first-party
/// symbol this is the target module path; for a **foreign** reference
/// (protoc / ts-proto) it is the foreign module.
// TODO(prototype): Module / Import / ImportBinding shapes — placement key +
// import source + binding (proto namespace-head import vs. first-party named
// import); start from `LanguageImportSpec`. See json-schema/integration-plan.md
// "Open items".
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct Module(pub String);

impl Module {
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// How a cross-module reference is brought into scope.
///
/// Started from `LanguageImportStyle` in the existing crate
/// (`Module` / `Namespace` / `Named`). The proto namespace-head import vs.
/// first-party named import distinction lives here.
// TODO(prototype): Module / Import / ImportBinding shapes — see
// json-schema/integration-plan.md "Open items".
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum ImportBinding {
    /// Import the whole module (e.g. `import foo`).
    Module,
    /// Import the namespace head (e.g. proto `import foo.bar` then `foo.bar.X`).
    Namespace,
    /// Import a specific name (e.g. `from foo import X` / `import { X } from`).
    Named,
}

/// A resolved import for an [`EmittedFile`].
///
/// Modeled on `LanguageImportSpec`: a source `module`, an optional `name`
/// (for named imports), the `binding` style, and whether it is type-only.
// TODO(prototype): Module / Import / ImportBinding shapes — see
// json-schema/integration-plan.md "Open items".
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct Import {
    /// The module being imported from.
    pub module: Module,
    /// The specific name imported, for [`ImportBinding::Named`] /
    /// [`ImportBinding::Namespace`].
    pub name: Option<String>,
    /// How the import is bound into scope.
    pub binding: ImportBinding,
    /// Whether this is a type-only import (e.g. TS `import type`).
    pub type_only: bool,
}

/// One rendered file produced by an emitter.
///
/// The emitter owns file layout (which files, what's in each). The `body`
/// does **not** include the import block; the base renders the import block
/// via [`render_imports`](crate::render_imports) and stitches it in during
/// [`assemble`](crate::assemble).
#[derive(Clone, Debug)]
pub struct EmittedFile {
    /// Output-relative path for this file.
    pub path: PathBuf,
    /// The module this file represents (its placement key).
    pub module: Module,
    /// The file's resolved imports (structured; rendered by the base).
    pub imports: Vec<Import>,
    /// The rendered body, WITHOUT the import block.
    pub body: String,
}
