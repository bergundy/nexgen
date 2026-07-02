//! The two front-end-facing traits: [`Loader`] and [`Emitter`].
//!
//! A [`Loader`] is per-frontend: it validates input files and lowers them into
//! the IR. An [`Emitter`] is per-language: it renders symbols for one target
//! language, owning file layout. Both are generic over the frontend's open kind
//! type `K`; the emitter renders straight from the loader's `IR<K>` (no private
//! side table) and produces the files itself.

use crate::emit::EmittedFile;
use crate::error::Result;
use crate::ir::{LoadOutput, IR};
use crate::language::Language;
use crate::render::NameResolver;

/// Validates input files for one frontend and produces the core IR.
///
/// This is where **input validation** lives: each frontend loader parses and
/// validates its own input format before lowering. The loader lowers its inputs
/// directly into a [`SymbolTable`] (`IR<Self::Kind>`) whose symbols carry all
/// the data their emitter needs — there is no private side table.
///
/// The loader instance is **constructed by the frontend**, holding its own
/// inputs/config (input paths, descriptor paths, …). `language` is supplied per
/// call — one loader backs every language emitter for its frontend, and some
/// frontends resolve types per-language at parse time.
///
/// [`SymbolTable`]: crate::SymbolTable
pub trait Loader {
    /// The frontend's open symbol-kind type. The generator is fixed to this
    /// kind: emitters are paired with a loader that shares it.
    type Kind;

    /// Validate the loader's inputs and lower them into the IR for `language`,
    /// together with any non-fatal warnings surfaced during lowering.
    fn load(&self, language: Language) -> Result<LoadOutput<Self::Kind>>;
}

/// Renders symbols for one language, owning file layout.
///
/// Type symbols are rendered by the emitter (reading its private data by id);
/// service symbols are rendered by the core
/// [`render_service`](crate::render_service) utility, which the emitter calls.
/// Import blocks are rendered by the core
/// [`render_imports`](crate::render_imports) (not an emitter method) and
/// stitched in during [`assemble`](crate::assemble).
pub trait Emitter<K> {
    /// The target language. The generator keys its emitters by this.
    fn language(&self) -> Language;

    /// Render the IR into a set of files (bodies without import blocks).
    ///
    /// Each [`EmittedFile`] declares the symbols it references (`refs`) and any
    /// non-symbol runtime imports; the core resolves them into the import block
    /// during [`assemble`](crate::assemble) using [`resolver`](Emitter::resolver).
    ///
    /// Fallible: rendering may resolve types that fail validation (e.g. an
    /// unresolvable I/O type), so the error surfaces here rather than panicking.
    fn emit(&self, ir: &IR<K>) -> Result<Vec<EmittedFile>>;

    /// How the core names and locates referenced symbols for this emitter.
    ///
    /// Used by [`assemble`](crate::assemble) to resolve each
    /// [`EmittedFile`](crate::EmittedFile)'s `refs` into cross-module imports,
    /// and by [`render_service`](crate::render_service) to name operation I/O
    /// types. Resolution stays out of the emitter's `emit` body: the emitter
    /// only declares `refs`, the core resolves them through this resolver.
    fn resolver(&self) -> &dyn NameResolver;
}
