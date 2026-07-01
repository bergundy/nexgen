//! WIT front-end loader lowering WIT inputs into the shared base IR.
//!
//! This is the WIT side of the `Loader` -> `IR` -> `Emitter` pipeline described
//! in `crates/nex-gen-codegen`. [`WitLoader`] reuses the existing WIT lowering
//! ([`build_api_plan`](crate::api_plan::build_api_plan)) to produce an
//! [`ApiPlan`], then **explodes** that plan into a
//! [`SymbolTable<WitSymbolKind>`] via [`plan_to_symbols`]. The old generation
//! path is untouched; this module only adds the symbol-table lowering.

use std::path::PathBuf;

use nex_gen_codegen::{IR, Language, Name, SchemaType, Symbol, SymbolId, SymbolTable};

use crate::api_plan::{
    ApiPlan, PlannedEnum, PlannedFieldKind, PlannedFlags, PlannedModel, PlannedOperationOutput,
    PlannedService, PlannedValueType, PlannedVariant, build_api_plan,
};
use crate::descriptors::DescriptorIndex;
use crate::resources::ensure_unique_resource_names;
use crate::spec::ApiSpec;
use crate::validation::validate_type_overrides;

/// The WIT front-end's open symbol-kind: one variant per planned type/service.
///
/// Each variant owns the corresponding `ApiPlan` item so the (future) WIT
/// emitter renders straight from the symbol without a private side table.
#[derive(Debug, Clone)]
pub(crate) enum WitSymbolKind {
    Service(PlannedService),
    Model(PlannedModel),
    Enum(PlannedEnum),
    Flags(PlannedFlags),
    Variant(PlannedVariant),
}

/// Loads WIT inputs (plus proto descriptors) into the base IR.
///
/// Holds its own inputs; `language` is supplied per [`Loader::load`] call
/// because WIT resolves language-specific overrides at parse time.
pub(crate) struct WitLoader {
    input_paths: Vec<PathBuf>,
    descriptor_paths: Vec<PathBuf>,
}

impl WitLoader {
    /// Construct a loader over the given WIT input and proto descriptor paths.
    pub(crate) fn new(input_paths: Vec<PathBuf>, descriptor_paths: Vec<PathBuf>) -> Self {
        Self {
            input_paths,
            descriptor_paths,
        }
    }
}

impl nex_gen_codegen::Loader for WitLoader {
    type Kind = WitSymbolKind;

    fn schema_type(&self) -> SchemaType {
        SchemaType::Wit
    }

    fn load(&self, language: Language) -> nex_gen_codegen::Result<IR<WitSymbolKind>> {
        let spec = ApiSpec::load_for_language_with_inputs(language, &self.input_paths)
            .map_err(|error| nex_gen_codegen::Error::Load {
                message: error.to_string(),
            })?;
        let descriptors = DescriptorIndex::load_many(&self.descriptor_paths).map_err(|error| {
            nex_gen_codegen::Error::Load {
                message: error.to_string(),
            }
        })?;
        validate_type_overrides(&spec, &descriptors, language).map_err(|error| {
            nex_gen_codegen::Error::Load {
                message: error.to_string(),
            }
        })?;
        ensure_unique_resource_names(&spec).map_err(|error| nex_gen_codegen::Error::Load {
            message: error.to_string(),
        })?;
        let plan = build_api_plan(&spec, &descriptors).map_err(|error| {
            nex_gen_codegen::Error::Load {
                message: error.to_string(),
            }
        })?;
        Ok(IR::new(plan_to_symbols(plan)))
    }
}

