//! The two front-end-facing traits: [`Loader`] and [`Emitter`].
//!
//! A [`Loader`] is per-frontend: it validates input files and lowers them into
//! the IR. An [`Emitter`] is per-language: it renders symbols for one target
//! language, owning file layout. Both are generic over the frontend's open kind
//! type `K`; the emitter renders straight from the loader's `IR<K>` (no private
//! side table) and produces the files itself.

use crate::emit::EmittedFile;
use crate::error::Result;
use crate::ir::{IR, LoadOutput};
use crate::language::Language;

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
/// service symbols and import blocks are rendered via the per-language
/// [`render`](crate::render) helpers, which the emitter calls directly. Each
/// [`EmittedFile`] carries a complete body (import block included);
/// [`assemble`](crate::assemble) only collects and lays them out.
pub trait Emitter<K> {
    /// The target language. The generator keys its emitters by this.
    fn language(&self) -> Language;

    /// Render the IR into a set of files, each with its body rendered in full
    /// (import block included).
    ///
    /// Fallible: rendering may resolve types that fail validation (e.g. an
    /// unresolvable I/O type), so the error surfaces here rather than panicking.
    fn emit(&self, ir: &IR<K>) -> Result<Vec<EmittedFile>>;
}
