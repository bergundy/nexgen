//! The loader: parse → mode → resolve `$ref` → strict-subset gate → IR.
//!
//! Implements [`nex_gen_core::Loader`]. All input validation lives here: the
//! two file modes, the root rules, `$ref`/`$defs` resolution over the local
//! input closure, the strict JSON Schema subset gate (rejecting unsupported
//! keywords with fix-it diagnostics), and the per-language identifier collision
//! pass. Loud rejection over silently-wrong output is the core principle.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use nex_gen_core::{
    Error, IR, Language, LoadOutput, Loader, Name, Operation, Result, Service, Symbol, SymbolId,
    SymbolTable,
};
use serde_json::Value;

use crate::ir::{Additional, Field, FieldType, Kind, MapType, NameOverrides, Record, Scalar};
use crate::naming;
use crate::schema::{self, AdditionalProperties, Document, Schema};

/// The JSON Schema front-end loader.
pub struct SchemaLoader {
    inputs: Vec<PathBuf>,
}

impl SchemaLoader {
    pub fn new(inputs: Vec<PathBuf>) -> Self {
        Self { inputs }
    }

}

/// Lower a set of in-memory `(path, text)` sources directly, bypassing the
/// filesystem. The public entry point for tests and embedding.
pub fn load_sources(
    sources: Vec<(PathBuf, String)>,
    language: Language,
) -> Result<LoadOutput<Kind>> {
    lower(sources, language).map_err(|message| Error::Load { message })
}

impl Loader for SchemaLoader {
    type Kind = Kind;

    fn load(&self, language: Language) -> Result<LoadOutput<Kind>> {
        let mut sources = Vec::new();
        for path in &self.inputs {
            let text = std::fs::read_to_string(path).map_err(|source| Error::ReadFile {
                path: path.clone(),
                source,
            })?;
            sources.push((path.clone(), text));
        }
        lower(sources, language).map_err(|message| Error::Load { message })
    }
}

/// A key identifying a named type in the closure.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
enum TypeKey {
    Root(PathBuf),
    Def(PathBuf, String),
}

/// The whole lowering: parse every source, gate the subset, build the IR, and
/// run the per-language collision pass. Returns the first diagnostic on failure.
fn lower(sources: Vec<(PathBuf, String)>, language: Language) -> std::result::Result<LoadOutput<Kind>, String> {
    let mut docs: IndexMap<PathBuf, Document> = IndexMap::new();
    for (path, text) in &sources {
        let doc = schema::parse_document(text)
            .map_err(|e| format!("{}: failed to parse: {e}", path.display()))?;
        docs.insert(canonical(path), doc);
    }

    // Pass 1: validate document-level rules and collect every named type.
    let mut registry: BTreeMap<TypeKey, SymbolId> = BTreeMap::new();
    let mut table: SymbolTable<Kind> = SymbolTable::new();
    // Deferred lowering work: (id, TypeKey, schema, docs, synthesized).
    let mut pending: Vec<(SymbolId, PathBuf, Schema, bool)> = Vec::new();
    // Service work: (path, key, Service schema).
    let mut service_work: Vec<(PathBuf, String, schema::Service)> = Vec::new();

    for (path, doc) in &docs {
        validate_document(path, doc)?;
        collect_types(path, doc, &mut registry, &mut table, &mut pending, &mut service_work)?;
    }

    // Pass 2: lower each collected type schema into a Record/Map symbol.
    for (id, path, node, synthesized) in &pending {
        let kind = lower_named_type(path, node, *synthesized, &registry, &docs)?;
        insert_symbol(&mut table, *id, kind);
    }

    // Pass 3: services.
    for (path, key, svc) in &service_work {
        let id = *registry.get(&TypeKey::Def(path.clone(), format!("service::{key}"))).unwrap();
        let service = lower_service(path, key, svc, &registry, &docs)?;
        let refs = service_refs(&service);
        table.insert(Symbol {
            id,
            name: Name::new(key.clone()),
            refs,
            kind: Kind::Service(service),
        });
    }

    // Pass 4: per-language identifier collision pass (P15).
    collision_check(&table, language)?;

    Ok(LoadOutput::new(IR::new(table)))
}