/// Explode an [`ApiPlan`] into a [`SymbolTable`], resolving cross-type
/// references to [`SymbolId`]s.
///
/// Runs in two passes so a symbol's `refs` can point at not-yet-inserted
/// symbols: pass 1 allocates ids and builds a `full_name -> SymbolId` index;
/// pass 2 computes each symbol's `refs` from that index and inserts it.
pub(crate) fn plan_to_symbols(plan: ApiPlan) -> SymbolTable<WitSymbolKind> {
    use std::collections::BTreeMap;

    let ApiPlan {
        services,
        enums,
        flags,
        variants,
        models,
    } = plan;

    let mut table = SymbolTable::new();

    // Owned, id-tagged items in the fixed insertion order:
    // services, models, enums, flags, variants.
    let mut planned_services: Vec<(SymbolId, PlannedService)> = Vec::new();
    let mut planned_models: Vec<(SymbolId, PlannedModel)> = Vec::new();
    let mut planned_enums: Vec<(SymbolId, PlannedEnum)> = Vec::new();
    let mut planned_flags: Vec<(SymbolId, PlannedFlags)> = Vec::new();
    let mut planned_variants: Vec<(SymbolId, PlannedVariant)> = Vec::new();

    // Maps a type's `full_name` (the IndexMap key) to its allocated `SymbolId`.
    // Services are not referenced by full_name, so they are not indexed.
    let mut full_name_to_id: BTreeMap<String, SymbolId> = BTreeMap::new();

    // Pass 1: allocate + index (services, models, enums, flags, variants).
    for service in services {
        let id = table.alloc_id();
        planned_services.push((id, service));
    }
    for (full_name, model) in models {
        let id = table.alloc_id();
        full_name_to_id.insert(full_name, id);
        planned_models.push((id, model));
    }
    for (full_name, enumeration) in enums {
        let id = table.alloc_id();
        full_name_to_id.insert(full_name, id);
        planned_enums.push((id, enumeration));
    }
    for (full_name, flag_set) in flags {
        let id = table.alloc_id();
        full_name_to_id.insert(full_name, id);
        planned_flags.push((id, flag_set));
    }
    for (full_name, variant) in variants {
        let id = table.alloc_id();
        full_name_to_id.insert(full_name, id);
        planned_variants.push((id, variant));
    }

    // Pass 2: compute refs + insert.
    for (id, service) in planned_services {
        let refs = service_refs(&service, &full_name_to_id);
        let name = Name::new(&service.name);
        table.insert(Symbol {
            id,
            name,
            refs,
            kind: WitSymbolKind::Service(service),
        });
    }
    for (id, model) in planned_models {
        let refs = model_refs(&model, &full_name_to_id);
        let name = Name::new(&model.name);
        table.insert(Symbol {
            id,
            name,
            refs,
            kind: WitSymbolKind::Model(model),
        });
    }
    for (id, enumeration) in planned_enums {
        let name = Name::new(&enumeration.name);
        table.insert(Symbol {
            id,
            name,
            refs: Vec::new(),
            kind: WitSymbolKind::Enum(enumeration),
        });
    }
    for (id, flag_set) in planned_flags {
        let name = Name::new(&flag_set.name);
        table.insert(Symbol {
            id,
            name,
            refs: Vec::new(),
            kind: WitSymbolKind::Flags(flag_set),
        });
    }
    for (id, variant) in planned_variants {
        let refs = variant_refs(&variant, &full_name_to_id);
        let name = Name::new(&variant.name);
        table.insert(Symbol {
            id,
            name,
            refs,
            kind: WitSymbolKind::Variant(variant),
        });
    }

    table
}

/// Push `id` onto `out` only if it is not already present (dedup preserving
/// first-seen order).
fn push_unique(out: &mut Vec<SymbolId>, id: SymbolId) {
    if !out.contains(&id) {
        out.push(id);
    }
}

/// Resolve `full_name` to a `SymbolId` (if present in `map`) and push it.
///
/// References whose `full_name` is not in the table (replacements, externals,
/// unknown/proto types not planned) are silently skipped.
fn push_full_name(
    map: &std::collections::BTreeMap<String, SymbolId>,
    full_name: &str,
    out: &mut Vec<SymbolId>,
) {
    if let Some(id) = map.get(full_name) {
        push_unique(out, *id);
    }
}

fn value_type_refs(
    value_type: &PlannedValueType,
    map: &std::collections::BTreeMap<String, SymbolId>,
    out: &mut Vec<SymbolId>,
) {
    match value_type {
        PlannedValueType::Message(message) => {
            push_full_name(map, &message.info.full_name, out);
        }
        PlannedValueType::Enum(enum_type) => {
            if let Some(info) = &enum_type.info {
                push_full_name(map, &info.full_name, out);
            }
        }
        PlannedValueType::Flags(flags_type) => {
            push_full_name(map, &flags_type.info.full_name, out);
        }
        PlannedValueType::Variant(variant_type) => {
            push_full_name(map, &variant_type.info.full_name, out);
        }
        PlannedValueType::Tuple(items) => {
            for item in items {
                value_type_refs(item, map, out);
            }
        }
        PlannedValueType::Result { ok, err } => {
            if let Some(ok) = ok {
                value_type_refs(ok, map, out);
            }
            if let Some(err) = err {
                value_type_refs(err, map, out);
            }
        }
        PlannedValueType::External { fallback, .. } => {
            value_type_refs(fallback, map, out);
        }
        PlannedValueType::Scalar(_) | PlannedValueType::Unknown => {}
    }
}

