//! The two front-end-facing traits: [`Loader`] and [`Emitter`].
//!
//! A [`Loader`] is per-schema_type: it validates input files and produces the
//! base IR. An [`Emitter`] is per-`(lang, schema_type)`: it renders symbols for
//! one target language, owning file layout. There is no `Ir` generic — the
//! emitter renders against the shared [`SymbolTable`] plus its own private
//! type data, and produces the files itself.

use std::path::PathBuf;

use crate::emit::EmittedFile;
use crate::error::Result;
use crate::ir::{IR, SymbolTable};
use crate::language::Language;
use crate::schema_type::SchemaType;

/// Validates input files for one schema_type and produces the base IR.
///
/// This is where **input validation** lives: JSON Schema strict-subset checks
/// for the JSON loader; WIT parse + proto descriptor resolution for the WIT
/// loader. The loader also builds (and retains, privately) the schema_type's
/// type-data side table keyed by `SymbolId`.
pub trait Loader {
    /// The schema_type this loader handles. Used as the registry key.
    fn schema_type(&self) -> SchemaType;

    /// Validate `inputs` and produce the IR (base [`SymbolTable`] + the
    /// loader's private side table, retained by the loader/front-end crate).
    fn load(&self, inputs: &[PathBuf]) -> Result<IR>;
}

/// Renders symbols for one `(language, schema_type)` pair, owning file layout.
///
/// Type symbols are rendered by the emitter (reading its private data by id);
/// service symbols are rendered by the base
/// [`render_service`](crate::render_service) utility, which the emitter calls.
/// Import blocks are rendered by the base
/// [`render_imports`](crate::render_imports) (not an emitter method) and
/// stitched in during [`assemble`](crate::assemble).
pub trait Emitter {
    /// The target language. Part of the registry key.
    fn language(&self) -> Language;

    /// The schema_type. Part of the registry key.
    fn schema_type(&self) -> SchemaType;

    /// Render the symbols into a set of files (bodies without import blocks).
    fn emit(&self, symbols: &SymbolTable) -> Vec<EmittedFile>;
}