fn insert_symbol(table: &mut SymbolTable<Kind>, id: SymbolId, kind: Kind) {
    let name = table.get(id).expect("preallocated").name.clone();
    let refs = kind_refs(&kind);
    table.insert(Symbol {
        id,
        name,
        refs,
        kind,
    });
}

fn kind_refs(kind: &Kind) -> Vec<SymbolId> {
    let mut refs = Vec::new();
    match kind {
        Kind::Record(record) => {
            for field in &record.fields {
                collect_type_refs(&field.ty, &mut refs);
            }
            if let Additional::Typed(ty) = &record.additional {
                collect_type_refs(ty, &mut refs);
            }
        }
        Kind::Map(map) => collect_type_refs(&map.value, &mut refs),
        Kind::Service(_) => {}
    }
    refs.sort();
    refs.dedup();
    refs
}

fn collect_type_refs(ty: &FieldType, out: &mut Vec<SymbolId>) {
    match ty {
        FieldType::Ref(id) => out.push(*id),
        FieldType::Array(inner) => collect_type_refs(inner, out),
        _ => {}
    }
}

fn service_refs(service: &Service) -> Vec<SymbolId> {
    let mut refs = Vec::new();
    for op in &service.operations {
        refs.extend(op.input);
        refs.extend(op.output);
    }
    refs.sort();
    refs.dedup();
    refs
}

// ---------------------------------------------------------------------------
// Document-level rules (input-files.md).
// ---------------------------------------------------------------------------

fn validate_document(path: &Path, doc: &Document) -> std::result::Result<(), String> {
    let loc = path.display();
    // $id anywhere at the root is rejected (owned by $ref).
    if doc.root.id.is_some() {
        return Err(format!("{loc}: `$id` is not supported; refs resolve by file path and JSON pointer"));
    }
    // $schema must be the 2020-12 dialect if present.
    if let Some(schema) = &doc.schema {
        let ok = schema.as_str() == Some("https://json-schema.org/draft/2020-12/schema");
        if !ok {
            return Err(format!(
                "{loc}: `$schema` must be `https://json-schema.org/draft/2020-12/schema` (only JSON Schema 2020-12 is supported)"
            ));
        }
    }

    if doc.is_nexus() {
        // nexusrpc must be exactly "1.0.0".
        if doc.nexusrpc.as_ref().and_then(Value::as_str) != Some("1.0.0") {
            return Err(format!(
                "{loc}: `nexusrpc` must be exactly \"1.0.0\""
            ));
        }
        // The root is an envelope, not a type: no schema-shaped keyword allowed.
        if root_is_schema_shaped(&doc.root) {
            return Err(format!(
                "{loc}: a Nexus document root is an envelope, not a type — move the type into `$defs`"
            ));
        }
        // A service map must exist and be validated in collect_types.
    } else {
        // Pure JSON Schema: a stray top-level `services` needs the nexusrpc marker.
        if doc.services.is_some() {
            return Err(format!(
                "{loc}: `services` requires a Nexus document — add `nexusrpc: \"1.0.0\"`"
            ));
        }
    }
    Ok(())
}

/// Whether a pure-schema root carries any schema-shaped keyword (i.e. is itself
/// a type), as opposed to being definitions-only.
fn root_is_schema_shaped(root: &Schema) -> bool {
    root.reference.is_some()
        || root.ty.is_some()
        || root.properties.is_some()
        || root.additional_properties.is_some()
        || root.one_of.is_some()
        || root.items.is_some()
        || root.constant.is_some()
        || root.pattern_properties.is_some()
        || root.property_names.is_some()
}

// ---------------------------------------------------------------------------
// Type collection (pass 1): mint ids for every named type.
// ---------------------------------------------------------------------------

