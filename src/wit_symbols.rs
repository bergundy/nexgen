use std::collections::BTreeMap;

use heck::ToUpperCamelCase;
use indexmap::IndexMap;
use nex_gen_codegen::{Name, Symbol, SymbolId, SymbolTable};
use prost_types::FieldDescriptorProto;
use prost_types::FileOptions;
use prost_types::field_descriptor_proto::{Label, Type};

use crate::descriptors::{DescriptorIndex, EnumMetadata, MessageMetadata};
use crate::error::{Error, Result};
use crate::generator::ModelCapabilities;
use crate::resources::{
    RequestPlan, ResolvedResourceBindingSource, ResolvedResourceMethodBinding,
    ResolvedResourceReturnSpec, ResolvedResourceSpec, resolve_service_resources,
};
use crate::spec::{
    ApiSpec, AuthoredFieldTypeSpec, FunctionFieldSpec, GeneratedModelSpec, LanguageStringSpec,
    OperationOutputTransformSpec, OperationSpec, ResourceFieldSpec, ServiceSpec,
    TypeReplacementSpec, WitEnumSpec, WitFlagsSpec, WitRecordSpec, WitVariantSpec,
};

#[derive(Debug, Clone, Default)]
struct WitTables {
    pub(crate) services: Vec<WitService>,
    pub(crate) enums: IndexMap<String, WitEnum>,
    pub(crate) flags: IndexMap<String, WitFlags>,
    pub(crate) variants: IndexMap<String, WitVariant>,
    pub(crate) models: IndexMap<String, WitModel>,
}

#[derive(Debug, Clone)]
pub(crate) struct WitService {
    pub(crate) name: String,
    pub(crate) wire_name: String,
    pub(crate) namespace: LanguageStringSpec,
    pub(crate) operations_class: LanguageStringSpec,
    pub(crate) endpoint: String,
    pub(crate) experimental: bool,
    pub(crate) delay_load_temporalio_workflow: bool,
    pub(crate) operations: Vec<WitOperation>,
    pub(crate) resources: Vec<WitResource>,
}

#[derive(Debug, Clone)]
pub(crate) struct WitOperation {
    pub(crate) name: String,
    pub(crate) wire_name: String,
    pub(crate) experimental: bool,
    pub(crate) doc: LanguageStringSpec,
    pub(crate) return_doc: LanguageStringSpec,
    pub(crate) input: WitMessageType,
    pub(crate) output: WitOperationOutput,
    pub(crate) output_transform: Option<OperationOutputTransformSpec>,
    pub(crate) output_resource_return: Option<WitOperationResourceReturn>,
    pub(crate) output_direct_result: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum WitOperationOutput {
    Message(WitMessageType),
    Resource { type_name: String },
    None,
}

#[derive(Debug, Clone)]
pub(crate) struct WitOperationResourceReturn {
    pub(crate) resource_type_name: String,
    pub(crate) bindings: Vec<WitOperationResourceFieldBinding>,
}

#[derive(Debug, Clone)]
pub(crate) struct WitOperationResourceFieldBinding {
    pub(crate) field_name: String,
    pub(crate) optional: bool,
    pub(crate) source: ResolvedResourceBindingSource,
}

#[derive(Debug, Clone)]
pub(crate) struct WitResource {
    pub(crate) name: String,
    pub(crate) type_name: String,
    pub(crate) fields: Vec<WitResourceField>,
    pub(crate) methods: Vec<WitResourceMethod>,
}

#[derive(Debug, Clone)]
pub(crate) struct WitResourceField {
    pub(crate) name: String,
    pub(crate) optional: bool,
    pub(crate) kind: WitFieldKind,
    pub(crate) function: Option<FunctionFieldSpec>,
}

#[derive(Debug, Clone)]
pub(crate) struct WitResourceMethod {
    pub(crate) name: String,
    pub(crate) params: Vec<WitResourceField>,
    pub(crate) result: Option<WitResourceMethodResult>,
    pub(crate) binding: WitResourceMethodBindingSpec,
}

#[derive(Debug, Clone)]
pub(crate) struct WitResourceMethodResult {
    pub(crate) kind: WitResourceMethodResultKind,
    pub(crate) optional: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum WitResourceMethodResultKind {
    Resource { type_name: String },
    Value(WitFieldKind),
}

#[derive(Debug, Clone)]
pub(crate) enum WitResourceMethodBindingSpec {
    Operation {
        operation_name: String,
        request_plan: RequestPlan,
        direct_return: bool,
    },
    Stub,
}

#[derive(Debug, Clone)]
pub(crate) struct WitTypeInfo {
    pub(crate) full_name: String,
    pub(crate) package: String,
    pub(crate) file_name: Option<String>,
    pub(crate) file_options: Option<FileOptions>,
    pub(crate) proto_reference: crate::spec::LanguageStringSpec,
    pub(crate) proto_type_name: crate::spec::LanguageStringSpec,
}

impl WitTypeInfo {
    fn from_message(message: &MessageMetadata, spec: &ApiSpec) -> Self {
        Self {
            full_name: message.full_name.clone(),
            package: message.package.clone(),
            file_name: message.file_name.clone(),
            file_options: message.file_options.clone(),
            proto_reference: spec
                .type_override(&message.full_name)
                .map(|type_override| type_override.proto_reference().clone())
                .unwrap_or_default(),
            proto_type_name: spec
                .type_override(&message.full_name)
                .map(|type_override| type_override.proto_type_name().clone())
                .unwrap_or_default(),
        }
    }

    fn from_enum(enumeration: &EnumMetadata, spec: &ApiSpec) -> Self {
        Self {
            full_name: enumeration.full_name.clone(),
            package: enumeration.package.clone(),
            file_name: enumeration.file_name.clone(),
            file_options: enumeration.file_options.clone(),
            proto_reference: spec
                .type_override(&enumeration.full_name)
                .map(|type_override| type_override.proto_reference().clone())
                .unwrap_or_default(),
            proto_type_name: spec
                .type_override(&enumeration.full_name)
                .map(|type_override| type_override.proto_type_name().clone())
                .unwrap_or_default(),
        }
    }

    fn from_wit_enum(enumeration: &WitEnumSpec) -> Self {
        Self {
            full_name: enumeration.full_name.clone(),
            package: String::new(),
            file_name: None,
            file_options: None,
            proto_reference: crate::spec::LanguageStringSpec::default(),
            proto_type_name: crate::spec::LanguageStringSpec::default(),
        }
    }

    fn from_wit_flags(flags: &WitFlagsSpec) -> Self {
        Self {
            full_name: flags.full_name.clone(),
            package: String::new(),
            file_name: None,
            file_options: None,
            proto_reference: crate::spec::LanguageStringSpec::default(),
            proto_type_name: crate::spec::LanguageStringSpec::default(),
        }
    }

    fn from_wit_record(record: &WitRecordSpec) -> Self {
        Self {
            full_name: record.full_name.clone(),
            package: String::new(),
            file_name: None,
            file_options: None,
            proto_reference: crate::spec::LanguageStringSpec::default(),
            proto_type_name: crate::spec::LanguageStringSpec::default(),
        }
    }

