//! The loader + emitter registry.
//!
//! Keys are **inferred**, not passed: a [`Loader`] reports its
//! [`schema_type()`](Loader::schema_type), and an [`Emitter`] reports its
//! [`language()`](Emitter::language) + [`schema_type()`](Emitter::schema_type),
//! so registration takes only the value. The library/CLI entry resolves
//! `(lang, schema_type)` -> loader + emitter, loads inputs, constructs the
//! emitter over the loaded private data, and calls
//! [`assemble`](crate::assemble).

use std::collections::HashMap;

use crate::language::Language;
use crate::schema_type::SchemaType;
use crate::traits::{Emitter, Loader};

/// A registry of loaders (keyed by schema_type) and emitters (keyed by
/// `(language, schema_type)`).
#[derive(Default)]
pub struct Registry {
    loaders: HashMap<SchemaType, Box<dyn Loader>>,
    emitters: HashMap<(Language, SchemaType), Box<dyn Emitter>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a loader. The key is inferred from
    /// [`Loader::schema_type`] — no key argument.
    pub fn register_loader(&mut self, loader: impl Loader + 'static) {
        let key = loader.schema_type();
        self.loaders.insert(key, Box::new(loader));
    }

    /// Register an emitter. The key is inferred from
    /// [`Emitter::language`] + [`Emitter::schema_type`] — no key argument.
    //
    // TODO(prototype): emitter construction over private data — in the real
    // pipeline the registry entry for a (lang, schema_type) pair builds the
    // emitter from the loader's private side table, so this likely stores an
    // emitter *factory* (a fn over that private data) rather than a constructed
    // emitter. Settle the exact signature in the prototype. See
    // json-schema/integration-plan.md "Open items".
    pub fn register_emitter(&mut self, emitter: impl Emitter + 'static) {
        let key = (emitter.language(), emitter.schema_type());
        self.emitters.insert(key, Box::new(emitter));
    }

    /// Look up the loader for a schema_type.
    pub fn loader(&self, schema_type: SchemaType) -> Option<&dyn Loader> {
        self.loaders.get(&schema_type).map(Box::as_ref)
    }

    /// Look up the emitter for a `(language, schema_type)` pair.
    pub fn emitter(&self, language: Language, schema_type: SchemaType) -> Option<&dyn Emitter> {
        self.emitters
            .get(&(language, schema_type))
            .map(Box::as_ref)
    }
}