fn collect_types(
    path: &Path,
    doc: &Document,
    registry: &mut BTreeMap<TypeKey, SymbolId>,
    table: &mut SymbolTable<Kind>,
    pending: &mut Vec<(SymbolId, PathBuf, Schema, bool)>,
    service_work: &mut Vec<(PathBuf, String, schema::Service)>,
) -> std::result::Result<(), String> {
    let path = canonical(path);

    // $defs entries become named types.
    if let Some(defs) = &doc.defs {
        for (name, node) in defs {
            let id = mint(table, name);
            registry.insert(TypeKey::Def(path.clone(), name.clone()), id);
            pending.push((id, path.clone(), node.clone(), false));
        }
    }

    // Pure-schema root that is itself a (non-bare-ref) type.
    if !doc.is_nexus() && root_is_schema_shaped(&doc.root) && !doc.root.is_bare_ref() {
        let name = root_type_name(&path);
        let id = mint(table, &name);
        registry.insert(TypeKey::Root(path.clone()), id);
        pending.push((id, path.clone(), doc.root.clone(), false));
    }

    // Services: mint ids for the service symbol and any synthesized inline I/O.
    if let Some(services) = &doc.services {
        for (svc_name, svc) in services {
            validate_service_name(svc_name)?;
            if svc.operations.is_empty() {
                return Err(format!("service `{svc_name}` has no operations"));
            }
            let svc_id = mint(table, svc_name);
            registry.insert(TypeKey::Def(path.clone(), format!("service::{svc_name}")), svc_id);

            for (op_name, op) in &svc.operations {
                validate_operation_name(op_name)?;
                for (io, suffix) in [(&op.input, "Input"), (&op.output, "Output")] {
                    if let Some(schema) = io {
                        if schema.reference.is_none() {
                            // Inline object → synthesized <Op><Suffix> type.
                            let synth = format!("{}{}", pascal(op_name), suffix);
                            let key = TypeKey::Def(path.clone(), synth.clone());
                            if registry.contains_key(&key) {
                                return Err(format!(
                                    "synthesized type `{synth}` collides with an existing type"
                                ));
                            }
                            let id = mint(table, &synth);
                            registry.insert(key, id);
                            pending.push((id, path.clone(), schema.clone(), true));
                        }
                    }
                }
            }
            service_work.push((path.clone(), svc_name.clone(), svc.clone()));
        }
    }

    Ok(())
}

/// Mint a preallocated symbol slot carrying just the name (kind filled later).
fn mint(table: &mut SymbolTable<Kind>, name: &str) -> SymbolId {
    let id = table.alloc_id();
    table.insert(Symbol {
        id,
        name: Name::new(name.to_string()),
        refs: Vec::new(),
        // Placeholder kind; replaced in pass 2/3.
        kind: Kind::Record(Record {
            docs: None,
            fields: Vec::new(),
            additional: Additional::Open,
            min_properties: None,
            max_properties: None,
            dependent_required: Vec::new(),
            synthesized: false,
        }),
    });
    id
}

// ---------------------------------------------------------------------------
// Type lowering (pass 2): strict-subset gate + IR construction.
// ---------------------------------------------------------------------------

fn lower_named_type(
    path: &Path,
    node: &Schema,
    synthesized: bool,
    registry: &BTreeMap<TypeKey, SymbolId>,
    docs: &IndexMap<PathBuf, Document>,
) -> std::result::Result<Kind, String> {
    reject_common(node)?;

    // A named type must be an object (record) or a typed map. Bare-ref roots
    // and scalar roots are handled elsewhere / out of scope for v1 named types.
    let ty = node
        .ty
        .as_ref()
        .and_then(Value::as_str)
        .ok_or_else(|| format!("type schema is missing a `type` keyword"))?;
    if ty != "object" {
        return Err(format!(
            "top-level type must be `object` (got `{ty}`); wrap a scalar in a single-field object"
        ));
    }

    lower_object(path, node, synthesized, registry, docs)
}

