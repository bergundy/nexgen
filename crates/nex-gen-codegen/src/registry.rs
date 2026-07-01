//! The pipeline registry.
//!
//! A loader is registered together with its emitters — one loader per
//! schema_type, shared by every language emitter for that schema_type. The
//! keys are **inferred** from [`Emitter::language`] + [`Emitter::schema_type`],
//! not passed. Because each emitter renders straight from the loader's `IR<K>`
//! (no private side table, no construction-over-private-data factory), the
//! registry pairs the loader with each emitter behind a **type-erased runner**
//! closure — the frontend kind `K` never escapes it.

use std::collections::HashMap;
use std::rc::Rc;

use crate::assemble::assemble;
use crate::error::Result;
use crate::language::Language;
use crate::output::GeneratedFiles;
use crate::schema_type::SchemaType;
use crate::traits::{Emitter, Loader};

/// A type-erased pipeline: `load(language) -> assemble -> GeneratedFiles`. The
/// loader holds its own inputs (frontend-constructed), so the runner takes no
/// arguments.
type Runner = Box<dyn Fn() -> Result<GeneratedFiles>>;

/// A registry of `(language, schema_type)` pipelines.
#[derive(Default)]
pub struct Registry {
    runners: HashMap<(Language, SchemaType), Runner>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a `loader` together with its `emitters` (one per language for
    /// that schema_type). Each `(loader, emitter)` becomes a runner keyed by
    /// [`Emitter::language`] + [`Emitter::schema_type`] — no key argument. The
    /// loader is shared across the per-language runners; the loader's `Kind` and
    /// every emitter's `K` must match.
    ///
    /// `emitters` is any iterable of boxed emitters, e.g. an array literal:
    /// `registry.register(WitLoader::new(), [Box::new(py), Box::new(ts)])`.
    pub fn register<K, L>(
        &mut self,
        loader: L,
        emitters: impl IntoIterator<Item = Box<dyn Emitter<K>>>,
    ) where
        K: 'static,
        L: Loader<Kind = K> + 'static,
    {
        let loader = Rc::new(loader);
        for emitter in emitters {
            let loader = Rc::clone(&loader);
            let language = emitter.language();
            let key = (language, emitter.schema_type());
            let runner: Runner = Box::new(move || {
                let loaded = loader.load(language)?;
                let mut generated = assemble(&loaded.ir, emitter.as_ref())?;
                generated.warnings = loaded.warnings;
                Ok(generated)
            });
            self.runners.insert(key, runner);
        }
    }

    /// Run the pipeline registered for `(language, schema_type)`, or `None` if
    /// no pipeline is registered for that pair.
    pub fn generate(
        &self,
        language: Language,
        schema_type: SchemaType,
    ) -> Option<Result<GeneratedFiles>> {
        self.runners
            .get(&(language, schema_type))
            .map(|runner| runner())
    }
}