fn field_kind_refs(
    kind: &PlannedFieldKind,
    map: &std::collections::BTreeMap<String, SymbolId>,
    out: &mut Vec<SymbolId>,
) {
    match kind {
        PlannedFieldKind::Singular(value_type) | PlannedFieldKind::Repeated(value_type) => {
            value_type_refs(value_type, map, out);
        }
        PlannedFieldKind::Map { key, value } => {
            value_type_refs(key, map, out);
            value_type_refs(value, map, out);
        }
    }
}

fn model_refs(
    model: &PlannedModel,
    map: &std::collections::BTreeMap<String, SymbolId>,
) -> Vec<SymbolId> {
    let mut refs = Vec::new();
    for field in &model.fields {
        field_kind_refs(&field.kind, map, &mut refs);
    }
    for field in &model.sourced_fields {
        field_kind_refs(&field.kind, map, &mut refs);
    }
    refs
}

fn service_refs(
    service: &PlannedService,
    map: &std::collections::BTreeMap<String, SymbolId>,
) -> Vec<SymbolId> {
    let mut refs = Vec::new();
    for operation in &service.operations {
        push_full_name(map, &operation.input.info.full_name, &mut refs);
        match &operation.output {
            PlannedOperationOutput::Message(message) => {
                push_full_name(map, &message.info.full_name, &mut refs);
            }
            // TODO: resource refs when resources become symbols
            PlannedOperationOutput::Resource { .. } | PlannedOperationOutput::None => {}
        }
    }
    refs
}

fn variant_refs(
    variant: &PlannedVariant,
    map: &std::collections::BTreeMap<String, SymbolId>,
) -> Vec<SymbolId> {
    let mut refs = Vec::new();
    for case in &variant.cases {
        if let Some(payload) = &case.payload {
            value_type_refs(payload, map, &mut refs);
        }
    }
    refs
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use prost_types::FileDescriptorSet;

    use super::{WitSymbolKind, plan_to_symbols};
    use crate::api_plan::build_api_plan;
    use crate::descriptors::DescriptorIndex;
    use crate::Language;
    use crate::spec::ApiSpec;

    const INLINE_WIT: &str = r#"
package temporal:users@1.0.0;

world system {
  export user-service;
}

/// @nexus.endpoint "__user_service"
interface user-service {
  resource user {
    constructor(user-id: string, email: string);

    update-email: func(email: string) -> user-result;
  }

  type user-result = own<user>;

  record update-email-request {
    users-id: string,
    email: string,
  }

  update-email: func(request: update-email-request) -> user-result;
}
"#;

    fn build_symbols() -> nex_gen_codegen::SymbolTable<WitSymbolKind> {
        let spec =
            ApiSpec::parse_for_language(Language::Python, INLINE_WIT, PathBuf::from("inline.wit"))
                .unwrap();
        let descriptors =
            DescriptorIndex::from_descriptor_set(FileDescriptorSet { file: Vec::new() }).unwrap();
        let plan = build_api_plan(&spec, &descriptors).unwrap();
        plan_to_symbols(plan)
    }

    #[test]
    fn explodes_single_service_symbol() {
        let table = build_symbols();
        let services: Vec<&str> = table
            .iter()
            .filter_map(|symbol| match &symbol.kind {
                WitSymbolKind::Service(service) => Some(service.name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(services, vec!["UserService"]);
    }

    #[test]
    fn includes_model_symbols_by_name() {
        let table = build_symbols();
        let has_model = table.iter().any(|symbol| {
            matches!(&symbol.kind, WitSymbolKind::Model(model) if model.name == "UpdateEmailRequest")
        });
        assert!(
            has_model,
            "expected an UpdateEmailRequest model symbol to be present"
        );
    }

    #[test]
    fn service_refs_include_input_model() {
        let table = build_symbols();

        // Find the sole service symbol and the full_name of its first
        // operation's input model.
        let service_symbol = table
            .iter()
            .find(|symbol| matches!(&symbol.kind, WitSymbolKind::Service(_)))
            .expect("a service symbol should exist");
        let WitSymbolKind::Service(service) = &service_symbol.kind else {
            unreachable!()
        };
        let operation = service
            .operations
            .first()
            .expect("service should have at least one operation");
        let input_model_name = &operation.input.model_name;

        // Find the model symbol matching the operation input's model name.
        let input_model_symbol = table
            .iter()
            .find(|symbol| {
                matches!(&symbol.kind, WitSymbolKind::Model(model) if &model.name == input_model_name)
            })
            .expect("operation input model should be a symbol");

        assert!(
            service_symbol.refs.contains(&input_model_symbol.id),
            "service refs {:?} should include input model id {:?}",
            service_symbol.refs,
            input_model_symbol.id
        );
    }
}
