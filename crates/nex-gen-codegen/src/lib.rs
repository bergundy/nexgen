//! `nex-gen-codegen` — the front-end-agnostic codegen base layer.
//!
//! This crate is the base described in `json-schema/integration-plan.md`: the
//! `Symbol`-centric IR, the [`Loader`] / [`Emitter`] traits, the per-language
//! [`render_service`] / [`render_imports`] utilities, the [`assemble`]
//! pipeline (placement + import resolution), the [`Generator`], and the output
//! plumbing ([`GeneratedFiles`], [`write_generated_files`],
//! [`format_generated_file`]).
//!
//! The pipeline is **loader -> IR -> emitter**:
//!
//! ```text
//! inputs --Loader--> IR<K> (SymbolTable<K>; K = frontend kind)
//!        --Emitter[lang]--> Vec<EmittedFile>
//!        --assemble: group by module -> resolve imports -> render -> write/format
//!        --> GeneratedFiles
//! ```
//!
//! It is **frontend-agnostic**: symbol kinds are frontend-defined (the
//! generic `K`), and the base reasons only over `id` / `name` / `refs` plus
//! emit-tier data (`module`, `type_ref`, `import_binding`, `body`) — it never
//! inspects `K`. Services stay a base concept ([`Service`] + [`render_service`]),
//! which a frontend kind wraps.
//!
//! The import-resolution + assembly path is implemented end-to-end:
//! [`assemble`] resolves each [`EmittedFile`]'s `refs` to cross-module imports
//! (dropping same-module refs, unioning runtime imports, deduping) and
//! [`render_imports`] renders them for Python and TypeScript.

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
pub use emit::{EmittedFile, Import, ImportBinding, Module};
pub use error::{Error, Result};
pub use generator::Generator;
pub use ir::{IR, LoadOutput, Name, Operation, Service, Symbol, SymbolId, SymbolTable};
pub use language::Language;
pub use output::{
    GeneratedFiles, GeneratedOutputLayout, format_generated_file, formatter_command,
    write_generated_files,
};
pub use render::{NameResolver, render_imports, render_service};
pub use traits::{Emitter, Loader};