/// Lower an object schema into a [`Record`] or a [`MapType`].
fn lower_object(
    path: &Path,
    node: &Schema,
    synthesized: bool,
    registry: &BTreeMap<TypeKey, SymbolId>,
    docs: &IndexMap<PathBuf, Document>,
) -> std::result::Result<Kind, String> {
    if node.pattern_properties.is_some() {
        return Err("`patternProperties` is not yet supported".to_string());
    }

    let has_props = node.properties.is_some();

    // propertyNames is only allowed on map-shaped objects (no properties).
    if node.property_names.is_some() && has_props {
        return Err("`propertyNames` is not supported alongside `properties`".to_string());
    }
    if let Some(pn) = &node.property_names {
        validate_property_names(pn)?;
    }

    let additional = lower_additional(path, node, registry, docs)?;
    let min_p = extract_count(&node.min_properties, "minProperties")?;
    let max_p = extract_count(&node.max_properties, "maxProperties")?;
    if let (Some(min), Some(max)) = (min_p, max_p) {
        if min > max {
            return Err(format!(
                "minProperties ({min}) must not exceed maxProperties ({max})"
            ));
        }
    }

    // A map-shaped object: no declared properties, typed/open additional.
    if !has_props {
        // Bare {type: object} with no shape at all is rejected.
        if node.additional_properties.is_none() {
            return Err(
                "`type: object` requires an explicit shape — add `properties`, `additionalProperties: true`, or `additionalProperties: false`".to_string(),
            );
        }
        let value = match &additional {
            Additional::Typed(ty) => ty.clone(),
            // additionalProperties:true / false with no properties → opaque map.
            _ => FieldType::String, // placeholder value type for opaque maps
        };
        if let Additional::Typed(_) = &additional {
            return Ok(Kind::Map(MapType {
                docs: node.description.clone(),
                value,
                min_properties: min_p,
                max_properties: max_p,
            }));
        }
        // additionalProperties: true/false with no properties → empty record.
        return Ok(Kind::Record(Record {
            docs: node.description.clone(),
            fields: Vec::new(),
            additional,
            min_properties: min_p,
            max_properties: max_p,
            dependent_required: Vec::new(),
            synthesized,
        }));
    }

    // A record with declared properties.
    let properties = node.properties.as_ref().unwrap();
    let required = extract_required(node, properties)?;
    let dependent_required = extract_dependent_required(node, properties, &required)?;

    let mut fields = Vec::new();
    for (json_name, member) in properties {
        let field = lower_field(json_name, member, required.contains(json_name), path, registry, docs)?;
        fields.push(field);
    }

    Ok(Kind::Record(Record {
        docs: node.description.clone(),
        fields,
        additional,
        min_properties: min_p,
        max_properties: max_p,
        dependent_required,
        synthesized,
    }))
}

/// Lower one property member schema into a [`Field`].
fn lower_field(
    json_name: &str,
    member: &Schema,
    required: bool,
    path: &Path,
    registry: &BTreeMap<TypeKey, SymbolId>,
    docs: &IndexMap<PathBuf, Document>,
) -> std::result::Result<Field, String> {
    reject_common(member)?;

    let (ty, nullable, inner_docs) = classify_member(json_name, member, path, registry, docs)?;

    // const / default handling.
    let primitive = &ty;
    let constant = match member.get_const() {
        Some(v) => {
            if v.is_null() {
                return Err(format!("`{json_name}`: `const: null` is not supported"));
            }
            let scalar = to_scalar(v)
                .ok_or_else(|| format!("`{json_name}`: composite `const` values are not supported"))?;
            if !scalar.matches_type(primitive) {
                return Err(format!("`{json_name}`: `const` value is incompatible with the declared type"));
            }
            Some(scalar)
        }
        None => None,
    };
    let default = match &member.default {
        Some(v) => {
            if v.is_null() {
                return Err(format!("`{json_name}`: `default: null` is not supported"));
            }
            if required {
                return Err(format!("`{json_name}`: `default` is not allowed on a required member"));
            }
            let scalar = to_scalar(v)
                .ok_or_else(|| format!("`{json_name}`: composite `default` values are not supported"))?;
            if !scalar.matches_type(primitive) {
                return Err(format!("`{json_name}`: `default` value is incompatible with the declared type"));
            }
            Some(scalar)
        }
        None => None,
    };
    if constant.is_some() && default.is_some() {
        return Err(format!("`{json_name}`: `const` and `default` are mutually exclusive"));
    }
    if member.extra.contains_key("enum") && constant.is_some() {
        return Err(format!("`{json_name}`: `const` and `enum` are mutually exclusive"));
    }

    let docs = member
        .description
        .clone()
        .or(inner_docs);

    Ok(Field {
        json_name: json_name.to_string(),
        docs,
        ty,
        required,
        nullable,
        constant,
        default,
        overrides: name_overrides(member),
    })
}

