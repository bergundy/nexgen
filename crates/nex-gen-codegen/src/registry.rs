//! The pipeline registry.
//!
//! A registry is tied to one frontend symbol kind `K`: a loader is registered
//! together with its emitters — one loader, shared by every language emitter
//! for that kind. The key is **inferred** from [`Emitter::language`], not
//! passed. Because each emitter renders straight from the loader's `IR<K>` (no
//! private side table, no construction-over-private-data factory), the registry
//! pairs the loader with each emitter behind a **type-erased runner** closure.

use std::collections::HashMap;
use std::marker::PhantomData;
use std::rc::Rc;

use crate::assemble::assemble;
use crate::error::Result;
use crate::language::Language;
use crate::output::GeneratedFiles;
use crate::traits::{Emitter, Loader};

/// A type-erased pipeline: `load(language) -> assemble -> GeneratedFiles`. The
/// loader holds its own inputs (frontend-constructed), so the runner takes no
/// arguments.
type Runner = Box<dyn Fn() -> Result<GeneratedFiles>>;

/// A registry of per-language pipelines for one frontend symbol kind `K`. The
/// frontend chooses the registry by the loader's [`Kind`](Loader::Kind); there
/// is no runtime schema-type key.
pub struct Registry<K> {
    runners: HashMap<Language, Runner>,
    _kind: PhantomData<K>,
}

impl<K> Default for Registry<K> {
    fn default() -> Self {
        Self {
            runners: HashMap::new(),
            _kind: PhantomData,
        }
    }
}

impl<K> Registry<K> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a `loader` together with its `emitters` (one per language). Each
    /// `(loader, emitter)` becomes a runner keyed by [`Emitter::language`] — no
    /// key argument. The loader is shared across the per-language runners; the
    /// loader's `Kind` and every emitter's `K` must match.
    ///
    /// `emitters` is any iterable of boxed emitters, e.g. an array literal:
    /// `registry.register(WitLoader::new(), [Box::new(py), Box::new(ts)])`.
    pub fn register<L>(
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
            let runner: Runner = Box::new(move || {
                let loaded = loader.load(language)?;
                let mut generated = assemble(&loaded.ir, emitter.as_ref())?;
                generated.warnings = loaded.warnings;
                Ok(generated)
            });
            self.runners.insert(language, runner);
        }
    }

    /// Run the pipeline registered for `language`, or `None` if no pipeline is
    /// registered for it.
    pub fn generate(&self, language: Language) -> Option<Result<GeneratedFiles>> {
        self.runners.get(&language).map(|runner| runner())
    }
}
