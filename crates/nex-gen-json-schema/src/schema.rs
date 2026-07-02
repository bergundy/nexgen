//! The JSON Schema 2020-12 document model.
//!
//! This is a faithful, round-trippable model of the *subset of* JSON Schema
//! documents `nex-gen` reads. Parsing accepts both YAML and JSON (YAML is a
//! superset of JSON, so one path handles both). Every keyword the loader or an
//! emitter needs is a typed field; everything else is preserved verbatim in
//! [`Schema::extra`] so a document round-trips (`parse` then serialize) back to
//! a semantically identical value.
//!
//! The model deliberately does **not** enforce the strict subset — that is the
//! [`loader`](crate::loader)'s job. Here we only decode structure.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A parsed input document (the file root), either a Nexus document or a pure
/// JSON Schema, decided by the presence of the `nexusrpc` marker.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Document {
    /// The Nexus document marker (`nexusrpc: "1.0.0"`). Present only in Nexus
    /// documents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nexusrpc: Option<Value>,

    /// The dialect URI (`$schema`).
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<Value>,

    /// Nexus service bindings (Nexus documents only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub services: Option<IndexMap<String, Service>>,

    /// Reusable named definitions.
    #[serde(rename = "$defs", skip_serializing_if = "Option::is_none")]
    pub defs: Option<IndexMap<String, Schema>>,

    /// Root-level schema keywords (pure JSON Schema mode). These are flattened
    /// so a pure schema document decodes both here and — after
    /// [`Document::root_schema`] — as a [`Schema`].
    #[serde(flatten)]
    pub root: Schema,
}

impl Document {
    /// Whether this document carries the `nexusrpc` marker.
    pub fn is_nexus(&self) -> bool {
        self.nexusrpc.is_some()
    }

    /// The document's `description`, if any (mirrors the root schema's).
    pub fn description(&self) -> Option<&str> {
        self.root.description.as_deref()
    }
}

/// A Nexus service binding.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Service {
    /// Optional wire name (fully-qualified). Defaults to the service key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fqn: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub operations: IndexMap<String, Operation>,
    #[serde(flatten)]
    pub extra: IndexMap<String, Value>,
}

/// A single Nexus operation.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Operation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fqn: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<Schema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<Schema>,
    #[serde(flatten)]
    pub extra: IndexMap<String, Value>,
}

/// A JSON Schema (2020-12 subset), round-trippable.
///
/// Typed fields cover every keyword the loader inspects; unknown or
/// not-yet-modeled keywords land in [`Schema::extra`] so nothing is dropped on
/// re-serialize.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct Schema {
    /// `$ref` target string.
    #[serde(rename = "$ref", skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,

    /// Nested `$id` (always rejected).
    #[serde(rename = "$id", skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,

    /// `type` keyword. Kept as a raw [`Value`] so the array form and non-string
    /// forms round-trip and can be rejected with a precise diagnostic.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub ty: Option<Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Object `properties`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<IndexMap<String, Schema>>,

    /// `required` names.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Value>,

    /// `additionalProperties`: `false`, `true`, or a schema.
    #[serde(rename = "additionalProperties", skip_serializing_if = "Option::is_none")]
    pub additional_properties: Option<Box<AdditionalProperties>>,

    #[serde(rename = "patternProperties", skip_serializing_if = "Option::is_none")]
    pub pattern_properties: Option<Value>,

    #[serde(rename = "propertyNames", skip_serializing_if = "Option::is_none")]
    pub property_names: Option<Value>,

    #[serde(rename = "minProperties", skip_serializing_if = "Option::is_none")]
    pub min_properties: Option<Value>,
    #[serde(rename = "maxProperties", skip_serializing_if = "Option::is_none")]
    pub max_properties: Option<Value>,

    #[serde(rename = "dependentRequired", skip_serializing_if = "Option::is_none")]
    pub dependent_required: Option<Value>,

    /// Array `items` schema.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<Schema>>,

    /// The nullability / union applicator.
    #[serde(rename = "oneOf", skip_serializing_if = "Option::is_none")]
    pub one_of: Option<Vec<Schema>>,

    #[serde(
        rename = "const",
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_present_value"
    )]
    pub constant: Option<Value>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_present_value"
    )]
    pub default: Option<Value>,

    /// Everything else (unsupported applicators, extra annotations,
    /// `x-<lang>-name` overrides, string/number constraints not yet modeled),
    /// preserved verbatim for round-trip fidelity and precise rejection.
    #[serde(flatten)]
    pub extra: IndexMap<String, Value>,
}

impl Schema {
    /// `const` under its Rust-safe field name.
    pub fn get_const(&self) -> Option<&Value> {
        self.constant.as_ref()
    }

    /// Whether this schema is empty (`{}`) — no keywords at all.
    pub fn is_empty(&self) -> bool {
        *self == Schema::default()
    }