/// Classify a member schema into (type, nullable, docs), applying the strict
/// subset. Handles the nullability `oneOf` pattern, `$ref`, primitives, and
/// homogeneous arrays.
fn classify_member(
    json_name: &str,
    member: &Schema,
    path: &Path,
    registry: &BTreeMap<TypeKey, SymbolId>,
    docs: &IndexMap<PathBuf, Document>,
) -> std::result::Result<(FieldType, bool, Option<String>), String> {
    // Nullability: oneOf: [T, null].
    if let Some(branches) = &member.one_of {
        let (inner, doc) = parse_nullability(json_name, branches)?;
        let (ty, _, _) = classify_member(json_name, &inner, path, registry, docs)?;
        return Ok((ty, true, doc));
    }

    // $ref.
    if let Some(reference) = &member.reference {
        let id = resolve_ref(path, reference, registry, docs)?;
        return Ok((FieldType::Ref(id), false, None));
    }

    let ty = member
        .ty
        .as_ref()
        .ok_or_else(|| format!("`{json_name}`: member schema is missing a `type` keyword"))?;
    let ty = ty
        .as_str()
        .ok_or_else(|| format!("`{json_name}`: `type` must be a single string (array `type` is not supported)"))?;

    let field_ty = match ty {
        "string" => FieldType::String,
        "integer" => FieldType::Integer,
        "number" => FieldType::Number,
        "boolean" => FieldType::Boolean,
        "array" => {
            let items = member
                .items
                .as_ref()
                .ok_or_else(|| format!("`{json_name}`: array `type` requires an `items` schema"))?;
            let (inner, _, _) = classify_member(json_name, items, path, registry, docs)?;
            FieldType::Array(Box::new(inner))
        }
        "object" => {
            return Err(format!(
                "`{json_name}`: inline nested objects must be extracted to a `$defs` type and referenced with `$ref`"
            ));
        }
        "null" => {
            return Err(format!(
                "`{json_name}`: `type: \"null\"` is only allowed inside the nullability pattern `oneOf: [T, null]`"
            ));
        }
        other => {
            return Err(format!("`{json_name}`: unknown type `{other}`"));
        }
    };
    Ok((field_ty, false, None))
}

/// Parse the nullability `oneOf: [T, null]` pattern, returning the non-null
/// branch. Rejects every other `oneOf` shape.
fn parse_nullability(
    json_name: &str,
    branches: &[Schema],
) -> std::result::Result<(Schema, Option<String>), String> {
    if branches.len() != 2 {
        return Err(format!(
            "`{json_name}`: `oneOf` is only supported as the nullability pattern `[T, {{type: null}}]`"
        ));
    }
    let is_null = |s: &Schema| s.ty.as_ref().and_then(Value::as_str) == Some("null");
    let null_count = branches.iter().filter(|s| is_null(s)).count();
    if null_count != 1 {
        return Err(format!(
            "`{json_name}`: `oneOf` is only supported as the nullability pattern `[T, {{type: null}}]`"
        ));
    }
    let inner = branches.iter().find(|s| !is_null(s)).unwrap().clone();
    let docs = inner.description.clone();
    Ok((inner, docs))
}

/// Lower the `additionalProperties` keyword into the [`Additional`] policy.
fn lower_additional(
    path: &Path,
    node: &Schema,
    registry: &BTreeMap<TypeKey, SymbolId>,
    docs: &IndexMap<PathBuf, Document>,
) -> std::result::Result<Additional, String> {
    match &node.additional_properties {
        None => Ok(Additional::Open), // open by default
        Some(ap) => match ap.as_ref() {
            AdditionalProperties::Closed => Ok(Additional::Closed),
            AdditionalProperties::Open => Ok(Additional::Open),
            AdditionalProperties::Schema(schema) => {
                if schema.is_empty() {
                    return Err(
                        "`additionalProperties: {}` is ambiguous — use `true` for an open struct or a typed schema"
                            .to_string(),
                    );
                }
                let (ty, _, _) = classify_member("additionalProperties", schema, path, registry, docs)?;
                Ok(Additional::Typed(ty))
            }
        },
    }
}

