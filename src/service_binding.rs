//! Bridge for lowering a frontend's per-language service/operation model into
//! the base [`Service`](nex_gen_codegen::Service) /
//! [`Operation`](nex_gen_codegen::Operation) binding model.
//!
//! Each frontend implements the lowering as `From<Interned<'_, T>>` (see the
//! `render_service_definition` in the `dotnet` / `python` / `typescript`
//! modules), so the conversion reads as `Service::from(Interned { .. })`.

use nex_gen_codegen::ServiceTypeRefs;

/// Pairs a frontend value with the [`ServiceTypeRefs`] interner it feeds while
/// being lowered into the base binding model.
///
/// The base names operation I/O types by
/// [`SymbolId`](nex_gen_codegen::SymbolId); a frontend rendering a service
/// already holds those types as resolved *strings*, so it interns them into a
/// [`ServiceTypeRefs`] (which doubles as the resolver) while building each
/// operation. `From`'s single-argument shape can't carry that running
/// accumulator on its own, so the lowerings take it wrapped here. Defining the
/// wrapper in this crate (rather than passing a bare tuple) also keeps those
/// `From` impls orphan-rule-legal — a tuple would hide the local source type
/// from coherence, leaving the impl with only foreign types.
pub(crate) struct Interned<'a, T> {
    /// The frontend value being lowered, plus any extra context the lowering
    /// needs (e.g. the .NET symbol table).
    pub(crate) value: T,
    /// The interner the lowering pushes each operation's I/O type-ref strings
    /// into, in the order the base assigns their [`SymbolId`]s.
    pub(crate) refs: &'a mut ServiceTypeRefs,
}
