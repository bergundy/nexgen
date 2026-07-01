//! The base intermediate representation.
//!
//! `Symbol<K>` is the core IR primitive. Its `kind: K` is **frontend-defined
//! and open** — the base is compile-time-agnostic to `K` and only ever touches
//! `id` / `name` / `refs` (plus the emitter's [`NameResolver`](crate::NameResolver)).
//! Records, enums, variants, resources, type references, and services are all
//! variants of the kind type each frontend crate defines and owns.
//!
//! Because `refs` are explicit on every symbol, placement, import resolution,
//! reachability, and dedup are one algorithm over `refs` + the emitter-computed
//! `module`, with no frontend-specific knowledge in the base.
//!
//! Services stay a base concept (they are well-defined across all frontends):
//! the base provides the [`Service`] / [`Operation`] data structs and the
//! [`render_service`](crate::render_service) utility. A frontend kind simply
//! *wraps* a [`Service`], and its emitter calls `render_service` when it renders
//! that symbol — the base never matches on `K`.

use std::collections::BTreeMap;

/// Stable identity for a symbol, unique within a [`SymbolTable`].
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct SymbolId(pub u32);

/// A canonical, language-independent name.
///
/// Language-specific casing/mapping is applied later (at emit time) — this is
/// the name as the loader read it from the source schema.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct Name(pub String);

impl Name {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The core IR primitive, generic over the frontend's open symbol-kind type `K`.
///
/// The base treats `kind` opaquely; it only reasons over `id`, `name`, and
/// `refs`. The frontend's emitter matches on `kind` to render each symbol.
#[derive(Clone, Debug)]
pub struct Symbol<K> {
    /// Stable identity, unique in the table.
    pub id: SymbolId,
    /// Canonical name (language mapping applied later, at emit time).
    pub name: Name,
    /// Symbols this one references (service I/O, type fields). Drives
    /// reachability, placement, and import resolution in the base.
    pub refs: Vec<SymbolId>,
    /// Frontend-defined kind + its data. Opaque to the base.
    pub kind: K,
}

/// Base data for a service symbol: operations, wire names, docs.
///
/// Services are frontend-independent, so this is base data a frontend kind
/// wraps (e.g. a frontend's `Kind::Service(Service)` variant); it never leaks
/// frontend-specific details. Rendered by
/// [`render_service`](crate::render_service).
#[derive(Clone, Debug)]
pub struct Service {
    /// Canonical service name.
    pub name: Name,
    /// On-the-wire service name (may differ from the canonical name).
    pub wire_name: String,
    /// Whether the service is marked experimental. A Nexus annotation,
    /// schema-agnostic.
    pub experimental: bool,
    /// The service's operations.
    pub operations: Vec<Operation>,
    /// Documentation for the service, if any.
    pub docs: Option<String>,
}

/// A single service operation.
///
/// All fields are Nexus operation properties shared across front-ends; the
/// I/O are referenced by [`SymbolId`] so the base never sees the
/// schema-specific type data.
#[derive(Clone, Debug)]
pub struct Operation {
    /// Canonical operation name.
    pub name: Name,
    /// On-the-wire operation name.
    pub wire_name: String,
    /// Whether the operation is marked experimental. A Nexus annotation,
    /// schema-agnostic.
    pub experimental: bool,
    /// Input type symbol, if any (`None` for input-less operations).
    pub input: Option<SymbolId>,
    /// Output type symbol, if any (`None` for output-less operations).
    pub output: Option<SymbolId>,
    /// Documentation for the operation, if any.
    pub docs: Option<String>,
    /// Documentation for the operation's return value, if any. Rendered by
    /// front-ends whose service binding documents returns (e.g. .NET's XML
    /// `<returns>`); front-ends that don't leave it `None`.
    pub returns_doc: Option<String>,
}

/// The symbol table produced by a loader, generic over the frontend kind `K`.
///
/// Keyed by [`SymbolId`]. Everything a symbol needs to render lives *in the
/// symbol* (in `kind`) — there is no private side table.
#[derive(Clone, Debug)]
pub struct SymbolTable<K> {
    symbols: BTreeMap<SymbolId, Symbol<K>>,
    next_id: u32,
}

impl<K> Default for SymbolTable<K> {
    fn default() -> Self {
        Self {
            symbols: BTreeMap::new(),
            next_id: 0,
        }
    }
}

impl<K> SymbolTable<K> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocate the next unused [`SymbolId`]. Loaders use this to mint ids
    /// before inserting (e.g. to record a symbol's `refs` to not-yet-inserted
    /// symbols).
    pub fn alloc_id(&mut self) -> SymbolId {
        let id = SymbolId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Insert a symbol, returning its id. The symbol's `id` must already be set
    /// (typically via [`SymbolTable::alloc_id`]).
    pub fn insert(&mut self, symbol: Symbol<K>) -> SymbolId {
        let id = symbol.id;
        self.symbols.insert(id, symbol);
        id
    }

    /// Look up a symbol by id.
    pub fn get(&self, id: SymbolId) -> Option<&Symbol<K>> {
        self.symbols.get(&id)
    }

    /// Iterate over all symbols in id order.
    pub fn iter(&self) -> impl Iterator<Item = &Symbol<K>> {
        self.symbols.values()
    }

    /// Number of symbols in the table.
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }
}

/// What a [`Loader`](crate::Loader) produces: the base [`SymbolTable`] for the
/// frontend kind `K`. The emitter receives `&IR<K>` and works only from it.
#[derive(Clone, Debug)]
pub struct IR<K> {
    pub symbols: SymbolTable<K>,
}

impl<K> IR<K> {
    pub fn new(symbols: SymbolTable<K>) -> Self {
        Self { symbols }
    }
}

/// A loader's full output: the [`IR`] plus any non-fatal warnings surfaced
/// while lowering the inputs.
///
/// Warnings are frontend diagnostics that are **not** symbols and place no code
/// (e.g. "resource method generated as a stub"). They are derived from the
/// inputs during load, so they travel alongside the IR rather than living in it;
/// the [`Generator`](crate::Generator) copies them onto the resulting
/// [`GeneratedFiles`](crate::GeneratedFiles) after [`assemble`](crate::assemble).
#[derive(Clone, Debug)]
pub struct LoadOutput<K> {
    /// The lowered IR.
    pub ir: IR<K>,
    /// Non-fatal diagnostics surfaced during lowering.
    pub warnings: Vec<String>,
}

impl<K> LoadOutput<K> {
    /// Wrap an IR with no warnings.
    pub fn new(ir: IR<K>) -> Self {
        Self {
            ir,
            warnings: Vec::new(),
        }
    }

    /// Wrap an IR together with the warnings surfaced during lowering.
    pub fn with_warnings(ir: IR<K>, warnings: Vec<String>) -> Self {
        Self { ir, warnings }
    }
}
