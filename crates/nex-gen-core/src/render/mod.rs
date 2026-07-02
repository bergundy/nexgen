//! Per-language rendering utilities, reused by every emitter.
//!
//! Rendering is per-language: each submodule ([`python`], [`typescript`],
//! [`dotnet`]) exposes its own `render_service` / `render_imports` plus the
//! minimal `Import` type that language needs, and front-ends call them directly
//! (there is no cross-language dispatcher — the caller already knows its
//! language). Service rendering names each operation's I/O type through a
//! [`NameResolver`] the emitter supplies; type rendering itself stays in the
//! front-end crate.

pub mod dotnet;
pub mod python;
pub mod typescript;

use crate::ir::SymbolId;

/// How a referrer names a referenced symbol in source, supplied by the emitter.
///
/// The core reasons over [`SymbolId`]s, but a front-end that renders a service
/// already holds each operation's I/O type as a resolved source string. It
/// keeps those strings on its own per-service model and passes them here so the
/// per-language `render_service` can name I/O types without inspecting the
/// front-end's private type data.
pub trait NameResolver {
    /// How a referrer names the symbol in source (its emit-tier `type_ref`),
    /// e.g. the local or qualified type name.
    fn type_ref(&self, id: SymbolId) -> String;
}

/// A [`NameResolver`] backed by a service's I/O type-reference names, indexed by
/// the per-service [`SymbolId`] the front-end assigned them: one resolved source
/// string per id it handed out, in id order.
impl NameResolver for Vec<String> {
    fn type_ref(&self, id: SymbolId) -> String {
        self[id.0 as usize].clone()
    }
}

/// The experimental-warning text emitted as the `@experimental` doc tag. Shared
/// by the per-language submodules.
const EXPERIMENTAL_WARNING: &str = "This API is experimental and subject to change.";
