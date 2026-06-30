//! The base intermediate representation.
//!
//! `Symbol` is the core IR primitive: a **service** and a **type** are both
//! kinds of symbol, so placement, import resolution, reachability, and dedup
//! are one algorithm over [`Symbol::refs`] + the emitter-computed `module`,
//! with no schema-specific knowledge in the base.
//!
//! This is the **IR tier** of the two-tier common contract (`id`, `name`,
//! `kind`, `refs`) — produced by a [`Loader`](crate::Loader). The **emit
//! tier** (`module`, `type_ref`, `import_binding`, `body`) is computed later by
//! an [`Emitter`](crate::Emitter).

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

/// The core IR primitive. Services and types are both symbols.
#[derive(Clone, Debug)]
pub struct Symbol {
    /// Stable identity, unique in the table.
    pub id: SymbolId,
    /// Canonical name (language mapping applied later, at emit time).
    pub name: Name,
    /// What kind of symbol this is, with its base data.
    pub kind: SymbolKind,
    /// Symbols this one references (service I/O, type fields). Drives
    /// reachability, placement, and import resolution in the base.
    pub refs: Vec<SymbolId>,
}

/// The kind of a [`Symbol`], with whatever base data is frontend-independent.
#[derive(Clone, Debug)]
pub enum SymbolKind {
    /// A Nexus service. `ServiceDef` is base data (services are
    /// frontend-independent).
    Service(ServiceDef),
    /// A type. Carries **nothing** schema-specific here; its definition data
    /// lives in the schema_type's PRIVATE side table, keyed by [`Symbol::id`],
    /// so the base IR stays pure (no generics, no erased payloads). A
    /// **foreign** reference (protoc / ts-proto) is a `Type` symbol whose
    /// emit-time `module` is the foreign module and whose `body` is empty.
    Type,
}

/// Base data for a service symbol: operations, wire names, docs.
#[derive(Clone, Debug)]
pub struct ServiceDef {
    /// Canonical service name.
    pub name: Name,
    /// On-the-wire service name (may differ from the canonical name).
    pub wire_name: String,
    /// The service's operations.
    pub operations: Vec<Operation>,
    /// Documentation for the service, if any.
    pub docs: Option<String>,
}

/// A single service operation.
#[derive(Clone, Debug)]
pub struct Operation {
    /// Canonical operation name.
    pub name: Name,
    /// On-the-wire operation name.
    pub wire_name: String,
    /// Input type symbol, if any (`None` for input-less operations).
    pub input: Option<SymbolId>,
    /// Output type symbol, if any (`None` for output-less operations).
    pub output: Option<SymbolId>,
    /// Documentation for the operation, if any.
    pub docs: Option<String>,
}

/// The pure, schema-agnostic symbol table produced by a loader.
///
/// Keyed by [`SymbolId`]. The schema_type's private type-data side table is
/// held by the schema_type crate (correlated by `SymbolId`), **not** here, so
/// the base table stays free of erased payloads.
#[derive(Clone, Debug, Default)]
pub struct SymbolTable {
    symbols: BTreeMap<SymbolId, Symbol>,
    next_id: u32,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocate the next unused [`SymbolId`]. Loaders use this to mint ids
    /// before correlating private type data against them.
    pub fn alloc_id(&mut self) -> SymbolId {
        let id = SymbolId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Insert a symbol, returning its id. The symbol's `id` must already be set
    /// (typically via [`SymbolTable::alloc_id`]).
    pub fn insert(&mut self, symbol: Symbol) -> SymbolId {
        let id = symbol.id;
        self.symbols.insert(id, symbol);
        id
    }

    /// Look up a symbol by id.
    pub fn get(&self, id: SymbolId) -> Option<&Symbol> {
        self.symbols.get(&id)
    }

    /// Iterate over all symbols in id order.
    pub fn iter(&self) -> impl Iterator<Item = &Symbol> {
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

/// What a [`Loader`](crate::Loader) produces.
///
/// Holds the base, pure [`SymbolTable`]. The schema_type's private side table
/// (`SymbolId` -> its type data) is held by the schema_type crate and surfaced
/// only to its own emitters — never to the base — so no `Ir` generic and no
/// erased payload appear here.
pub struct IR {
    pub symbols: SymbolTable,
    // NOTE: the schema_type's private type-data side table (SymbolId -> schema
    // type data) is intentionally NOT a field here. It is held by the
    // schema_type crate and passed to its own emitters; the base never sees it.
}
