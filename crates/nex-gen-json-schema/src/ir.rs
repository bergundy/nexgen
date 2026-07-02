//! The front-end symbol kind and the lowered type model.
//!
//! [`Kind`] is the open symbol-kind that parameterizes [`nex_gen_core`]'s
//! `Symbol<K>`. Every named schema type becomes a [`Record`] or a [`MapType`]
//! symbol; every Nexus service becomes a [`Service`](nex_gen_core::Service)
//! symbol. The core reasons only over each symbol's `id`, `name`, and `refs`;
//! the emitters match on `Kind` to render.

use nex_gen_core::{Service, SymbolId};

/// The front-end symbol kind.
#[derive(Clone, Debug)]
pub enum Kind {
    /// An object type with declared members (`properties`), open or closed.
    Record(Record),
    /// A typed-map wrapper (`additionalProperties: <schema>` with no
    /// `properties`) — a named wrapper around `map<string, T>`.
    Map(MapType),
    /// A Nexus service binding.
    Service(Service),
}

/// An object type: declared members plus an open/closed/typed catch-all and the
/// object-level count / dependency assertions.
#[derive(Clone, Debug)]
pub struct Record {
    pub docs: Option<String>,
    pub fields: Vec<Field>,
    pub additional: Additional,
    pub min_properties: Option<u64>,
    pub max_properties: Option<u64>,
    /// `dependentRequired`: (trigger member, dependent members).
    pub dependent_required: Vec<(String, Vec<String>)>,
    /// True when synthesized from an inline operation I/O object.
    pub synthesized: bool,
}

/// The catch-all policy for an object's undeclared members.
#[derive(Clone, Debug, PartialEq)]
pub enum Additional {
    /// `additionalProperties: false` — unknown keys are a violation.
    Closed,
    /// Default / `additionalProperties: true` — unknown keys preserved verbatim.
    Open,
    /// `additionalProperties: <schema>` — unknown keys are typed values.
    Typed(FieldType),
}

/// A typed-map wrapper type (`Labels`-style).
#[derive(Clone, Debug)]
pub struct MapType {
    pub docs: Option<String>,
    /// The value type of the map (`additionalProperties` schema).
    pub value: FieldType,
    pub min_properties: Option<u64>,
    pub max_properties: Option<u64>,
}

/// One declared object member.
#[derive(Clone, Debug)]
pub struct Field {
    /// The member key as it appears on the wire (the JSON name).
    pub json_name: String,
    pub docs: Option<String>,
    pub ty: FieldType,
    /// Present in the `required` array.
    pub required: bool,
    /// Encoded via the `oneOf: [T, null]` nullability pattern.
    pub nullable: bool,
    /// `const` assertion (the fixed wire value).
    pub constant: Option<Scalar>,
    /// `default` (off-the-wire, materialized on read).
    pub default: Option<Scalar>,
    /// `x-<lang>-name` overrides.
    pub overrides: NameOverrides,
}

/// Per-language identifier overrides declared on a member or type.
#[derive(Clone, Debug, Default)]
pub struct NameOverrides {
    pub go: Option<String>,
    pub java: Option<String>,
    pub python: Option<String>,
    pub typescript: Option<String>,
}

/// A lowered field type.
#[derive(Clone, Debug, PartialEq)]
pub enum FieldType {
    String,
    Integer,
    Number,
    Boolean,
    /// A homogeneous array (`items`).
    Array(Box<FieldType>),
    /// A reference to another named type (record or map), by symbol id.
    Ref(SymbolId),
}

/// A scalar `const` / `default` value.
#[derive(Clone, Debug, PartialEq)]
pub enum Scalar {
    String(String),
    Integer(i64),
    Number(f64),
    Boolean(bool),
}

impl Scalar {
    /// The declared JSON type this scalar is compatible with.
    pub fn matches_type(&self, primitive: &FieldType) -> bool {
        matches!(
            (self, primitive),
            (Scalar::String(_), FieldType::String)
                | (Scalar::Integer(_), FieldType::Integer)
                | (Scalar::Integer(_), FieldType::Number)
                | (Scalar::Number(_), FieldType::Number)
                | (Scalar::Boolean(_), FieldType::Boolean)
        )
    }
}
