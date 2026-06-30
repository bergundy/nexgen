//! Input formats / front-ends.
//!
//! A `schema_type` is an input format with a dedicated [`Loader`](crate::Loader)
//! that validates inputs and produces the base IR, plus per-language
//! [`Emitter`](crate::Emitter)s. The base reasons over symbols uniformly and
//! never inspects the schema_type's private type data.

use std::fmt;

/// An input format / front-end the base can drive.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum SchemaType {
    /// WIT + proto descriptors (the existing generator's front-end).
    Wit,
    /// JSON Schema (the front-end tracked by `PLAN.md`).
    JsonSchema,
}

impl SchemaType {
    /// Stable, lowercase identifier used in CLI flags and registry keys.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Wit => "wit",
            Self::JsonSchema => "json-schema",
        }
    }
}

impl fmt::Display for SchemaType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
