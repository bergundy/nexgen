//! The codegen generator.
//!
//! A [`Generator`] pairs one frontend loader with its per-language emitters —
//! one loader, shared by every language emitter for the loader's symbol kind.
//! The loader and emitters are supplied to [`Generator::new`]; each emitter is
//! keyed by its [`Emitter::language`]. Because each emitter renders straight
//! from the loader's `IR<K>` (no private side table), [`Generator::generate`]
//! runs the whole `load(language) -> assemble -> GeneratedFiles` pipeline
//! directly, with no type erasure.

use std::collections::HashMap;

use crate::assemble::assemble;
use crate::error::Result;
use crate::language::Language;
use crate::output::GeneratedFiles;
use crate::traits::{Emitter, Loader};

/// Pairs a frontend [`Loader`] with its per-language [`Emitter`]s and runs the
/// `load -> assemble` pipeline. The loader's [`Kind`](Loader::Kind) fixes the
/// symbol kind `K`; every emitter must share it. The loader is shared across
/// languages — [`generate`](Generator::generate) picks the emitter for the
/// requested language and loads once for it.
pub struct Generator<L: Loader> {
    loader: L,
    emitters: HashMap<Language, Box<dyn Emitter<L::Kind>>>,
}

impl<L: Loader> Generator<L> {
    /// Build a generator from a `loader` and its `emitters` (one per language).
    /// Each emitter is keyed by [`Emitter::language`]; the loader's `Kind` and
    /// every emitter's `K` must match.
    ///
    /// `emitters` is any iterable of boxed emitters, e.g. an array literal:
    /// `Generator::new(WitLoader::new(), [Box::new(py), Box::new(ts)])`.
    pub fn new(
        loader: L,
        emitters: impl IntoIterator<Item = Box<dyn Emitter<L::Kind>>>,
    ) -> Self {
        let emitters = emitters
            .into_iter()
            .map(|emitter| (emitter.language(), emitter))
            .collect();
        Self { loader, emitters }
    }

    /// Run the pipeline for `language`, or `None` if no emitter is registered
    /// for it: load the IR (with warnings), assemble the emitter's files, and
    /// carry the loader's warnings onto the result.
    pub fn generate(&self, language: Language) -> Option<Result<GeneratedFiles>> {
        let emitter = self.emitters.get(&language)?;
        Some((|| {
            let loaded = self.loader.load(language)?;
            let mut generated = assemble(&loaded.ir, emitter.as_ref())?;
            generated.warnings = loaded.warnings;
            Ok(generated)
        })())
    }
}
