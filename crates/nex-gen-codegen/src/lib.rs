//! `nex-gen-codegen` — the front-end-agnostic codegen base layer.
//!
//! This crate is the base described in `json-schema/integration-plan.md`: the
//! `Symbol`-centric IR, the [`Loader`] / [`Emitter`] traits, the per-language
//! [`render_service`] / [`render_imports`] utilities, the [`assemble`]
//! pipeline (placement + import resolution), the [`Registry`], and the output
//! plumbing ([`GeneratedFiles`], [`write_generated_files`],
//! [`format_generated_file`]).
//!
//! The pipeline is **loader -> IR -> emitter**:
//!
//! ```text
//! inputs --Loader[schema_type]--> IR (SymbolTable + private side table)
//!        --Emitter[(lang, schema_type)]--> Vec<EmittedFile>
//!        --assemble: group by module -> resolve imports -> render -> write/format
//!        --> GeneratedFiles
//! ```
//!
//! It is **schema_type-agnostic**: the base reasons over symbols (`id`, `name`,
//! `kind`, `refs`) and emit-tier data (`module`, `type_ref`, `import_binding`,
//! `body`) uniformly, and never inspects a schema_type's private type data.
//!
//! The import-resolution + assembly path is implemented end-to-end: the
//! [`Module`] / [`Import`] / [`ImportBinding`] shapes are concrete,
//! [`assemble`] resolves each [`EmittedFile`]'s `refs` to cross-module imports
//! (dropping same-module refs, unioning runtime imports, deduping) and
//! [`render_imports`] renders them for Python and TypeScript. Bodies still
//! deferred to later phases are marked `// TODO(prototype): ...` (the
//! per-language [`render_service`], and the registry emitter-factory).

pub mod assemble;
pub mod emit;
pub mod error;
pub mod ir;
pub mod language;
pub mod output;
pub mod registry;
pub mod render;
pub mod schema_type;
pub mod traits;

pub use assemble::assemble;
pub use emit::{EmittedFile, Import, ImportBinding, Module};
pub use error::{Error, Result};
pub use ir::{IR, Name, Operation, ServiceDef, Symbol, SymbolId, SymbolKind, SymbolTable};
pub use language::Language;
pub use output::{
    GeneratedFiles, GeneratedOutputLayout, format_generated_file, formatter_command,
    write_generated_files,
};
pub use registry::Registry;
pub use render::{NameResolver, render_imports, render_service};
pub use schema_type::SchemaType;
pub use traits::{Emitter, Loader};
