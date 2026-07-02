//! `nex-gen-core` — the front-end-agnostic codegen core layer.
//!
//! This crate is the frontend-agnostic core: the
//! `Symbol`-centric IR, the [`Loader`] / [`Emitter`] traits, the per-language
//! [`render`] utilities (each language's `render_service` / `render_imports` +
//! its own `Import` type), the [`assemble`] pipeline (collect + layout), the
//! [`Generator`], and the output plumbing ([`GeneratedFiles`],
//! [`write_generated_files`], [`format_generated_file`]).
//!
//! The pipeline is **loader -> IR -> emitter**:
//!
//! ```text
//! inputs --Loader--> IR<K> (SymbolTable<K>; K = frontend kind)
//!        --Emitter[lang]--> Vec<EmittedFile>  (bodies rendered in full)
//!        --assemble: collect by path -> choose layout -> write/format
//!        --> GeneratedFiles
//! ```
//!
//! It is **frontend-agnostic**: symbol kinds are frontend-defined (the
//! generic `K`), and the core reasons only over `id` / `name` plus emit-tier
//! data (`module`, `type_ref`, `body`) — it never inspects `K`. Services stay a
//! core concept ([`Service`] + [`render`]), which a frontend kind wraps.
//!
//! Rendering is per-language: each emitter renders its own files' bodies —
//! including import blocks — by calling the [`render`] submodule for its
//! language directly (each owns its minimal `Import` type; only TypeScript has
//! type-only imports). [`assemble`] then just collects the files and picks the
//! layout.

pub mod assemble;
pub mod emit;
pub mod error;
pub mod generator;
pub mod ir;
pub mod language;
pub mod output;
pub mod render;
pub mod traits;

pub use assemble::assemble;
pub use emit::EmittedFile;
pub use error::{Error, Result};
pub use generator::Generator;
pub use ir::{IR, LoadOutput, Name, Operation, Service, Symbol, SymbolId, SymbolTable};
pub use language::Language;
pub use output::{
    GeneratedFiles, GeneratedOutputLayout, format_generated_file, formatter_command,
    write_generated_files,
};
pub use render::NameResolver;
pub use traits::{Emitter, Loader};
