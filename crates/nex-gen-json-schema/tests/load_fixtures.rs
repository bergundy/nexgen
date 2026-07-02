//! Load-time accept/reject fixtures drawn from the `features/*/spec.md`
//! property-testing matrices. Each `accept_*` schema must load; each `reject_*`
//! schema must be rejected by the strict-subset gate or a document rule.

use std::path::PathBuf;

use nex_gen_core::Language;
use nex_gen_json_schema::loader::load_sources;
use serde_json::{Value, json};

/// Load a single JSON document (given as a [`Value`]) for Go and return the
/// result. Go is used because its PascalCase mapping surfaces the collision
/// cases the fixtures exercise.
fn load(doc: Value) -> Result<(), String> {
    let sources = vec![(PathBuf::from("test.json"), doc.to_string())];
    load_sources(sources, Language::Go)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Wrap a member schema as the single property of a closed object `$defs` type.
fn member(schema: Value) -> Value {
    json!({"$defs": {"T": {
        "type": "object",
        "additionalProperties": false,
        "properties": {"m": schema},
    }}})
}

/// Wrap a member schema as a *required* property (for default-on-required etc.).
fn required_member(schema: Value) -> Value {
    json!({"$defs": {"T": {
        "type": "object",
        "additionalProperties": false,
        "properties": {"m": schema},
        "required": ["m"],
    }}})
}

/// Wrap an object schema as a `$defs` type.
fn object(schema: Value) -> Value {
    json!({"$defs": {"T": schema}})
}

fn accept(doc: Value) {
    if let Err(e) = load(doc.clone()) {
        panic!("expected accept, got reject: {e}\n{doc}");
    }
}

fn reject(doc: Value) {
    if load(doc.clone()).is_ok() {
        panic!("expected reject, but accepted:\n{doc}");
    }
}

// ---------------------------------------------------------------------------
// type
// ---------------------------------------------------------------------------

#[test]
fn type_accepts_single_primitives() {
    for t in ["boolean", "number", "string", "integer"] {
        accept(member(json!({"type": t})));
    }
    accept(member(json!({"type": "array", "items": {"type": "string"}})));
}

#[test]
fn type_rejects_array_form() {
    reject(member(json!({"type": ["string", "null"]})));
    reject(member(json!({"type": ["integer", "number"]})));
}

#[test]
fn type_rejects_missing() {
    reject(member(json!({"description": "no type"})));
    reject(member(json!({})));
}

#[test]
fn type_rejects_unshaped_object() {
    reject(object(json!({"type": "object"})));
}

#[test]
fn type_rejects_null_standalone() {
    reject(member(json!({"type": "null"})));
}

#[test]
fn type_rejects_unknown_names() {
    for t in ["int", "float", "date", "any", "bigint", "String", "INTEGER"] {
        reject(member(json!({"type": t})));
    }
}

// ---------------------------------------------------------------------------
// properties
// ---------------------------------------------------------------------------

#[test]
fn properties_accepts_typed_struct() {
    accept(object(json!({
        "type": "object",
        "properties": {"id": {"type": "integer"}, "name": {"type": "string"}},
    })));
}

#[test]
fn properties_accepts_recased_names() {
    accept(object(json!({
        "type": "object",
        "properties": {"user_id": {"type": "string"}},
    })));
}

#[test]
fn properties_rejects_non_object_value() {
    reject(object(json!({"type": "object", "properties": []})));
}

#[test]
fn properties_rejects_non_schema_member() {
    reject(object(json!({"type": "object", "properties": {"a": 5}})));
}

#[test]
fn properties_rejects_shapeless_member() {
    reject(object(json!({"type": "object", "properties": {"a": {}}})));
}

#[test]
fn properties_rejects_member_missing_type() {
    reject(object(json!({"type": "object", "properties": {"a": {"minLength": 1}}})));
}

#[test]
fn properties_rejects_collision_after_recasing() {
    // user_id and userId both map to Go `UserId`/`UserID`? heck: user_id ->
    // UserId, userId -> UserId. Collision in Go.
    reject(object(json!({
        "type": "object",
        "properties": {"user_id": {"type": "string"}, "userId": {"type": "string"}},
    })));
}

// ---------------------------------------------------------------------------
// additionalProperties
// ---------------------------------------------------------------------------

#[test]
fn additional_properties_accepts_all_valid_forms() {
    accept(object(json!({"type": "object", "properties": {"id": {"type": "integer"}}})));
    accept(object(json!({"type": "object", "properties": {"id": {"type": "integer"}}, "additionalProperties": false})));
    accept(object(json!({"type": "object", "properties": {"id": {"type": "integer"}}, "additionalProperties": true})));
    accept(object(json!({"type": "object", "additionalProperties": true})));
    accept(object(json!({"type": "object", "additionalProperties": {"type": "string"}})));
    accept(object(json!({"type": "object", "properties": {"id": {"type": "integer"}}, "additionalProperties": {"type": "string"}})));
    accept(object(json!({"type": "object", "additionalProperties": false})));
}

#[test]
fn additional_properties_rejects_empty_schema_spelling() {
    reject(object(json!({"type": "object", "additionalProperties": {}})));
}

#[test]
fn additional_properties_rejects_unshaped_extras() {
    reject(object(json!({"type": "object", "additionalProperties": {"type": "object"}})));
}

// ---------------------------------------------------------------------------
// required
// ---------------------------------------------------------------------------

#[test]
fn required_accepts() {
    accept(object(json!({
        "type": "object",
        "properties": {"id": {"type": "integer"}, "name": {"type": "string"}},
        "required": ["id"],
    })));
    accept(object(json!({"type": "object", "properties": {"id": {"type": "integer"}}, "required": []})));
}

#[test]
fn required_rejects_bad_forms() {
    reject(object(json!({"type": "object", "properties": {"id": {"type": "integer"}}, "required": "id"})));
    reject(object(json!({"type": "object", "properties": {"id": {"type": "integer"}}, "required": [1]})));
    reject(object(json!({"type": "object", "properties": {"id": {"type": "integer"}}, "required": ["id", "id"]})));
    reject(object(json!({"type": "object", "properties": {"id": {"type": "integer"}}, "required": ["name"]})));
}

// ---------------------------------------------------------------------------
// min/maxProperties
// ---------------------------------------------------------------------------

#[test]
fn max_properties_accepts() {
    accept(object(json!({"type": "object", "additionalProperties": true, "maxProperties": 3})));
    accept(object(json!({"type": "object", "additionalProperties": false, "maxProperties": 0})));
}

#[test]
fn max_properties_rejects() {
    reject(object(json!({"type": "object", "additionalProperties": true, "maxProperties": -1})));
    reject(object(json!({"type": "object", "additionalProperties": true, "maxProperties": 1.5})));
    reject(object(json!({"type": "object", "additionalProperties": true, "maxProperties": "3"})));
    reject(object(json!({"type": "object", "additionalProperties": true, "minProperties": 5, "maxProperties": 2})));
}

#[test]
fn min_properties_accepts() {
    accept(object(json!({"type": "object", "additionalProperties": true, "minProperties": 1})));
    accept(object(json!({"type": "object", "properties": {"a": {"type": "string"}}, "minProperties": 0})));
}

#[test]
fn min_properties_rejects_bad_forms() {
    reject(object(json!({"type": "object", "additionalProperties": true, "minProperties": -1})));
    reject(object(json!({"type": "object", "additionalProperties": true, "minProperties": 1.5})));
    reject(object(json!({"type": "object", "additionalProperties": true, "minProperties": "1"})));
}

// ---------------------------------------------------------------------------
// dependentRequired
// ---------------------------------------------------------------------------

#[test]
fn dependent_required_accepts() {
    accept(object(json!({
        "type": "object",
        "properties": {"a": {"type": "string"}, "b": {"type": "string"}},
        "dependentRequired": {"a": ["b"]},
    })));
    accept(object(json!({"type": "object", "properties": {"a": {"type": "string"}}, "dependentRequired": {}})));
}

#[test]
fn dependent_required_rejects() {
    reject(object(json!({"type": "object", "properties": {"a": {"type": "string"}}, "dependentRequired": []})));
    reject(object(json!({"type": "object", "properties": {"a": {"type": "string"}}, "dependentRequired": {"a": "b"}})));
    reject(object(json!({"type": "object", "properties": {"a": {"type": "string"}, "b": {"type": "string"}}, "dependentRequired": {"a": ["b", "b"]}})));
    reject(object(json!({"type": "object", "properties": {"id": {"type": "integer"}}, "dependentRequired": {"name": ["email"]}})));
    reject(object(json!({
        "type": "object",
        "properties": {"a": {"type": "string"}, "b": {"type": "string"}},
        "required": ["b"],
        "dependentRequired": {"a": ["b"]},
    })));
    reject(object(json!({
        "type": "object",
        "properties": {"a": {"type": "string"}, "b": {"type": "string"}},
        "required": ["a"],
        "dependentRequired": {"a": ["b"]},
    })));
}

// ---------------------------------------------------------------------------
// patternProperties (temporarily unsupported)
// ---------------------------------------------------------------------------

#[test]
fn pattern_properties_rejected() {
    reject(object(json!({"type": "object", "patternProperties": {"^x-": {"type": "string"}}})));
    reject(object(json!({"type": "object", "properties": {"id": {"type": "integer"}}, "patternProperties": {"^m": {"type": "string"}}})));
}

// ---------------------------------------------------------------------------
// propertyNames
// ---------------------------------------------------------------------------

#[test]
fn property_names_accepts_on_maps() {
    accept(object(json!({"type": "object", "additionalProperties": {"type": "integer"}, "propertyNames": {"type": "string", "pattern": "^[a-z]+$"}})));
    accept(object(json!({"type": "object", "additionalProperties": true, "propertyNames": {"type": "string", "maxLength": 64}})));
}

#[test]
fn property_names_rejects() {
    reject(object(json!({"type": "object", "properties": {"id": {"type": "integer"}}, "propertyNames": {"type": "string", "pattern": "x"}})));
    reject(object(json!({"type": "object", "additionalProperties": true, "propertyNames": {"type": "integer"}})));
    reject(object(json!({"type": "object", "additionalProperties": true, "propertyNames": {}})));
    reject(object(json!({"type": "object", "additionalProperties": true, "propertyNames": true})));
}

// ---------------------------------------------------------------------------
// const
// ---------------------------------------------------------------------------

#[test]
fn const_accepts_scalars() {
    accept(member(json!({"type": "string", "const": "user"})));
    accept(member(json!({"type": "integer", "const": 1})));
    accept(member(json!({"type": "boolean", "const": true})));
    accept(member(json!({"type": "number", "const": 1.5})));
}

#[test]
fn const_rejects() {
    reject(member(json!({"type": "integer", "const": "x"})));
    reject(member(json!({"type": "string", "const": "v1", "default": "v1"})));
    reject(member(json!({"type": "string", "enum": ["a"], "const": "a"})));
    reject(member(json!({"type": "null", "const": null})));
    reject(member(json!({"type": "object", "const": {"a": 1}, "additionalProperties": false, "properties": {"a": {"type": "integer"}}})));
    reject(member(json!({"type": "array", "const": [1], "items": {"type": "integer"}})));
}

// ---------------------------------------------------------------------------
// default
// ---------------------------------------------------------------------------

#[test]
fn default_accepts_scalars() {
    accept(member(json!({"type": "string", "default": "anon"})));
    accept(member(json!({"type": "integer", "default": 0})));
    accept(member(json!({"type": "boolean", "default": false})));
    accept(member(json!({"oneOf": [{"type": "string"}, {"type": "null"}], "default": "x"})));
}

#[test]
fn default_rejects() {
    reject(required_member(json!({"type": "string", "default": "a"})));
    reject(member(json!({"type": "string", "default": 42})));
    reject(member(json!({"oneOf": [{"type": "string"}, {"type": "null"}], "default": null})));
}

// ---------------------------------------------------------------------------
// nullability
// ---------------------------------------------------------------------------

#[test]
fn nullability_accepts_pattern() {
    accept(member(json!({"oneOf": [{"type": "string"}, {"type": "null"}]})));
    accept(member(json!({"oneOf": [{"type": "null"}, {"type": "string"}]})));
}

#[test]
fn nullability_rejects() {
    reject(member(json!({"type": ["string", "null"]})));
    reject(member(json!({"type": "string", "nullable": true, "x-oas": true})));
    reject(member(json!({"oneOf": [{"type": "string"}, {"type": "null"}, {"type": "integer"}]})));
    reject(member(json!({"oneOf": [{"type": "string"}]})));
    reject(member(json!({"oneOf": [{"type": "string"}, {"type": "integer"}]})));
}

// ---------------------------------------------------------------------------
// $ref
// ---------------------------------------------------------------------------

#[test]
fn ref_accepts_named_local() {
    accept(json!({
        "$defs": {
            "Address": {"type": "object", "properties": {"city": {"type": "string"}}},
            "T": {"type": "object", "properties": {"addr": {"$ref": "#/$defs/Address"}}},
        }
    }));
}

#[test]
fn ref_rejects_siblings() {
    reject(json!({
        "$defs": {
            "Address": {"type": "object", "properties": {"city": {"type": "string"}}},
            "T": {"type": "object", "properties": {"addr": {"$ref": "#/$defs/Address", "description": "x"}}},
        }
    }));
}

#[test]
fn ref_rejects_id_and_remote_and_pointer() {
    reject(json!({"$id": "http://example.com", "$defs": {"T": {"type": "object", "properties": {"a": {"type": "string"}}}}}));
    reject(member(json!({"$ref": "https://example.com/s.json"})));
    reject(member(json!({"$ref": "#/properties/x/items"})));
    reject(member(json!({"$ref": "#/$defs/Missing"})));
}

// ---------------------------------------------------------------------------
// document rules
// ---------------------------------------------------------------------------

#[test]
fn rejects_stray_services_without_marker() {
    reject(json!({"services": {"ChatService": {"operations": {"ping": {}}}}}));
}

#[test]
fn rejects_bad_schema_dialect() {
    reject(json!({"$schema": "http://json-schema.org/draft-07/schema#", "$defs": {"T": {"type": "object", "properties": {"a": {"type": "string"}}}}}));
}

#[test]
fn rejects_nexus_root_schema_shaped() {
    reject(json!({"nexusrpc": "1.0.0", "type": "object", "properties": {"a": {"type": "string"}}}));
}

// ---------------------------------------------------------------------------
// services
// ---------------------------------------------------------------------------

#[test]
fn services_accepts_valid() {
    accept(json!({
        "nexusrpc": "1.0.0",
        "services": {
            "ChatService": {
                "fqn": "example.v1.ChatService",
                "operations": {
                    "sendMessage": {
                        "input": {"type": "object", "additionalProperties": false, "properties": {"message": {"type": "string"}}},
                    },
                },
            },
        },
    }));
}

#[test]
fn services_rejects_bad_names_and_shapes() {
    reject(json!({"nexusrpc": "1.0.0", "services": {"chatService": {"operations": {"ping": {}}}}}));
    reject(json!({"nexusrpc": "1.0.0", "services": {"Chat_Service": {"operations": {"ping": {}}}}}));
    reject(json!({"nexusrpc": "1.0.0", "services": {"ChatService": {"operations": {}}}}));
    reject(json!({"nexusrpc": "1.0.0", "services": {"ChatService": {"operations": {"PollMessages": {}}}}}));
    reject(json!({"nexusrpc": "1.0.0", "services": {"ChatService": {"operations": {"sendMessage": {"input": {"type": "object"}}}}}}));
    reject(json!({"nexusrpc": "1.0.0", "services": {"ChatService": {"operations": {"sendMessage": {"input": {"type": "string"}}}}}}));
}

#[test]
fn nexusrpc_version_must_be_exact() {
    reject(json!({"nexusrpc": "1.1.0", "services": {"ChatService": {"operations": {"ping": {}}}}}));
    reject(json!({"nexusrpc": 1, "services": {"ChatService": {"operations": {"ping": {}}}}}));
}