    /// Whether this schema is a bare `$ref` — the reference and nothing else.
    /// A `$ref` carrying *any* sibling keyword (even `description`) is not bare
    /// and is rejected by the loader.
    pub fn is_bare_ref(&self) -> bool {
        self.reference.is_some()
            && Schema {
                reference: None,
                ..self.clone()
            } == Schema::default()
    }

    /// An `x-<lang>-name` override for a language, if present.
    pub fn name_override(&self, key: &str) -> Option<&str> {
        self.extra.get(key).and_then(Value::as_str)
    }
}

/// The `additionalProperties` keyword's three shapes.
#[derive(Clone, Debug, PartialEq)]
pub enum AdditionalProperties {
    /// `additionalProperties: false` — closed struct.
    Closed,
    /// `additionalProperties: true` — explicit open struct / opaque map.
    Open,
    /// `additionalProperties: <schema>` — typed extras / typed map.
    Schema(Schema),
}

impl Serialize for AdditionalProperties {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            AdditionalProperties::Closed => s.serialize_bool(false),
            AdditionalProperties::Open => s.serialize_bool(true),
            AdditionalProperties::Schema(schema) => schema.serialize(s),
        }
    }
}

impl<'de> Deserialize<'de> for AdditionalProperties {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(d)?;
        match value {
            Value::Bool(false) => Ok(AdditionalProperties::Closed),
            Value::Bool(true) => Ok(AdditionalProperties::Open),
            other => {
                let schema: Schema =
                    serde_json::from_value(other).map_err(serde::de::Error::custom)?;
                Ok(AdditionalProperties::Schema(schema))
            }
        }
    }
}

/// Deserialize a value that must preserve an explicit `null` as `Some(Null)`
/// (serde's default collapses `null` into `None` for `Option`). Used for
/// `const` / `default`, where `const: null` and `default: null` must be
/// distinguishable from absence so the loader can reject them.
fn de_present_value<'de, D>(d: D) -> Result<Option<Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(Value::deserialize(d)?))
}

/// Parse a document from bytes. YAML and JSON both parse through the YAML
/// reader (YAML 1.2 is a JSON superset).
pub fn parse_document(text: &str) -> Result<Document, String> {
    serde_yaml::from_str(text).map_err(|e| e.to_string())
}

/// Parse a standalone schema (used by the round-trip tests and inline I/O).
pub fn parse_schema(text: &str) -> Result<Schema, String> {
    serde_yaml::from_str(text).map_err(|e| e.to_string())
}

/// Serialize a schema back to a canonical [`Value`].
pub fn schema_to_value(schema: &Schema) -> Value {
    serde_json::to_value(schema).expect("schema serializes")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A canonical schema round-trips to a semantically identical value.
    fn assert_roundtrip(json: &str) {
        let canonical: Value = serde_json::from_str(json).expect("valid json");
        let schema: Schema = serde_json::from_value(canonical.clone()).expect("decodes");
        let back = schema_to_value(&schema);
        assert_eq!(canonical, back, "round-trip changed the schema");
    }

    #[test]
    fn roundtrips_scalar() {
        assert_roundtrip(r#"{"type":"string"}"#);
        assert_roundtrip(r#"{"type":"integer","default":0}"#);
        assert_roundtrip(r#"{"type":"string","const":"text"}"#);
    }

    #[test]
    fn roundtrips_object() {
        assert_roundtrip(
            r#"{"type":"object","additionalProperties":false,
                "properties":{"a":{"type":"string"},"b":{"type":"integer"}},
                "required":["a"]}"#,
        );
    }

    #[test]
    fn roundtrips_nullability_and_maps() {
        assert_roundtrip(r#"{"oneOf":[{"type":"string"},{"type":"null"}]}"#);
        assert_roundtrip(
            r#"{"type":"object","additionalProperties":{"type":"string"},"maxProperties":50}"#,
        );
    }

    #[test]
    fn roundtrips_unmodeled_keywords() {
        // minLength/pattern aren't typed fields yet — they must still round-trip.
        assert_roundtrip(r#"{"type":"string","minLength":1,"pattern":"^[a-z]+$"}"#);
    }

    /// The canonical chat document round-trips through the [`Document`] model
    /// back to a semantically identical value (comments aside).
    #[test]
    fn roundtrips_canonical_chat_document() {
        let yaml = include_str!("../spec/samples/chat.nexusrpc.yaml");
        let canonical: Value = serde_yaml::from_str(yaml).expect("valid yaml");
        let doc: Document = serde_json::from_value(canonical.clone()).expect("decodes");
        let back = serde_json::to_value(&doc).expect("serializes");
        assert_eq!(canonical, back, "document round-trip changed the schema");
    }

    #[test]
    fn detects_bare_ref() {
        let s = parse_schema(r##"{"$ref":"#/$defs/Foo"}"##).unwrap();
        assert!(s.is_bare_ref());
        let s = parse_schema(r##"{"$ref":"#/$defs/Foo","type":"string"}"##).unwrap();
        assert!(!s.is_bare_ref());
    }
}
