//! Helpers shared by the front-ends when lowering their per-language service
//! models into the core [`Service`](nex_gen_core::Service) /
//! [`Operation`](nex_gen_core::Operation) binding model.
//!
//! The core names operation I/O types by [`SymbolId`](nex_gen_core::SymbolId)
//! and resolves them through a [`NameResolver`](nex_gen_core::NameResolver);
//! a front-end already holds those types as resolved *strings*, so it keeps a
//! per-service table of them (a `Vec<String>` — which is itself the resolver,
//! see the core `impl NameResolver for Vec<String>`) and assigns each operation
//! the [`SymbolId`](nex_gen_core::SymbolId) that indexes its entry.

use nex_gen_core::SymbolId;

/// Append a resolved I/O type-reference string to a service's `refs` table and
/// return the per-service [`SymbolId`] that indexes it. Ids are handed out in
/// call order, so the caller interns each operation's input then output.
pub(crate) fn push_io_ref(refs: &mut Vec<String>, type_ref: String) -> SymbolId {
    let id = SymbolId(refs.len() as u32);
    refs.push(type_ref);
    id
}