    fn from_wit_variant(variant: &WitVariantSpec) -> Self {
        Self {
            full_name: variant.full_name.clone(),
            package: String::new(),
            file_name: None,
            file_options: None,
            proto_reference: crate::spec::LanguageStringSpec::default(),
            proto_type_name: crate::spec::LanguageStringSpec::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct WitEnum {
    pub(crate) info: WitTypeInfo,
    pub(crate) name: String,
    pub(crate) values: Vec<WitEnumValue>,
}

#[derive(Debug, Clone)]
pub(crate) struct WitEnumValue {
    pub(crate) name: String,
    pub(crate) number: i32,
}

#[derive(Debug, Clone)]
pub(crate) struct WitFlags {
    pub(crate) info: WitTypeInfo,
    pub(crate) name: String,
    pub(crate) flags: Vec<WitFlag>,
}

#[derive(Debug, Clone)]
pub(crate) struct WitFlag {
    pub(crate) name: String,
    pub(crate) bit: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct WitVariant {
    pub(crate) info: WitTypeInfo,
    pub(crate) name: String,
    pub(crate) cases: Vec<WitVariantCase>,
}

#[derive(Debug, Clone)]
pub(crate) struct WitVariantCase {
    pub(crate) name: String,
    pub(crate) payload: Option<WitValueType>,
}

#[derive(Debug, Clone)]
pub(crate) struct WitModel {
    pub(crate) info: WitTypeInfo,
    pub(crate) name: String,
    pub(crate) capabilities: ModelCapabilities,
    pub(crate) flatten_in_api: bool,
    pub(crate) experimental: bool,
    pub(crate) generated_model: GeneratedModelSpec,
    pub(crate) fields: Vec<WitField>,
    pub(crate) sourced_fields: Vec<WitSourcedField>,
}

#[derive(Debug, Clone)]
pub(crate) struct WitField {
    pub(crate) owner_name: String,
    pub(crate) proto_name: String,
    pub(crate) authored_name: String,
    pub(crate) doc: Option<LanguageStringSpec>,
    pub(crate) annotation_override: Option<crate::spec::LanguageStringSpec>,
    pub(crate) default_value: Option<WitFieldDefault>,
    pub(crate) required: bool,
    pub(crate) has_presence: bool,
    pub(crate) role: WitFieldRole,
    pub(crate) function: Option<FunctionFieldSpec>,
    pub(crate) function_args: bool,
    pub(crate) kind: WitFieldKind,
}

#[derive(Debug, Clone)]
pub(crate) struct WitFieldDefault {
    pub(crate) enum_case: String,
}

#[derive(Debug, Clone)]
pub(crate) struct WitSourcedField {
    pub(crate) proto_name: String,
    pub(crate) source_expr: String,
    pub(crate) kind: WitFieldKind,
}

#[derive(Debug, Clone)]
pub(crate) enum WitFieldRole {
    Plain,
    Function(FunctionFieldSpec),
    FunctionArgs(FunctionFieldSpec),
}

#[derive(Debug, Clone)]
pub(crate) enum WitFieldKind {
    Singular(WitValueType),
    Repeated(WitValueType),
    Map {
        key: WitValueType,
        value: WitValueType,
    },
}

#[derive(Debug, Clone)]
pub(crate) enum WitValueType {
    Scalar(WitScalarType),
    Enum(WitEnumType),
    Flags(WitFlagsType),
    Variant(WitVariantType),
    Message(WitMessageType),
    Tuple(Vec<WitValueType>),
    Result {
        ok: Option<Box<WitValueType>>,
        err: Option<Box<WitValueType>>,
    },
    External {
        type_name: crate::spec::LanguageStringSpec,
        fallback: Box<WitValueType>,
    },
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WitScalarType {
    Float,
    Int32,
    Int64,
    Bool,
    String,
    Bytes,
}

#[derive(Debug, Clone)]
pub(crate) struct WitEnumType {
    pub(crate) info: Option<WitTypeInfo>,
    pub(crate) name: Option<String>,
    pub(crate) replacement: Option<TypeReplacementSpec>,
}

#[derive(Debug, Clone)]
pub(crate) struct WitFlagsType {
    pub(crate) info: WitTypeInfo,
    pub(crate) name: String,
}

#[derive(Debug, Clone)]
pub(crate) struct WitVariantType {
    pub(crate) info: WitTypeInfo,
    pub(crate) name: String,
}

#[derive(Debug, Clone)]
pub(crate) struct WitMessageType {
    pub(crate) info: WitTypeInfo,
    pub(crate) model_name: String,
    pub(crate) replacement: Option<TypeReplacementSpec>,
    pub(crate) authored_type: Option<AuthoredFieldTypeSpec>,
    pub(crate) source: WitMessageSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WitMessageSource {
    Proto,
    Wit,
}

pub(crate) fn message_model_name(full_name: &str) -> String {
    full_name
        .rsplit('.')
        .next()
        .expect("descriptor names should not be empty")
        .to_upper_camel_case()
        .to_string()
}

fn wit_proto_model_name(message: &MessageMetadata, spec: &ApiSpec) -> String {
    spec.type_override(&message.full_name)
        .and_then(|type_override| type_override.model_name())
        .map(str::to_string)
        .unwrap_or_else(|| message_model_name(&message.full_name))
}

pub(crate) fn enum_name(full_name: &str) -> String {
    full_name
        .rsplit('.')
        .next()
        .expect("descriptor names should not be empty")
        .to_string()
}

pub(crate) fn relative_descriptor_name(full_name: &str, package: &str) -> String {
    if package.is_empty() {
        full_name.to_string()
    } else {
        full_name
            .strip_prefix(&format!("{package}."))
            .unwrap_or(full_name)
            .to_string()
    }
}

pub(crate) fn field_has_presence(field: &FieldDescriptorProto, field_type: Option<Type>) -> bool {
    matches!(field_type, Some(Type::Message))
        || field.proto3_optional.unwrap_or(false)
        || field.oneof_index.is_some()
}

pub(crate) fn field_label(field: &FieldDescriptorProto) -> Option<Label> {
    field.label.and_then(|label| Label::try_from(label).ok())
}

pub(crate) fn field_type(field: &FieldDescriptorProto) -> Option<Type> {
    field
        .r#type
        .and_then(|field_type| Type::try_from(field_type).ok())
}

/// Lower a validated [`ApiSpec`] (+ proto descriptors) into the WIT symbol
/// table — the WIT front-end's IR. Internally it groups the lowered `Wit*`
/// items into a transient [`WitTables`] (private to this module: it never
/// escapes as an API), then explodes them into a [`SymbolTable`] via
/// [`tables_to_symbols`]. There is no `WitSymbols`: the symbol table is the IR.
pub(crate) fn build_wit_symbols(
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
) -> Result<SymbolTable<WitSymbolKind>> {
    let mut tables = WitTables::default();
    let root_model_capabilities = root_model_capabilities(spec, descriptors)?;

    for service in &spec.services {
        let wit_service = build_service(
            service,
            spec,
            descriptors,
            &root_model_capabilities,
            &mut tables,
        )?;
        tables.services.push(wit_service);
    }

    Ok(tables_to_symbols(tables))
}

fn build_message_type(
    message: &MessageMetadata,
    requested_capabilities: ModelCapabilities,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    tables: &mut WitTables,
) -> WitMessageType {
    let wit_message = wit_message_reference(message, spec);
    if wit_message.replacement.is_none() && wit_message.authored_type.is_none() {
        ensure_model(message, requested_capabilities, spec, descriptors, tables);
    }
    wit_message
}

fn root_model_capabilities(
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
) -> Result<BTreeMap<String, ModelCapabilities>> {
    let mut capabilities: BTreeMap<String, ModelCapabilities> = BTreeMap::new();

    for service in &spec.services {
        for operation in &service.operations {
            let Some(input_proto) = operation.input_proto() else {
                continue;
            };
            let input_message = descriptors.message(input_proto).ok_or_else(|| {
                Error::UnknownOperationInputProto {
                    service: service.name.clone(),
                    operation: operation.name.clone(),
                    type_name: input_proto.to_string(),
                }
            })?;
            capabilities
                .entry(input_message.full_name.clone())
                .or_default()
                .merge(ModelCapabilities::TO_PROTO_ONLY);

            if operation.output_transform().is_some() || operation.output_resource().is_some() {
                continue;
            }

            let Some(output_proto) = operation.output_proto() else {
                continue;
            };
            let output_message = descriptors.message(output_proto).ok_or_else(|| {
                Error::UnknownOperationOutputProto {
                    service: service.name.clone(),
                    operation: operation.name.clone(),
                    type_name: output_proto.to_string(),
                }
            })?;
            capabilities
                .entry(output_message.full_name.clone())
                .or_default()
                .merge(ModelCapabilities::BIDIRECTIONAL);
        }
    }

    Ok(capabilities)
}

fn build_service(
    service: &ServiceSpec,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    root_model_capabilities: &BTreeMap<String, ModelCapabilities>,
    tables: &mut WitTables,
) -> Result<WitService> {
    let endpoint = service
        .endpoint
        .as_deref()
        .ok_or_else(|| Error::MissingServiceEndpoint {
            service: service.name.clone(),
        })?
        .to_string();

    let resolved_resources = resolve_service_resources(spec, service, descriptors)?;
    let operations = service
        .operations
        .iter()
        .map(|operation| {
            build_operation(
                &service.name,
                operation,
                spec,
                descriptors,
                root_model_capabilities,
                tables,
                resolved_resources.operation_returns.get(&operation.name),
            )
        })
        .collect::<Result<Vec<_>>>()?;

    let operation_bindings = operations
        .iter()
        .map(|operation| OperationBindingInfo {
            name: &operation.name,
            direct_return: operation.output_transform.is_some()
                || operation.output_resource_return.is_some()
                || operation.output_direct_result,
        })
        .collect::<Vec<_>>();
    let resources = resolved_resources
        .resources
        .iter()
        .map(|resource| {
            build_resource(
                service,
                resource,
                &operation_bindings,
                spec,
                descriptors,
                tables,
            )
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(WitService {
        name: service.name.clone(),
        wire_name: service.wire_name.clone(),
        namespace: service.namespace.clone(),
        operations_class: service.operations_class.clone(),
        endpoint,
        experimental: service.experimental,
        delay_load_temporalio_workflow: service.delay_load_temporalio_workflow,
        operations,
        resources,
    })
}

fn build_operation(
    service_name: &str,
    operation: &OperationSpec,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    root_model_capabilities: &BTreeMap<String, ModelCapabilities>,
    tables: &mut WitTables,
    output_resource_return: Option<&ResolvedResourceReturnSpec>,
) -> Result<WitOperation> {
    let input = build_operation_input(
        service_name,
        operation,
        spec,
        descriptors,
        root_model_capabilities,
        tables,
    )?;
    let output = build_operation_output(
        service_name,
        operation,
        spec,
        descriptors,
        root_model_capabilities,
        tables,
        output_resource_return,
    )?;

    Ok(WitOperation {
        name: operation.name.clone(),
        wire_name: operation.wire_name.clone(),
        experimental: operation.experimental,
        doc: operation.doc.clone(),
        return_doc: operation.return_doc.clone(),
        input,
        output,
        output_transform: operation.output_transform().cloned(),
        output_resource_return: build_operation_resource_return(output_resource_return),
        output_direct_result: operation.output_proto().is_none()
            && operation.output_record().is_none()
            && operation.output_resource().is_some(),
    })
}

fn build_operation_input(
    service_name: &str,
    operation: &OperationSpec,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    root_model_capabilities: &BTreeMap<String, ModelCapabilities>,
    tables: &mut WitTables,
) -> Result<WitMessageType> {
    if let Some(input_proto) = operation.input_proto() {
        let input_message =
            descriptors
                .message(input_proto)
                .ok_or_else(|| Error::UnknownOperationInputProto {
                    service: service_name.to_string(),
                    operation: operation.name.clone(),
                    type_name: input_proto.to_string(),
                })?;

        return Ok(build_message_type(
            input_message,
            root_model_capabilities
                .get(&input_message.full_name)
                .copied()
                .unwrap_or(ModelCapabilities::TO_PROTO_ONLY),
            spec,
            descriptors,
            tables,
        ));
    }

    let record_name = operation.input_record().ok_or_else(|| Error::InvalidWit {
        path: std::path::PathBuf::from("<api-tables>"),
        reason: format!(
            "operation `{}` has no proto-backed or WIT-native input type",
            operation.name
        ),
    })?;
    let record = spec
        .records
        .get(record_name)
        .ok_or_else(|| Error::InvalidWit {
            path: std::path::PathBuf::from("<api-tables>"),
            reason: format!(
                "operation `{}` references unknown WIT record `{record_name}`",
                operation.name
            ),
        })?;
    Ok(build_wit_record_type(
        record,
        ModelCapabilities::default(),
        spec,
        descriptors,
        tables,
    ))
}

fn build_operation_output(
    service_name: &str,
    operation: &OperationSpec,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    root_model_capabilities: &BTreeMap<String, ModelCapabilities>,
    tables: &mut WitTables,
    output_resource_return: Option<&ResolvedResourceReturnSpec>,
) -> Result<WitOperationOutput> {
    if let Some(output_proto) = operation.output_proto() {
        let output_message = descriptors.message(output_proto).ok_or_else(|| {
            Error::UnknownOperationOutputProto {
                service: service_name.to_string(),
                operation: operation.name.clone(),
                type_name: output_proto.to_string(),
            }
        })?;
        let output = wit_message_reference(output_message, spec);
        if operation.output_transform().is_none() && output_resource_return.is_none() {
            let _ = build_message_type(
                output_message,
                root_model_capabilities
                    .get(&output_message.full_name)
                    .copied()
                    .unwrap_or(ModelCapabilities::BIDIRECTIONAL),
                spec,
                descriptors,
                tables,
            );
        }
        return Ok(WitOperationOutput::Message(output));
    }

    if let Some(record_name) = operation.output_record() {
        let record = spec
            .records
            .get(record_name)
            .ok_or_else(|| Error::InvalidWit {
                path: std::path::PathBuf::from("<api-tables>"),
                reason: format!(
                    "operation `{}` references unknown WIT record `{record_name}`",
                    operation.name
                ),
            })?;
        return Ok(WitOperationOutput::Message(build_wit_record_type(
            record,
            ModelCapabilities::default(),
            spec,
            descriptors,
            tables,
        )));
    }

    if let Some(resource_name) = operation.output_resource() {
        return Ok(WitOperationOutput::Resource {
            type_name: resource_name.to_upper_camel_case(),
        });
    }

    Ok(WitOperationOutput::None)
}

fn build_operation_resource_return(
    output_resource_return: Option<&ResolvedResourceReturnSpec>,
) -> Option<WitOperationResourceReturn> {
    output_resource_return.map(|resource_return| WitOperationResourceReturn {
        resource_type_name: resource_return.resource_name.to_upper_camel_case(),
        bindings: resource_return
            .bindings
            .iter()
            .map(|binding| WitOperationResourceFieldBinding {
                field_name: binding.field_name.clone(),
                optional: binding.optional,
                source: binding.source.clone(),
            })
            .collect(),
    })
}

#[derive(Debug, Clone, Copy)]
struct OperationBindingInfo<'a> {
    name: &'a str,
    direct_return: bool,
}

fn build_resource(
    service: &ServiceSpec,
    resource: &ResolvedResourceSpec,
    operations: &[OperationBindingInfo<'_>],
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    tables: &mut WitTables,
) -> Result<WitResource> {
    let methods = resource
        .methods
        .iter()
        .map(|method| {
            let binding = match &method.binding {
                ResolvedResourceMethodBinding::Operation {
                    operation_name,
                    request_plan,
                } => {
                    let operation = operations
                        .iter()
                        .find(|operation| operation.name == operation_name)
                        .ok_or_else(|| Error::InvalidResourceMethod {
                            service: service.name.clone(),
                            resource: resource.name.to_upper_camel_case(),
                            method: method.name.to_string(),
                            reason: format!("bound operation `{operation_name}` was not rendered"),
                        })?;
                    WitResourceMethodBindingSpec::Operation {
                        operation_name: operation.name.to_string(),
                        request_plan: request_plan.clone(),
                        direct_return: operation.direct_return,
                    }
                }
                ResolvedResourceMethodBinding::Stub => WitResourceMethodBindingSpec::Stub,
            };

            Ok(WitResourceMethod {
                name: method.name.clone(),
                params: method
                    .params
                    .iter()
                    .map(|field| wit_resource_field(field, spec, descriptors, tables))
                    .collect(),
                result: method
                    .result
                    .as_ref()
                    .map(|result| wit_resource_method_result(result, spec, descriptors, tables)),
                binding,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(WitResource {
        name: resource.name.clone(),
        type_name: resource.name.to_upper_camel_case(),
        fields: resource
            .fields
            .iter()
            .map(|field| wit_resource_field(field, spec, descriptors, tables))
            .collect(),
        methods,
    })
}

fn wit_resource_method_result(
    result: &crate::spec::ResourceResultSpec,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    tables: &mut WitTables,
) -> WitResourceMethodResult {
    let optional = matches!(result.result_type, AuthoredFieldTypeSpec::Option(_));
    let kind = if let Some(resource) = result.resource.as_ref() {
        WitResourceMethodResultKind::Resource {
            type_name: resource.to_upper_camel_case(),
        }
    } else {
        WitResourceMethodResultKind::Value(wit_field_kind_from_authored(
            &result.result_type,
            spec,
            descriptors,
            tables,
        ))
    };
    WitResourceMethodResult { kind, optional }
}

fn wit_resource_field(
    field: &ResourceFieldSpec,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    tables: &mut WitTables,
) -> WitResourceField {
    let kind = wit_field_kind_from_authored(&field.field_type, spec, descriptors, tables);
    WitResourceField {
        name: field.name.clone(),
        optional: field.optional,
        kind,
        function: field.function.clone(),
    }
}

fn wit_message_reference(message: &MessageMetadata, spec: &ApiSpec) -> WitMessageType {
    let type_override = spec.type_override(&message.full_name);
    WitMessageType {
        info: WitTypeInfo::from_message(message, spec),
        model_name: wit_proto_model_name(message, spec),
        replacement: type_override
            .and_then(|type_override| type_override.replacement())
            .cloned(),
        authored_type: type_override.and_then(|type_override| type_override.authored_type.clone()),
        source: WitMessageSource::Proto,
    }
}

fn build_wit_record_type(
    record: &WitRecordSpec,
    requested_capabilities: ModelCapabilities,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    tables: &mut WitTables,
) -> WitMessageType {
    let wit_message = WitMessageType {
        info: WitTypeInfo::from_wit_record(record),
        model_name: record.name.clone(),
        replacement: None,
        authored_type: None,
        source: WitMessageSource::Wit,
    };
    ensure_wit_model(record, requested_capabilities, spec, descriptors, tables);
    wit_message
}

fn ensure_wit_model(
    record: &WitRecordSpec,
    requested_capabilities: ModelCapabilities,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    tables: &mut WitTables,
) {
    if let Some(existing) = tables.models.get_mut(&record.full_name) {
        existing.capabilities.merge(requested_capabilities);
        return;
    }

    tables.models.insert(
        record.full_name.clone(),
        WitModel {
            info: WitTypeInfo::from_wit_record(record),
            name: record.name.clone(),
            capabilities: requested_capabilities,
            flatten_in_api: false,
            experimental: record.experimental,
            generated_model: record.generated_model.clone(),
            fields: Vec::new(),
            sourced_fields: Vec::new(),
        },
    );

    let fields = record
        .generated_model
        .declared_fields
        .iter()
        .filter(|field_name| {
            !record
                .generated_model
                .field_sources
                .contains_key(*field_name)
        })
        .map(|field_name| build_wit_field(record, field_name, spec, descriptors, tables))
        .collect();

    let model = tables
        .models
        .get_mut(&record.full_name)
        .expect("WIT model should be inserted before recursive field lowering");
    model.fields = fields;
}

fn ensure_model(
    message: &MessageMetadata,
    requested_capabilities: ModelCapabilities,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    tables: &mut WitTables,
) {
    if spec
        .type_override(&message.full_name)
        .and_then(|type_override| type_override.replacement())
        .is_some()
    {
        return;
    }

    if let Some(existing) = tables.models.get_mut(&message.full_name) {
        existing.capabilities.merge(requested_capabilities);
        return;
    }

    let type_override = spec.type_override(&message.full_name);
    let generated_model = type_override
        .and_then(|type_override| type_override.generated_model())
        .cloned()
        .unwrap_or_default();
    let flatten_in_api = type_override.is_some_and(|type_override| type_override.flatten_in_api());
    let experimental = type_override.is_some_and(|type_override| type_override.experimental());

    tables.models.insert(
        message.full_name.clone(),
        WitModel {
            info: WitTypeInfo::from_message(message, spec),
            name: wit_proto_model_name(message, spec),
            capabilities: requested_capabilities,
            flatten_in_api,
            experimental,
            generated_model: generated_model.clone(),
            fields: Vec::new(),
            sourced_fields: Vec::new(),
        },
    );

    let fields = wit_message_fields(message, &generated_model)
        .into_iter()
        .filter(|field| {
            let proto_name = field
                .name
                .as_deref()
                .expect("descriptor fields should be named");
            !spec
                .type_override(&message.full_name)
                .is_some_and(|type_override| type_override.is_field_hidden(proto_name))
        })
        .map(|field| build_field(message, field, spec, descriptors, tables))
        .collect();

    let sourced_fields = wit_message_fields(message, &generated_model)
        .into_iter()
        .filter_map(|field| {
            let proto_name = field
                .name
                .as_deref()
                .expect("descriptor fields should be named");
            spec.type_override(&message.full_name)
                .and_then(|type_override| type_override.field_source(proto_name))
                .map(|source_expr| {
                    build_sourced_field(message, field, source_expr, spec, descriptors, tables)
                })
        })
        .collect();

    let model = tables
        .models
        .get_mut(&message.full_name)
        .expect("model should be inserted before recursive field lowering");
    model.fields = fields;
    model.sourced_fields = sourced_fields;
}

fn wit_message_fields<'a>(
    message: &'a MessageMetadata,
    generated_model: &GeneratedModelSpec,
) -> Vec<&'a FieldDescriptorProto> {
    if generated_model.declared_fields.is_empty() {
        return message.descriptor.field.iter().collect();
    }

    generated_model
        .declared_fields
        .iter()
        .map(|field_name| descriptor_field_by_name(message, field_name))
        .collect()
}

fn descriptor_field_by_name<'a>(
    message: &'a MessageMetadata,
    field_name: &str,
) -> &'a FieldDescriptorProto {
    message
        .descriptor
        .field
        .iter()
        .find(|field| field.name.as_deref() == Some(field_name))
        .expect("declared generated model field should exist in descriptor")
}

fn ensure_enum(enumeration: &EnumMetadata, spec: &ApiSpec, tables: &mut WitTables) {
    tables.enums
        .entry(enumeration.full_name.clone())
        .or_insert_with(|| WitEnum {
            info: WitTypeInfo::from_enum(enumeration, spec),
            name: enum_name(&enumeration.full_name),
            values: enumeration
                .descriptor
                .value
                .iter()
                .filter_map(|value| {
                    Some(WitEnumValue {
                        name: value.name.as_deref()?.to_string(),
                        number: value.number?,
                    })
                })
                .collect(),
        });
}

fn ensure_wit_enum(enumeration: &WitEnumSpec, tables: &mut WitTables) {
    tables.enums
        .entry(enumeration.full_name.clone())
        .or_insert_with(|| WitEnum {
            info: WitTypeInfo::from_wit_enum(enumeration),
            name: enumeration.name.clone(),
            values: enumeration
                .values
                .iter()
                .map(|value| WitEnumValue {
                    name: value.name.clone(),
                    number: value.number,
                })
                .collect(),
        });
}

fn ensure_wit_flags(flags: &WitFlagsSpec, tables: &mut WitTables) {
    tables.flags
        .entry(flags.full_name.clone())
        .or_insert_with(|| WitFlags {
            info: WitTypeInfo::from_wit_flags(flags),
            name: flags.name.clone(),
            flags: flags
                .flags
                .iter()
                .map(|flag| WitFlag {
                    name: flag.name.clone(),
                    bit: flag.bit,
                })
                .collect(),
        });
}

fn ensure_wit_variant(
    variant: &WitVariantSpec,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    tables: &mut WitTables,
) {
    if tables.variants.contains_key(&variant.full_name) {
        return;
    }

    let cases = variant
        .cases
        .iter()
        .map(|case| WitVariantCase {
            name: case.name.clone(),
            payload: case
                .payload
                .as_ref()
                .map(|payload| wit_value_type_from_authored(payload, spec, descriptors, tables)),
        })
        .collect();
    tables.variants.insert(
        variant.full_name.clone(),
        WitVariant {
            info: WitTypeInfo::from_wit_variant(variant),
            name: variant.name.clone(),
            cases,
        },
    );
}

fn build_field(
    message: &MessageMetadata,
    field: &FieldDescriptorProto,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    tables: &mut WitTables,
) -> WitField {
    let proto_name = field
        .name
        .as_deref()
        .expect("descriptor fields should be named")
        .to_string();
    let generated_model = spec
        .type_override(&message.full_name)
        .and_then(|type_override| type_override.generated_model());

    WitField {
        owner_name: wit_proto_model_name(message, spec),
        authored_name: generated_model
            .and_then(|generated_model| generated_model.field_name_override(&proto_name))
            .unwrap_or(&proto_name)
            .to_string(),
        doc: generated_model
            .and_then(|generated_model| generated_model.field_doc(&proto_name))
            .cloned(),
        annotation_override: generated_model
            .and_then(|generated_model| generated_model.field_annotation(&proto_name))
            .cloned(),
        default_value: generated_model
            .and_then(|generated_model| generated_model.field_default(&proto_name))
            .map(|field_default| WitFieldDefault {
                enum_case: field_default.enum_case.clone(),
            }),
        required: spec
            .type_override(&message.full_name)
            .is_some_and(|type_override| type_override.is_field_required(&proto_name)),
        has_presence: field_has_presence(field, field_type(field)),
        role: wit_field_role(generated_model, &proto_name),
        function: generated_model
            .and_then(|generated_model| generated_model.function(&proto_name))
            .cloned(),
        function_args: generated_model
            .and_then(|generated_model| generated_model.function_for_args_field(&proto_name))
            .is_some(),
        kind: wit_field_kind(field, spec, descriptors, tables),
        proto_name,
    }
}

fn build_wit_field(
    record: &WitRecordSpec,
    field_name: &str,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    tables: &mut WitTables,
) -> WitField {
    let wit_type = record
        .generated_model
        .field_wit_type(field_name)
        .expect("declared WIT field should have a WIT type");
    WitField {
        owner_name: record.name.clone(),
        proto_name: field_name.to_string(),
        authored_name: record
            .generated_model
            .field_name_override(field_name)
            .unwrap_or(field_name)
            .to_string(),
        doc: record.generated_model.field_doc(field_name).cloned(),
        annotation_override: record.generated_model.field_annotation(field_name).cloned(),
        default_value: record
            .generated_model
            .field_default(field_name)
            .map(|field_default| WitFieldDefault {
                enum_case: field_default.enum_case.clone(),
            }),
        required: record.required_fields.contains(field_name),
        has_presence: !record.required_fields.contains(field_name),
        role: wit_field_role(Some(&record.generated_model), field_name),
        function: record.generated_model.function(field_name).cloned(),
        function_args: record
            .generated_model
            .function_for_args_field(field_name)
            .is_some(),
        kind: wit_field_kind_from_authored(wit_type, spec, descriptors, tables),
    }
}

fn wit_field_kind_from_authored(
    wit_type: &AuthoredFieldTypeSpec,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    tables: &mut WitTables,
) -> WitFieldKind {
    if let AuthoredFieldTypeSpec::Proto(proto_name) = wit_type {
        if let Some(type_override) = spec.type_override(proto_name) {
            if type_override.replacement.is_none() {
                if let Some(authored_type) = type_override.authored_type.as_ref() {
                    return wit_field_kind_from_authored(
                        authored_type,
                        spec,
                        descriptors,
                        tables,
                    );
                }
            }
        }
    }

    match wit_type {
        AuthoredFieldTypeSpec::Option(inner) => {
            wit_field_kind_from_authored(inner, spec, descriptors, tables)
        }
        AuthoredFieldTypeSpec::List(inner) => WitFieldKind::Repeated(
            wit_value_type_from_authored(inner.without_option(), spec, descriptors, tables),
        ),
        AuthoredFieldTypeSpec::Map(key, value) => WitFieldKind::Map {
            key: wit_value_type_from_authored(key.without_option(), spec, descriptors, tables),
            value: wit_value_type_from_authored(
                value.without_option(),
                spec,
                descriptors,
                tables,
            ),
        },
        _ => WitFieldKind::Singular(wit_value_type_from_authored(
            wit_type.without_option(),
            spec,
            descriptors,
            tables,
        )),
    }
}

fn wit_value_type_from_authored(
    wit_type: &AuthoredFieldTypeSpec,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    tables: &mut WitTables,
) -> WitValueType {
    match wit_type {
        AuthoredFieldTypeSpec::Bool => WitValueType::Scalar(WitScalarType::Bool),
        AuthoredFieldTypeSpec::Int => WitValueType::Scalar(WitScalarType::Int32),
        AuthoredFieldTypeSpec::Float => WitValueType::Scalar(WitScalarType::Float),
        AuthoredFieldTypeSpec::String => WitValueType::Scalar(WitScalarType::String),
        AuthoredFieldTypeSpec::Bytes => WitValueType::Scalar(WitScalarType::Bytes),
        AuthoredFieldTypeSpec::Proto(proto_name) => {
            if let Some(message) = descriptors.message(proto_name) {
                WitValueType::Message(build_message_type(
                    message,
                    ModelCapabilities::BIDIRECTIONAL,
                    spec,
                    descriptors,
                    tables,
                ))
            } else if let Some(enumeration) = descriptors.enumeration(proto_name) {
                let replacement = spec
                    .type_override(&enumeration.full_name)
                    .and_then(|type_override| type_override.replacement())
                    .cloned();
                if replacement.is_none() {
                    ensure_enum(enumeration, spec, tables);
                }
                WitValueType::Enum(WitEnumType {
                    info: Some(WitTypeInfo::from_enum(enumeration, spec)),
                    name: Some(enum_name(&enumeration.full_name)),
                    replacement,
                })
            } else {
                WitValueType::Unknown
            }
        }
        AuthoredFieldTypeSpec::Enum(enum_name) => spec
            .enums
            .get(enum_name)
            .map(|enumeration| {
                ensure_wit_enum(enumeration, tables);
                WitValueType::Enum(WitEnumType {
                    info: Some(WitTypeInfo::from_wit_enum(enumeration)),
                    name: Some(enumeration.name.clone()),
                    replacement: None,
                })
            })
            .unwrap_or(WitValueType::Unknown),
        AuthoredFieldTypeSpec::Flags(flags_name) => spec
            .flags
            .get(flags_name)
            .map(|flags| {
                ensure_wit_flags(flags, tables);
                WitValueType::Flags(WitFlagsType {
                    info: WitTypeInfo::from_wit_flags(flags),
                    name: flags.name.clone(),
                })
            })
            .unwrap_or(WitValueType::Unknown),
        AuthoredFieldTypeSpec::Variant(variant_name) => spec
            .variants
            .get(variant_name)
            .map(|variant| {
                ensure_wit_variant(variant, spec, descriptors, tables);
                WitValueType::Variant(WitVariantType {
                    info: WitTypeInfo::from_wit_variant(variant),
                    name: variant.name.clone(),
                })
            })
            .unwrap_or(WitValueType::Unknown),
        AuthoredFieldTypeSpec::Record(record_name) => spec
            .records
            .get(record_name)
            .map(|record| {
                WitValueType::Message(build_wit_record_type(
                    record,
                    ModelCapabilities::default(),
                    spec,
                    descriptors,
                    tables,
                ))
            })
            .unwrap_or(WitValueType::Unknown),
        AuthoredFieldTypeSpec::Resource(_) => WitValueType::Unknown,
        AuthoredFieldTypeSpec::Option(inner) => {
            wit_value_type_from_authored(inner.without_option(), spec, descriptors, tables)
        }
        AuthoredFieldTypeSpec::Tuple(items) => WitValueType::Tuple(
            items
                .iter()
                .map(|item| wit_value_type_from_authored(item, spec, descriptors, tables))
                .collect(),
        ),
        AuthoredFieldTypeSpec::Result { ok, err } => WitValueType::Result {
            ok: ok.as_ref().map(|ok| {
                Box::new(wit_value_type_from_authored(
                    ok,
                    spec,
                    descriptors,
                    tables,
                ))
            }),
            err: err.as_ref().map(|err| {
                Box::new(wit_value_type_from_authored(
                    err,
                    spec,
                    descriptors,
                    tables,
                ))
            }),
        },
        AuthoredFieldTypeSpec::Alias {
            target, type_name, ..
        } => {
            let fallback =
                wit_value_type_from_authored(target.without_option(), spec, descriptors, tables);
            WitValueType::External {
                type_name: type_name.clone(),
                fallback: Box::new(fallback),
            }
        }
        AuthoredFieldTypeSpec::List(_) | AuthoredFieldTypeSpec::Map(_, _) => {
            WitValueType::Unknown
        }
    }
}

fn build_sourced_field(
    _message: &MessageMetadata,
    field: &FieldDescriptorProto,
    source_expr: &str,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    tables: &mut WitTables,
) -> WitSourcedField {
    WitSourcedField {
        proto_name: field
            .name
            .as_deref()
            .expect("descriptor fields should be named")
            .to_string(),
        source_expr: source_expr.to_string(),
        kind: wit_field_kind(field, spec, descriptors, tables),
    }
}

fn wit_field_role(
    generated_model: Option<&GeneratedModelSpec>,
    proto_name: &str,
) -> WitFieldRole {
    if let Some(function) =
        generated_model.and_then(|generated_model| generated_model.function(proto_name))
    {
        return WitFieldRole::Function(function.clone());
    }
    if let Some(function) = generated_model
        .and_then(|generated_model| generated_model.function_for_args_field(proto_name))
    {
        return WitFieldRole::FunctionArgs(function.clone());
    }
    WitFieldRole::Plain
}

fn wit_field_kind(
    field: &FieldDescriptorProto,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    tables: &mut WitTables,
) -> WitFieldKind {
    if let Some((key, value)) = map_field_value_types(field, spec, descriptors, tables) {
        return WitFieldKind::Map { key, value };
    }

    let value = wit_value_type(field, spec, descriptors, tables);
    if field_label(field) == Some(Label::Repeated) {
        WitFieldKind::Repeated(value)
    } else {
        WitFieldKind::Singular(value)
    }
}

fn map_field_value_types(
    field: &FieldDescriptorProto,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    tables: &mut WitTables,
) -> Option<(WitValueType, WitValueType)> {
    if field_label(field) != Some(Label::Repeated) || field_type(field) != Some(Type::Message) {
        return None;
    }

    let entry_name = field.type_name.as_deref()?.trim_start_matches('.');
    let entry = descriptors.message(entry_name)?;
    let is_map_entry = entry
        .descriptor
        .options
        .as_ref()
        .and_then(|options| options.map_entry)
        .unwrap_or(false);
    if !is_map_entry {
        return None;
    }

    let key_field = entry
        .descriptor
        .field
        .iter()
        .find(|field| field.name.as_deref() == Some("key"))?;
    let value_field = entry
        .descriptor
        .field
        .iter()
        .find(|field| field.name.as_deref() == Some("value"))?;

    Some((
        wit_value_type(key_field, spec, descriptors, tables),
        wit_value_type(value_field, spec, descriptors, tables),
    ))
}

fn wit_value_type(
    field: &FieldDescriptorProto,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    tables: &mut WitTables,
) -> WitValueType {
    match field_type(field) {
        Some(Type::Double | Type::Float) => WitValueType::Scalar(WitScalarType::Float),
        Some(Type::Int64 | Type::Uint64 | Type::Fixed64 | Type::Sfixed64 | Type::Sint64) => {
            WitValueType::Scalar(WitScalarType::Int64)
        }
        Some(Type::Int32 | Type::Fixed32 | Type::Uint32 | Type::Sfixed32 | Type::Sint32) => {
            WitValueType::Scalar(WitScalarType::Int32)
        }
        Some(Type::Bool) => WitValueType::Scalar(WitScalarType::Bool),
        Some(Type::String) => WitValueType::Scalar(WitScalarType::String),
        Some(Type::Bytes) => WitValueType::Scalar(WitScalarType::Bytes),
        Some(Type::Enum) => WitValueType::Enum(build_enum_type(field, spec, descriptors, tables)),
        Some(Type::Message) | Some(Type::Group) => {
            if let Some(message) = field
                .type_name
                .as_deref()
                .and_then(|type_name| descriptors.message(type_name.trim_start_matches('.')))
            {
                WitValueType::Message(build_message_type(
                    message,
                    ModelCapabilities::BIDIRECTIONAL,
                    spec,
                    descriptors,
                    tables,
                ))
            } else {
                WitValueType::Unknown
            }
        }
        None => WitValueType::Unknown,
    }
}

fn build_enum_type(
    field: &FieldDescriptorProto,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    tables: &mut WitTables,
) -> WitEnumType {
    let Some(enumeration) = field
        .type_name
        .as_deref()
        .and_then(|type_name| descriptors.enumeration(type_name.trim_start_matches('.')))
    else {
        return WitEnumType {
            info: None,
            name: None,
            replacement: None,
        };
    };

    let replacement = spec
        .type_override(&enumeration.full_name)
        .and_then(|type_override| type_override.replacement())
        .cloned();
    if replacement.is_none() {
        ensure_enum(enumeration, spec, tables);
    }

    WitEnumType {
        info: Some(WitTypeInfo::from_enum(enumeration, spec)),
        name: Some(enum_name(&enumeration.full_name)),
        replacement,
    }
}

// ---------------------------------------------------------------------------
// WIT symbol table: the front-end IR.
//
// `WitSymbolKind` is the WIT front-end's open symbol-kind; each variant owns
// the corresponding lowered item so an emitter renders straight from the
// symbol. `tables_to_symbols` explodes the transient `WitTables` into a
// `SymbolTable<WitSymbolKind>`, resolving cross-type references to `SymbolId`s.
// `WitSymbols` is a borrowing, grouped *view* over that table, indexed the way
// the emitters query it — it holds no owned data and replaces the old owned
// `WitSymbols`.
// ---------------------------------------------------------------------------

/// The WIT front-end's open symbol-kind: one variant per lowered type/service.
///
/// Each variant owns the corresponding lowered item so an emitter renders
/// straight from the symbol without a private side table.
#[derive(Debug, Clone)]
pub(crate) enum WitSymbolKind {
    Service(WitService),
    Model(WitModel),
    Enum(WitEnum),
    Flags(WitFlags),
    Variant(WitVariant),
}

/// Explode the transient [`WitTables`] into a [`SymbolTable`], resolving
/// cross-type references to [`SymbolId`]s.
///
/// Runs in two passes so a symbol's `refs` can point at not-yet-inserted
/// symbols: pass 1 allocates ids and builds a `full_name -> SymbolId` index;
/// pass 2 computes each symbol's `refs` from that index and inserts it. Ids are
/// allocated services -> models -> enums -> flags -> variants, each group in its
/// source order, so [`WitSymbols`] can replay the original grouping order.
fn tables_to_symbols(tables: WitTables) -> SymbolTable<WitSymbolKind> {
    let WitTables {
        services,
        enums,
        flags,
        variants,
        models,
    } = tables;

    let mut table = SymbolTable::new();

    // Owned, id-tagged items in the fixed insertion order:
    // services, models, enums, flags, variants.
    let mut wit_services: Vec<(SymbolId, WitService)> = Vec::new();
    let mut wit_models: Vec<(SymbolId, WitModel)> = Vec::new();
    let mut wit_enums: Vec<(SymbolId, WitEnum)> = Vec::new();
    let mut wit_flags: Vec<(SymbolId, WitFlags)> = Vec::new();
    let mut wit_variants: Vec<(SymbolId, WitVariant)> = Vec::new();

    // Maps a type's `full_name` (the IndexMap key) to its allocated `SymbolId`.
    // Services are not referenced by full_name, so they are not indexed.
    let mut full_name_to_id: BTreeMap<String, SymbolId> = BTreeMap::new();

    // Pass 1: allocate + index (services, models, enums, flags, variants).
    for service in services {
        let id = table.alloc_id();
        wit_services.push((id, service));
    }
    for (full_name, model) in models {
        let id = table.alloc_id();
        full_name_to_id.insert(full_name, id);
        wit_models.push((id, model));
    }
    for (full_name, enumeration) in enums {
        let id = table.alloc_id();
        full_name_to_id.insert(full_name, id);
        wit_enums.push((id, enumeration));
    }
    for (full_name, flag_set) in flags {
        let id = table.alloc_id();
        full_name_to_id.insert(full_name, id);
        wit_flags.push((id, flag_set));
    }
    for (full_name, variant) in variants {
        let id = table.alloc_id();
        full_name_to_id.insert(full_name, id);
        wit_variants.push((id, variant));
    }

    // Pass 2: compute refs + insert.
    for (id, service) in wit_services {
        let refs = service_refs(&service, &full_name_to_id);
        let name = Name::new(&service.name);
        table.insert(Symbol {
            id,
            name,
            refs,
            kind: WitSymbolKind::Service(service),
        });
    }
    for (id, model) in wit_models {
        let refs = model_refs(&model, &full_name_to_id);
        let name = Name::new(&model.name);
        table.insert(Symbol {
            id,
            name,
            refs,
            kind: WitSymbolKind::Model(model),
        });
    }
    for (id, enumeration) in wit_enums {
        let name = Name::new(&enumeration.name);
        table.insert(Symbol {
            id,
            name,
            refs: Vec::new(),
            kind: WitSymbolKind::Enum(enumeration),
        });
    }
    for (id, flag_set) in wit_flags {
        let name = Name::new(&flag_set.name);
        table.insert(Symbol {
            id,
            name,
            refs: Vec::new(),
            kind: WitSymbolKind::Flags(flag_set),
        });
    }
    for (id, variant) in wit_variants {
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
/// unknown/proto types not in the table) are silently skipped.
fn push_full_name(map: &BTreeMap<String, SymbolId>, full_name: &str, out: &mut Vec<SymbolId>) {
    if let Some(id) = map.get(full_name) {
        push_unique(out, *id);
    }
}

fn value_type_refs(
    value_type: &WitValueType,
    map: &BTreeMap<String, SymbolId>,
    out: &mut Vec<SymbolId>,
) {
    match value_type {
        WitValueType::Message(message) => {
            push_full_name(map, &message.info.full_name, out);
        }
        WitValueType::Enum(enum_type) => {
            if let Some(info) = &enum_type.info {
                push_full_name(map, &info.full_name, out);
            }
        }
        WitValueType::Flags(flags_type) => {
            push_full_name(map, &flags_type.info.full_name, out);
        }
        WitValueType::Variant(variant_type) => {
            push_full_name(map, &variant_type.info.full_name, out);
        }
        WitValueType::Tuple(items) => {
            for item in items {
                value_type_refs(item, map, out);
            }
        }
        WitValueType::Result { ok, err } => {
            if let Some(ok) = ok {
                value_type_refs(ok, map, out);
            }
            if let Some(err) = err {
                value_type_refs(err, map, out);
            }
        }
        WitValueType::External { fallback, .. } => {
            value_type_refs(fallback, map, out);
        }
        WitValueType::Scalar(_) | WitValueType::Unknown => {}
    }
}

fn field_kind_refs(kind: &WitFieldKind, map: &BTreeMap<String, SymbolId>, out: &mut Vec<SymbolId>) {
    match kind {
        WitFieldKind::Singular(value_type) | WitFieldKind::Repeated(value_type) => {
            value_type_refs(value_type, map, out);
        }
        WitFieldKind::Map { key, value } => {
            value_type_refs(key, map, out);
            value_type_refs(value, map, out);
        }
    }
}

fn model_refs(model: &WitModel, map: &BTreeMap<String, SymbolId>) -> Vec<SymbolId> {
    let mut refs = Vec::new();
    for field in &model.fields {
        field_kind_refs(&field.kind, map, &mut refs);
    }
    for field in &model.sourced_fields {
        field_kind_refs(&field.kind, map, &mut refs);
    }
    refs
}

fn service_refs(service: &WitService, map: &BTreeMap<String, SymbolId>) -> Vec<SymbolId> {
    let mut refs = Vec::new();
    for operation in &service.operations {
        push_full_name(map, &operation.input.info.full_name, &mut refs);
        match &operation.output {
            WitOperationOutput::Message(message) => {
                push_full_name(map, &message.info.full_name, &mut refs);
            }
            // TODO: resource refs when resources become symbols
            WitOperationOutput::Resource { .. } | WitOperationOutput::None => {}
        }
    }
    refs
}

fn variant_refs(variant: &WitVariant, map: &BTreeMap<String, SymbolId>) -> Vec<SymbolId> {
    let mut refs = Vec::new();
    for case in &variant.cases {
        if let Some(payload) = &case.payload {
            value_type_refs(payload, map, &mut refs);
        }
    }
    refs
}

/// A borrowing, grouped view over a WIT [`SymbolTable`], indexed the way the
/// emitters query it: services in symbol-id (source) order, and types by
/// `full_name`. The data lives in the symbols — this is only a query layer,
/// built once per generation, that replaces the old owned `WitSymbols`.
pub(crate) struct WitSymbols<'a> {
    services: Vec<&'a WitService>,
    models: IndexMap<&'a str, &'a WitModel>,
    enums: IndexMap<&'a str, &'a WitEnum>,
    flags: IndexMap<&'a str, &'a WitFlags>,
    variants: IndexMap<&'a str, &'a WitVariant>,
}

impl<'a> WitSymbols<'a> {
    /// Build the grouped view from a symbol table. Iterating in symbol-id order
    /// replays the original grouping order (see [`tables_to_symbols`]).
    pub(crate) fn new(symbols: &'a SymbolTable<WitSymbolKind>) -> Self {
        let mut view = Self {
            services: Vec::new(),
            models: IndexMap::new(),
            enums: IndexMap::new(),
            flags: IndexMap::new(),
            variants: IndexMap::new(),
        };
        for symbol in symbols.iter() {
            match &symbol.kind {
                WitSymbolKind::Service(service) => view.services.push(service),
                WitSymbolKind::Model(model) => {
                    view.models.insert(model.info.full_name.as_str(), model);
                }
                WitSymbolKind::Enum(enumeration) => {
                    view.enums
                        .insert(enumeration.info.full_name.as_str(), enumeration);
                }
                WitSymbolKind::Flags(flag_set) => {
                    view.flags.insert(flag_set.info.full_name.as_str(), flag_set);
                }
                WitSymbolKind::Variant(variant) => {
                    view.variants
                        .insert(variant.info.full_name.as_str(), variant);
                }
            }
        }
        view
    }

    pub(crate) fn services(&self) -> impl Iterator<Item = &'a WitService> + '_ {
        self.services.iter().copied()
    }

    pub(crate) fn models(&self) -> impl Iterator<Item = &'a WitModel> + '_ {
        self.models.values().copied()
    }

    pub(crate) fn enums(&self) -> impl Iterator<Item = &'a WitEnum> + '_ {
        self.enums.values().copied()
    }

    pub(crate) fn flags(&self) -> impl Iterator<Item = &'a WitFlags> + '_ {
        self.flags.values().copied()
    }

    pub(crate) fn variants(&self) -> impl Iterator<Item = &'a WitVariant> + '_ {
        self.variants.values().copied()
    }

    pub(crate) fn model(&self, full_name: &str) -> Option<&'a WitModel> {
        self.models.get(full_name).copied()
    }

    pub(crate) fn contains_model(&self, full_name: &str) -> bool {
        self.models.contains_key(full_name)
    }

    pub(crate) fn enum_(&self, full_name: &str) -> Option<&'a WitEnum> {
        self.enums.get(full_name).copied()
    }

    pub(crate) fn flags_(&self, full_name: &str) -> Option<&'a WitFlags> {
        self.flags.get(full_name).copied()
    }

    pub(crate) fn variant_(&self, full_name: &str) -> Option<&'a WitVariant> {
        self.variants.get(full_name).copied()
    }
}
