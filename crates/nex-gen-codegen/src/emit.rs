//! Emit-tier types: the placement key, structured imports, and emitted files.
//!
//! These are computed per language by an
//! [`Emitter`](crate::Emitter). `module` is the placement key that drives both
//! which file a symbol lands in and whether a cross-module reference needs an
//! import; `Import` / `ImportBinding` describe a resolved import; `EmittedFile`
//! is one rendered file (body without its import block — the base renders and
//! stitches the import block via [`render_imports`](crate::render_imports)).
//!
//! Shapes start from the existing crate's `LanguageImportSpec` /
//! `LanguageImportStyle` (grep `src/spec.rs`).

use std::path::PathBuf;

use crate::ir::SymbolId;

// `Module` is defined here (not in `ir`) because placement is an emit-tier
// concern — it is computed by the emitter, not produced by the loader.

/// A placement key: which logical module/file group a symbol belongs to.
///
/// `module` comparison drives placement (same module => same file) and import
/// resolution (cross-module => import; same module => none). For a first-party
/// symbol this is the target module path; for a **foreign** reference
/// (protoc / ts-proto) it is the foreign module.
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
/// Grounded in the real generated examples (`examples/{python,typescript}`):
///
/// - [`Module`](ImportBinding::Module) — whole-module import. Python
///   `import temporalio.common` (no alias) or
///   `import temporalio.api.workflowservice.v1.request_response_pb2` (proto);
///   TypeScript `import * as nexus from "nexus-rpc"` (the alias is the
///   [`Import::name`]). Referrers qualify uses through the module/alias path.
/// - [`Namespace`](ImportBinding::Namespace) — import a namespace *head*,
///   referrers qualify through it. TypeScript `import * as workflow from
///   "@temporalio/workflow"` then `workflow.X`.
/// - [`Named`](ImportBinding::Named) — import specific names, grouped per
///   module. Python `from .models import (X, Y)` / TypeScript
///   `import type { X, Y } from "./models.ts"`. The proto namespace-head
///   import (`import type { temporal } from "@temporalio/proto"`, with
///   referrers writing `temporal.api...IFoo`) is also a `Named` import whose
///   [`Import::name`] is the namespace head `temporal`.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum ImportBinding {
    /// Import the whole module. [`Import::name`], if set, is the bound alias
    /// (TS `import * as <name>`); when `None` the module path itself is the
    /// access path (Python `import a.b.c`).
    Module,
    /// Import a namespace head that referrers qualify through. [`Import::name`]
    /// is the namespace alias.
    Namespace,
    /// Import a specific name, grouped per module
    /// (`from m import X` / `import { X } from "m"`). [`Import::name`] is the
    /// imported symbol/namespace-head name.
    Named,
}

/// A resolved import for an [`EmittedFile`].
///
/// A source `module`, an optional `name` (set for [`ImportBinding::Named`] /
/// [`ImportBinding::Namespace`], and as the alias for an aliased
/// [`ImportBinding::Module`]), the `binding` style, and whether it is
/// type-only (TS `import type`). `Named` imports to the same `module` are
/// merged into one statement by [`render_imports`](crate::render_imports).
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct Import {
    /// The module being imported from.
    pub module: Module,
    /// The specific name imported, for [`ImportBinding::Named`] /
    /// [`ImportBinding::Namespace`] (and the alias for an aliased
    /// [`ImportBinding::Module`]).
    pub name: Option<String>,
    /// How the import is bound into scope.
    pub binding: ImportBinding,
    /// Whether this is a type-only import (e.g. TS `import type`).
    pub type_only: bool,
}

/// One rendered file produced by an emitter.
///
/// The emitter owns file layout (which files, what's in each). The `body`
/// does **not** include the import block; the base resolves cross-module
/// imports from `refs`, renders the block via
/// [`render_imports`](crate::render_imports), and stitches it in during
/// [`assemble`](crate::assemble).
///
/// Import resolution is split so it stays out of the emitter: the emitter
/// declares which symbols the file references (`refs`) and any non-symbol
/// runtime imports (`runtime_imports`); the base walks `refs` through the
/// emitter's [`NameResolver`](crate::NameResolver), drops same-module refs,
/// resolves the rest to [`Import`]s, unions in the runtime imports, and dedups.
#[derive(Clone, Debug)]
pub struct EmittedFile {
    /// Output-relative path for this file.
    pub path: PathBuf,
    /// The module this file represents (its placement key).
    pub module: Module,
    /// Symbols this file references. The base resolves these to cross-module
    /// [`Import`]s (same-module refs are dropped) via the emitter's
    /// [`NameResolver`](crate::NameResolver).
    pub refs: Vec<SymbolId>,
    /// Non-symbol runtime imports the file needs regardless of `refs` (e.g.
    /// `nexus-rpc` / `nexusrpc`, `dataclasses`). Unioned in and deduped with
    /// the resolved cross-module imports by [`assemble`](crate::assemble).
    pub runtime_imports: Vec<Import>,
    /// The rendered body, WITHOUT the import block.
    pub body: String,
}