fn validate_property_names(pn: &Value) -> std::result::Result<(), String> {
    let obj = pn
        .as_object()
        .ok_or_else(|| "`propertyNames` must be a string schema".to_string())?;
    if obj.is_empty() {
        return Err("`propertyNames` must constrain string keys (a `{}`/`true` schema is not allowed)".to_string());
    }
    if obj.get("type").and_then(Value::as_str) != Some("string") {
        return Err("`propertyNames` subschema must be `type: string`".to_string());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// required / dependentRequired / count extraction.
// ---------------------------------------------------------------------------

fn extract_required(
    node: &Schema,
    properties: &IndexMap<String, Schema>,
) -> std::result::Result<Vec<String>, String> {
    let Some(required) = &node.required else {
        return Ok(Vec::new());
    };
    let array = required
        .as_array()
        .ok_or_else(|| "`required` must be an array of member names".to_string())?;
    let mut names = Vec::new();
    for item in array {
        let name = item
            .as_str()
            .ok_or_else(|| "`required` entries must be strings".to_string())?;
        if names.contains(&name.to_string()) {
            return Err(format!("`required` lists `{name}` more than once"));
        }
        if !properties.contains_key(name) {
            return Err(format!("`required` names `{name}`, which is not a declared property"));
        }
        names.push(name.to_string());
    }
    Ok(names)
}

fn extract_dependent_required(
    node: &Schema,
    properties: &IndexMap<String, Schema>,
    required: &[String],
) -> std::result::Result<Vec<(String, Vec<String>)>, String> {
    let Some(dr) = &node.dependent_required else {
        return Ok(Vec::new());
    };
    let obj = dr
        .as_object()
        .ok_or_else(|| "`dependentRequired` must be an object".to_string())?;
    let mut out = Vec::new();
    for (trigger, deps) in obj {
        if !properties.contains_key(trigger) {
            return Err(format!("`dependentRequired` trigger `{trigger}` is not a declared property"));
        }
        if required.contains(trigger) {
            return Err(format!("`dependentRequired` trigger `{trigger}` is already required (vacuous)"));
        }
        let deps_arr = deps
            .as_array()
            .ok_or_else(|| format!("`dependentRequired.{trigger}` must be an array of member names"))?;
        let mut names = Vec::new();
        for dep in deps_arr {
            let dep = dep
                .as_str()
                .ok_or_else(|| format!("`dependentRequired.{trigger}` entries must be strings"))?;
            if names.contains(&dep.to_string()) {
                return Err(format!("`dependentRequired.{trigger}` lists `{dep}` more than once"));
            }
            if !properties.contains_key(dep) {
                return Err(format!("`dependentRequired.{trigger}` dependent `{dep}` is not a declared property"));
            }
            if required.contains(&dep.to_string()) {
                return Err(format!("`dependentRequired.{trigger}` dependent `{dep}` is already required (redundant)"));
            }
            names.push(dep.to_string());
        }
        out.push((trigger.clone(), names));
    }
    Ok(out)
}

fn extract_count(value: &Option<Value>, keyword: &str) -> std::result::Result<Option<u64>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let n = value
        .as_u64()
        .ok_or_else(|| format!("`{keyword}` must be a non-negative integer"))?;
    // Reject fractional / stringly forms: as_u64 already rejects those, but a
    // float like 1.5 becomes None too, so the message covers it.
    Ok(Some(n))
}

// ---------------------------------------------------------------------------
// $ref resolution.
// ---------------------------------------------------------------------------

fn resolve_ref(
    path: &Path,
    reference: &str,
    registry: &BTreeMap<TypeKey, SymbolId>,
    docs: &IndexMap<PathBuf, Document>,
) -> std::result::Result<SymbolId, String> {
    if reference.starts_with("http://") || reference.starts_with("https://") {
        return Err(format!("remote `$ref` `{reference}` is not supported (local files only)"));
    }
    let (file_part, pointer) = match reference.split_once('#') {
        Some((f, p)) => (f, p),
        None => (reference, ""),
    };

    let target_path = if file_part.is_empty() {
        canonical(path)
    } else {
        let base = path.parent().unwrap_or_else(|| Path::new("."));
        let joined = base.join(file_part);
        let canon = canonical(&joined);
        if !docs.contains_key(&canon) {
            return Err(format!("`$ref` target file `{file_part}` is not in the input set"));
        }
        canon
    };

    let key = if pointer.is_empty() || pointer == "/" {
        TypeKey::Root(target_path)
    } else if let Some(name) = pointer.strip_prefix("/$defs/") {
        // Un-escape JSON pointer tokens (~1 → /, ~0 → ~).
        let name = name.replace("~1", "/").replace("~0", "~");
        TypeKey::Def(target_path, name)
    } else {
        return Err(format!(
            "`$ref` `{reference}` must point at a `$defs` entry or a file root (no pointers into a schema body)"
        ));
    };

    registry
        .get(&key)
        .copied()
        .ok_or_else(|| format!("`$ref` `{reference}` does not resolve to a known type"))
}

// ---------------------------------------------------------------------------
// Services.
// ---------------------------------------------------------------------------

fn lower_service(
    path: &Path,
    key: &str,
    svc: &schema::Service,
    registry: &BTreeMap<TypeKey, SymbolId>,
    docs: &IndexMap<PathBuf, Document>,
) -> std::result::Result<Service, String> {
    let mut operations = Vec::new();
    for (op_name, op) in &svc.operations {
        let input = resolve_io(path, op_name, "Input", &op.input, registry, docs)?;
        let output = resolve_io(path, op_name, "Output", &op.output, registry, docs)?;
        operations.push(Operation {
            name: Name::new(op_name.clone()),
            wire_name: op.fqn.clone().unwrap_or_else(|| pascal(op_name)),
            experimental: false,
            input,
            output,
            docs: op.description.clone(),
            returns_doc: None,
        });
    }
    Ok(Service {
        name: Name::new(key.to_string()),
        wire_name: svc.fqn.clone().unwrap_or_else(|| key.to_string()),
        experimental: false,
        operations,
        docs: svc.description.clone(),
    })
}

fn resolve_io(
    path: &Path,
    op_name: &str,
    suffix: &str,
    io: &Option<Schema>,
    registry: &BTreeMap<TypeKey, SymbolId>,
    docs: &IndexMap<PathBuf, Document>,
) -> std::result::Result<Option<SymbolId>, String> {
    let Some(schema) = io else {
        return Ok(None);
    };
    if let Some(reference) = &schema.reference {
        let id = resolve_ref(path, reference, registry, docs)?;
        // The referenced type must be an object (record/map), not a scalar.
        return Ok(Some(id));
    }
    // Inline object: must be a shaped object.
    let ty = schema.ty.as_ref().and_then(Value::as_str);
    if ty != Some("object") {
        return Err(format!(
            "operation `{op_name}` {} must be an object type (a `$ref` to an object `$defs` or an inline object)",
            suffix.to_lowercase()
        ));
    }
    let synth = format!("{}{}", pascal(op_name), suffix);
    let id = registry
        .get(&TypeKey::Def(canonical(path), synth))
        .copied()
        .expect("synthesized type minted in pass 1");
    Ok(Some(id))
}

fn validate_service_name(name: &str) -> std::result::Result<(), String> {
    if !matches_ident(name, true) {
        return Err(format!(
            "service name `{name}` must match `^[A-Z][a-zA-Z0-9]+$`"
        ));
    }
    Ok(())
}

fn validate_operation_name(name: &str) -> std::result::Result<(), String> {
    if !matches_ident(name, false) {
        return Err(format!(
            "operation name `{name}` must match `^[a-z][a-zA-Z0-9]+$`"
        ));
    }
    Ok(())
}

/// Match `^[A-Z][a-zA-Z0-9]+$` (upper=true) or `^[a-z][a-zA-Z0-9]+$`.
fn matches_ident(name: &str, upper_first: bool) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    let first_ok = if upper_first {
        first.is_ascii_uppercase()
    } else {
        first.is_ascii_lowercase()
    };
    if !first_ok {
        return false;
    }
    let rest: Vec<char> = chars.collect();
    if rest.is_empty() {
        return false; // needs 2+ chars per the regex `+`
    }
    rest.iter().all(|c| c.is_ascii_alphanumeric())
}

// ---------------------------------------------------------------------------
// Shared rejection helpers (strict-subset gate).
// ---------------------------------------------------------------------------

/// Reject keywords that are never supported in any schema position: `$id`,
/// sibling keywords on a `$ref`, and the P6-rejected applicators.
fn reject_common(schema: &Schema) -> std::result::Result<(), String> {
    if schema.id.is_some() {
        return Err("`$id` is not supported".to_string());
    }
    if schema.reference.is_some() && !schema.is_bare_ref() {
        return Err("a `$ref` must not carry sibling keywords".to_string());
    }
    for rejected in [
        "allOf",
        "anyOf",
        "not",
        "if",
        "then",
        "else",
        "prefixItems",
        "unevaluatedProperties",
        "unevaluatedItems",
        "dependentSchemas",
        "contains",
        "maxContains",
        "minContains",
        "$anchor",
        "$dynamicRef",
        "$dynamicAnchor",
        // OpenAPI 3.0 `nullable` — use the `oneOf: [T, null]` pattern instead.
        "nullable",
    ] {
        if schema.extra.contains_key(rejected) {
            return Err(format!("`{rejected}` is not supported (strict subset, P6)"));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Collision pass (P15).
// ---------------------------------------------------------------------------

fn collision_check(table: &SymbolTable<Kind>, language: Language) -> std::result::Result<(), String> {
    // Package-scope: type + service identifiers must not collide after mapping.
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for symbol in table.iter() {
        let ident = naming::type_ident(symbol.name.as_str(), language, &NameOverrides::default());
        if let Some(prev) = seen.insert(ident.clone(), symbol.name.0.clone()) {
            if prev != symbol.name.0 {
                return Err(format!(
                    "types `{prev}` and `{}` both map to `{ident}` in {language} — add an `x-{}-name` override",
                    symbol.name.0,
                    language.as_str()
                ));
            }
        }
    }

    // Per-record member-scope collisions.
    for symbol in table.iter() {
        if let Kind::Record(record) = &symbol.kind {
            let mut members: BTreeMap<String, String> = BTreeMap::new();
            for field in &record.fields {
                let ident = naming::field_ident(&field.json_name, language, &field.overrides);
                if let Some(prev) = members.insert(ident.clone(), field.json_name.clone()) {
                    return Err(format!(
                        "members `{prev}` and `{}` of `{}` both map to `{ident}` in {language} — add an `x-{}-name` override",
                        field.json_name,
                        symbol.name.0,
                        language.as_str()
                    ));
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Small helpers.
// ---------------------------------------------------------------------------

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| normalize(path))
}

/// Lexically normalize a path (collapse `.`/`..`) without touching the fs.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn root_type_name(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Root".to_string())
}

fn pascal(name: &str) -> String {
    use heck::ToPascalCase;
    name.to_pascal_case()
}

fn name_overrides(schema: &Schema) -> NameOverrides {
    NameOverrides {
        go: schema.name_override("x-go-name").map(str::to_string),
        java: schema.name_override("x-java-name").map(str::to_string),
        python: schema.name_override("x-python-name").map(str::to_string),
        typescript: schema.name_override("x-ts-name").map(str::to_string),
    }
}

/// Convert a JSON value into a scalar, or `None` for composite/null values.
fn to_scalar(value: &Value) -> Option<Scalar> {
    match value {
        Value::String(s) => Some(Scalar::String(s.clone())),
        Value::Bool(b) => Some(Scalar::Boolean(*b)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(Scalar::Integer(i))
            } else if let Some(f) = n.as_f64() {
                // Integer-valued float (1.0) is an integer; otherwise a number.
                if f.fract() == 0.0 && f.abs() < (1i64 << 53) as f64 {
                    Some(Scalar::Integer(f as i64))
                } else {
                    Some(Scalar::Number(f))
                }
            } else {
                None
            }
        }
        _ => None,
    }
}
