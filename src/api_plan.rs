use std::collections::BTreeMap;

use heck::ToUpperCamelCase;
use indexmap::IndexMap;
use prost_types::FieldDescriptorProto;
use prost_types::field_descriptor_proto::{Label, Type};

use crate::descriptors::{DescriptorIndex, EnumMetadata, MessageMetadata};
use crate::error::{Error, Result};
use crate::generator::ModelCapabilities;
use crate::resources::{
    RequestPlan, ResolvedResourceBindingSource, ResolvedResourceMethodBinding,
    ResolvedResourceReturnSpec, ResolvedResourceSpec, resolve_service_resources,
};
use crate::spec::{
    ApiSpec, FunctionFieldSpec, GeneratedModelSpec, OperationOutputTransformSpec, OperationSpec,
    ResourceFieldSpec, ServiceSpec, TypeReplacementSpec, WithArgumentsFieldSpec,
};

#[derive(Debug, Clone, Default)]
pub(crate) struct ApiPlan {
    pub(crate) services: Vec<PlannedService>,
    pub(crate) enums: IndexMap<String, PlannedEnum>,
    pub(crate) models: IndexMap<String, PlannedModel>,
}

#[derive(Debug, Clone)]
pub(crate) struct PlannedService {
    pub(crate) name: String,
    pub(crate) endpoint: String,
    pub(crate) operations: Vec<PlannedOperation>,
    pub(crate) resources: Vec<PlannedResource>,
}

#[derive(Debug, Clone)]
pub(crate) struct PlannedOperation {
    pub(crate) name: String,
    pub(crate) input: PlannedMessageType,
    pub(crate) output: PlannedMessageType,
    // NOTE: This is already the selected language's transform payload from the spec.
    pub(crate) output_transform: Option<OperationOutputTransformSpec>,
    pub(crate) output_resource_return: Option<PlannedOperationResourceReturn>,
}

#[derive(Debug, Clone)]
pub(crate) struct PlannedOperationResourceReturn {
    pub(crate) resource_type_name: String,
    pub(crate) bindings: Vec<PlannedOperationResourceFieldBinding>,
}

#[derive(Debug, Clone)]
pub(crate) struct PlannedOperationResourceFieldBinding {
    pub(crate) field_name: String,
    pub(crate) optional: bool,
    pub(crate) source: ResolvedResourceBindingSource,
}

#[derive(Debug, Clone)]
pub(crate) struct PlannedResource {
    pub(crate) name: String,
    pub(crate) type_name: String,
    pub(crate) fields: Vec<PlannedResourceField>,
    pub(crate) methods: Vec<PlannedResourceMethod>,
}

#[derive(Debug, Clone)]
pub(crate) struct PlannedResourceField {
    pub(crate) name: String,
    // NOTE: This is already the selected language's annotation string from the spec.
    pub(crate) annotation: String,
    pub(crate) optional: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct PlannedResourceMethod {
    pub(crate) name: String,
    pub(crate) params: Vec<PlannedResourceField>,
    // NOTE: When present, this is already the selected language's annotation string from the spec.
    pub(crate) result_annotation: Option<String>,
    pub(crate) binding: PlannedResourceMethodBindingSpec,
}

#[derive(Debug, Clone)]
pub(crate) enum PlannedResourceMethodBindingSpec {
    Operation {
        operation_name: String,
        request_plan: RequestPlan,
        direct_return: bool,
    },
    Stub,
}

#[derive(Debug, Clone)]
pub(crate) struct PlannedTypeInfo {
    pub(crate) full_name: String,
    pub(crate) package: String,
    pub(crate) file_name: Option<String>,
}

impl PlannedTypeInfo {
    fn from_message(message: &MessageMetadata) -> Self {
        Self {
            full_name: message.full_name.clone(),
            package: message.package.clone(),
            file_name: message.file_name.clone(),
        }
    }

    fn from_enum(enumeration: &EnumMetadata) -> Self {
        Self {
            full_name: enumeration.full_name.clone(),
            package: enumeration.package.clone(),
            file_name: enumeration.file_name.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PlannedEnum {
    pub(crate) info: PlannedTypeInfo,
    pub(crate) name: String,
    pub(crate) values: Vec<PlannedEnumValue>,
}

#[derive(Debug, Clone)]
pub(crate) struct PlannedEnumValue {
    pub(crate) name: String,
    pub(crate) number: i32,
}

#[derive(Debug, Clone)]
pub(crate) struct PlannedModel {
    pub(crate) info: PlannedTypeInfo,
    pub(crate) name: String,
    pub(crate) capabilities: ModelCapabilities,
    pub(crate) generated_model: GeneratedModelSpec,
    pub(crate) fields: Vec<PlannedField>,
    pub(crate) sourced_fields: Vec<PlannedSourcedField>,
}

#[derive(Debug, Clone)]
pub(crate) struct PlannedField {
    pub(crate) owner_name: String,
    pub(crate) proto_name: String,
    pub(crate) authored_name: String,
    // NOTE: This is already the selected language's annotation string from the spec.
    pub(crate) annotation_override: Option<String>,
    pub(crate) required: bool,
    pub(crate) has_presence: bool,
    pub(crate) role: PlannedFieldRole,
    pub(crate) kind: PlannedFieldKind,
}

#[derive(Debug, Clone)]
pub(crate) struct PlannedSourcedField {
    pub(crate) proto_name: String,
    // NOTE: This is already the selected language's source expression from the spec.
    pub(crate) source_expr: String,
    pub(crate) kind: PlannedFieldKind,
}

#[derive(Debug, Clone)]
pub(crate) enum PlannedFieldRole {
    Plain,
    // NOTE: FunctionFieldSpec still carries selected-language annotation strings today.
    Function(FunctionFieldSpec),
    FunctionArgs,
    // NOTE: WithArgumentsFieldSpec still carries selected-language annotation/expr strings today.
    WithArguments(WithArgumentsFieldSpec),
    WithArgumentsArgs,
}

#[derive(Debug, Clone)]
pub(crate) enum PlannedFieldKind {
    Singular(PlannedValueType),
    Repeated(PlannedValueType),
    Map {
        key: PlannedValueType,
        value: PlannedValueType,
    },
}

#[derive(Debug, Clone)]
pub(crate) enum PlannedValueType {
    Scalar(PlannedScalarType),
    Enum(PlannedEnumType),
    Message(PlannedMessageType),
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlannedScalarType {
    Float,
    Int32,
    Int64,
    Bool,
    String,
    Bytes,
}

#[derive(Debug, Clone)]
pub(crate) struct PlannedEnumType {
    pub(crate) info: Option<PlannedTypeInfo>,
    pub(crate) name: Option<String>,
    // NOTE: This is already the selected language's replacement payload from the spec.
    pub(crate) replacement: Option<TypeReplacementSpec>,
}

#[derive(Debug, Clone)]
pub(crate) struct PlannedMessageType {
    pub(crate) info: PlannedTypeInfo,
    pub(crate) model_name: String,
    // NOTE: This is already the selected language's replacement payload from the spec.
    pub(crate) replacement: Option<TypeReplacementSpec>,
}

pub(crate) fn service_client_name(service_name: &str) -> String {
    format!("{service_name}Client")
}

pub(crate) fn message_model_name(full_name: &str) -> String {
    full_name
        .rsplit('.')
        .next()
        .expect("descriptor names should not be empty")
        .to_string()
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

pub(crate) fn build_api_plan(spec: &ApiSpec, descriptors: &DescriptorIndex) -> Result<ApiPlan> {
    let mut plan = ApiPlan::default();
    let root_model_capabilities = root_model_capabilities(spec, descriptors)?;

    for service in &spec.services {
        let planned_service = plan_service(
            service,
            spec,
            descriptors,
            &root_model_capabilities,
            &mut plan,
        )?;
        plan.services.push(planned_service);
    }

    Ok(plan)
}

pub(crate) fn plan_message_type(
    message: &MessageMetadata,
    requested_capabilities: ModelCapabilities,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    plan: &mut ApiPlan,
) -> PlannedMessageType {
    let planned_message = planned_message_reference(message, spec);
    if planned_message.replacement.is_none() {
        ensure_model_plan(message, requested_capabilities, spec, descriptors, plan);
    }
    planned_message
}

fn root_model_capabilities(
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
) -> Result<BTreeMap<String, ModelCapabilities>> {
    let mut capabilities: BTreeMap<String, ModelCapabilities> = BTreeMap::new();

    for service in &spec.services {
        for operation in &service.operations {
            let input_message = descriptors
                .message(operation.input_proto())
                .ok_or_else(|| Error::UnknownOperationInputProto {
                    service: service.name.clone(),
                    operation: operation.name.clone(),
                    type_name: operation.input_proto().to_string(),
                })?;
            capabilities
                .entry(input_message.full_name.clone())
                .or_default()
                .merge(ModelCapabilities::TO_PROTO_ONLY);

            if operation.output_transform().is_some() || operation.output_resource().is_some() {
                continue;
            }

            let output_message = descriptors
                .message(operation.output_proto())
                .ok_or_else(|| Error::UnknownOperationOutputProto {
                    service: service.name.clone(),
                    operation: operation.name.clone(),
                    type_name: operation.output_proto().to_string(),
                })?;
            capabilities
                .entry(output_message.full_name.clone())
                .or_default()
                .merge(ModelCapabilities::BIDIRECTIONAL);
        }
    }

    Ok(capabilities)
}

fn plan_service(
    service: &ServiceSpec,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    root_model_capabilities: &BTreeMap<String, ModelCapabilities>,
    plan: &mut ApiPlan,
) -> Result<PlannedService> {
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
            plan_operation(
                &service.name,
                operation,
                spec,
                descriptors,
                root_model_capabilities,
                plan,
                resolved_resources.operation_returns.get(&operation.name),
            )
        })
        .collect::<Result<Vec<_>>>()?;

    let operation_bindings = operations
        .iter()
        .map(|operation| OperationBindingInfo {
            name: &operation.name,
            direct_return: operation.output_transform.is_some()
                || operation.output_resource_return.is_some(),
        })
        .collect::<Vec<_>>();
    let resources = resolved_resources
        .resources
        .iter()
        .map(|resource| plan_resource(service, resource, &operation_bindings))
        .collect::<Result<Vec<_>>>()?;

    Ok(PlannedService {
        name: service.name.clone(),
        endpoint,
        operations,
        resources,
    })
}

fn plan_operation(
    service_name: &str,
    operation: &OperationSpec,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    root_model_capabilities: &BTreeMap<String, ModelCapabilities>,
    plan: &mut ApiPlan,
    output_resource_return: Option<&ResolvedResourceReturnSpec>,
) -> Result<PlannedOperation> {
    let input_message = descriptors
        .message(operation.input_proto())
        .ok_or_else(|| Error::UnknownOperationInputProto {
            service: service_name.to_string(),
            operation: operation.name.clone(),
            type_name: operation.input_proto().to_string(),
        })?;
    let output_message = descriptors
        .message(operation.output_proto())
        .ok_or_else(|| Error::UnknownOperationOutputProto {
            service: service_name.to_string(),
            operation: operation.name.clone(),
            type_name: operation.output_proto().to_string(),
        })?;

    let input = plan_message_type(
        input_message,
        root_model_capabilities
            .get(&input_message.full_name)
            .copied()
            .unwrap_or(ModelCapabilities::TO_PROTO_ONLY),
        spec,
        descriptors,
        plan,
    );

    let output = planned_message_reference(output_message, spec);
    if operation.output_transform().is_none() && output_resource_return.is_none() {
        let _ = plan_message_type(
            output_message,
            root_model_capabilities
                .get(&output_message.full_name)
                .copied()
                .unwrap_or(ModelCapabilities::BIDIRECTIONAL),
            spec,
            descriptors,
            plan,
        );
    }

    Ok(PlannedOperation {
        name: operation.name.clone(),
        input,
        output,
        output_transform: operation.output_transform().cloned(),
        output_resource_return: plan_operation_resource_return(output_resource_return),
    })
}

fn plan_operation_resource_return(
    output_resource_return: Option<&ResolvedResourceReturnSpec>,
) -> Option<PlannedOperationResourceReturn> {
    output_resource_return.map(|resource_return| PlannedOperationResourceReturn {
        resource_type_name: resource_return.resource_name.to_upper_camel_case(),
        bindings: resource_return
            .bindings
            .iter()
            .map(|binding| PlannedOperationResourceFieldBinding {
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

fn plan_resource(
    service: &ServiceSpec,
    resource: &ResolvedResourceSpec,
    operations: &[OperationBindingInfo<'_>],
) -> Result<PlannedResource> {
    let methods = resource
        .methods
        .iter()
        .map(|method| {
            let binding = match &method.binding {
                ResolvedResourceMethodBinding::Operation {
                    operation_name,
                    request_plan,
                } => {
                    let operation = operations.iter().find(|operation| operation.name == operation_name).ok_or_else(|| Error::InvalidResourceMethod {
                        service: service.name.clone(),
                        resource: resource.name.to_upper_camel_case(),
                        method: method.name.to_string(),
                        reason: format!("bound operation `{operation_name}` was not rendered"),
                    })?;
                    PlannedResourceMethodBindingSpec::Operation {
                        operation_name: operation.name.to_string(),
                        request_plan: request_plan.clone(),
                        direct_return: operation.direct_return,
                    }
                }
                ResolvedResourceMethodBinding::Stub => PlannedResourceMethodBindingSpec::Stub,
            };

            Ok(PlannedResourceMethod {
                name: method.name.clone(),
                params: method
                    .params
                    .iter()
                    .map(planned_resource_field)
                    .collect(),
                result_annotation: method.result.as_ref().map(|result| result.annotation.clone()),
                binding,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(PlannedResource {
        name: resource.name.clone(),
        type_name: resource.name.to_upper_camel_case(),
        fields: resource.fields.iter().map(planned_resource_field).collect(),
        methods,
    })
}

fn planned_resource_field(field: &ResourceFieldSpec) -> PlannedResourceField {
    PlannedResourceField {
        name: field.name.clone(),
        annotation: field.annotation.clone(),
        optional: field.optional,
    }
}

fn planned_message_reference(message: &MessageMetadata, spec: &ApiSpec) -> PlannedMessageType {
    PlannedMessageType {
        info: PlannedTypeInfo::from_message(message),
        model_name: message_model_name(&message.full_name),
        replacement: spec
            .type_override(&message.full_name)
            .and_then(|type_override| type_override.replacement())
            .cloned(),
    }
}

fn ensure_model_plan(
    message: &MessageMetadata,
    requested_capabilities: ModelCapabilities,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    plan: &mut ApiPlan,
) {
    if spec
        .type_override(&message.full_name)
        .and_then(|type_override| type_override.replacement())
        .is_some()
    {
        return;
    }

    if let Some(existing) = plan.models.get_mut(&message.full_name) {
        existing.capabilities.merge(requested_capabilities);
        return;
    }

    let generated_model = spec
        .type_override(&message.full_name)
        .and_then(|type_override| type_override.generated_model())
        .cloned()
        .unwrap_or_default();

    plan.models.insert(
        message.full_name.clone(),
        PlannedModel {
            info: PlannedTypeInfo::from_message(message),
            name: message_model_name(&message.full_name),
            capabilities: requested_capabilities,
            generated_model: generated_model.clone(),
            fields: Vec::new(),
            sourced_fields: Vec::new(),
        },
    );

    let fields = message
        .descriptor
        .field
        .iter()
        .filter(|field| {
            let proto_name = field
                .name
                .as_deref()
                .expect("descriptor fields should be named");
            !spec
                .type_override(&message.full_name)
                .is_some_and(|type_override| type_override.is_field_hidden(proto_name))
        })
        .map(|field| plan_field(message, field, spec, descriptors, plan))
        .collect();

    let sourced_fields = message
        .descriptor
        .field
        .iter()
        .filter_map(|field| {
            let proto_name = field
                .name
                .as_deref()
                .expect("descriptor fields should be named");
            spec.type_override(&message.full_name)
                .and_then(|type_override| type_override.field_source(proto_name))
                .map(|source_expr| {
                    plan_sourced_field(message, field, source_expr, spec, descriptors, plan)
                })
        })
        .collect();

    let model = plan
        .models
        .get_mut(&message.full_name)
        .expect("model should be inserted before recursive field planning");
    model.fields = fields;
    model.sourced_fields = sourced_fields;
}

fn ensure_enum_plan(enumeration: &EnumMetadata, plan: &mut ApiPlan) {
    plan.enums
        .entry(enumeration.full_name.clone())
        .or_insert_with(|| PlannedEnum {
            info: PlannedTypeInfo::from_enum(enumeration),
            name: enum_name(&enumeration.full_name),
            values: enumeration
                .descriptor
                .value
                .iter()
                .filter_map(|value| {
                    Some(PlannedEnumValue {
                        name: value.name.as_deref()?.to_string(),
                        number: value.number?,
                    })
                })
                .collect(),
        });
}

fn plan_field(
    message: &MessageMetadata,
    field: &FieldDescriptorProto,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    plan: &mut ApiPlan,
) -> PlannedField {
    let proto_name = field
        .name
        .as_deref()
        .expect("descriptor fields should be named")
        .to_string();
    let generated_model = spec
        .type_override(&message.full_name)
        .and_then(|type_override| type_override.generated_model());

    PlannedField {
        owner_name: message_model_name(&message.full_name),
        authored_name: generated_model
            .and_then(|generated_model| generated_model.field_name_override(&proto_name))
            .unwrap_or(&proto_name)
            .to_string(),
        annotation_override: generated_model
            .and_then(|generated_model| generated_model.field_annotation(&proto_name))
            .map(str::to_string),
        required: spec
            .type_override(&message.full_name)
            .is_some_and(|type_override| type_override.is_field_required(&proto_name)),
        has_presence: field_has_presence(field, field_type(field)),
        role: planned_field_role(generated_model, &proto_name),
        kind: planned_field_kind(field, spec, descriptors, plan),
        proto_name,
    }
}

fn plan_sourced_field(
    _message: &MessageMetadata,
    field: &FieldDescriptorProto,
    source_expr: &str,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    plan: &mut ApiPlan,
) -> PlannedSourcedField {
    PlannedSourcedField {
        proto_name: field
            .name
            .as_deref()
            .expect("descriptor fields should be named")
            .to_string(),
        source_expr: source_expr.to_string(),
        kind: planned_field_kind(field, spec, descriptors, plan),
    }
}

fn planned_field_role(
    generated_model: Option<&GeneratedModelSpec>,
    proto_name: &str,
) -> PlannedFieldRole {
    if let Some(function) =
        generated_model.and_then(|generated_model| generated_model.function(proto_name))
    {
        return PlannedFieldRole::Function(function.clone());
    }
    if generated_model
        .and_then(|generated_model| generated_model.function_for_args_field(proto_name))
        .is_some()
    {
        return PlannedFieldRole::FunctionArgs;
    }
    if let Some(with_arguments) =
        generated_model.and_then(|generated_model| generated_model.with_arguments(proto_name))
    {
        return PlannedFieldRole::WithArguments(with_arguments.clone());
    }
    if generated_model
        .and_then(|generated_model| generated_model.with_arguments_for_args_field(proto_name))
        .is_some()
    {
        return PlannedFieldRole::WithArgumentsArgs;
    }
    PlannedFieldRole::Plain
}

fn planned_field_kind(
    field: &FieldDescriptorProto,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    plan: &mut ApiPlan,
) -> PlannedFieldKind {
    if let Some((key, value)) = map_field_value_types(field, spec, descriptors, plan) {
        return PlannedFieldKind::Map { key, value };
    }

    let value = planned_value_type(field, spec, descriptors, plan);
    if field_label(field) == Some(Label::Repeated) {
        PlannedFieldKind::Repeated(value)
    } else {
        PlannedFieldKind::Singular(value)
    }
}

fn map_field_value_types(
    field: &FieldDescriptorProto,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    plan: &mut ApiPlan,
) -> Option<(PlannedValueType, PlannedValueType)> {
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
        planned_value_type(key_field, spec, descriptors, plan),
        planned_value_type(value_field, spec, descriptors, plan),
    ))
}

fn planned_value_type(
    field: &FieldDescriptorProto,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    plan: &mut ApiPlan,
) -> PlannedValueType {
    match field_type(field) {
        Some(Type::Double | Type::Float) => PlannedValueType::Scalar(PlannedScalarType::Float),
        Some(Type::Int64 | Type::Uint64 | Type::Fixed64 | Type::Sfixed64 | Type::Sint64) => {
            PlannedValueType::Scalar(PlannedScalarType::Int64)
        }
        Some(Type::Int32 | Type::Fixed32 | Type::Uint32 | Type::Sfixed32 | Type::Sint32) => {
            PlannedValueType::Scalar(PlannedScalarType::Int32)
        }
        Some(Type::Bool) => PlannedValueType::Scalar(PlannedScalarType::Bool),
        Some(Type::String) => PlannedValueType::Scalar(PlannedScalarType::String),
        Some(Type::Bytes) => PlannedValueType::Scalar(PlannedScalarType::Bytes),
        Some(Type::Enum) => PlannedValueType::Enum(plan_enum_type(field, spec, descriptors, plan)),
        Some(Type::Message) | Some(Type::Group) => {
            if let Some(message) = field
                .type_name
                .as_deref()
                .and_then(|type_name| descriptors.message(type_name.trim_start_matches('.')))
            {
                PlannedValueType::Message(plan_message_type(
                    message,
                    ModelCapabilities::BIDIRECTIONAL,
                    spec,
                    descriptors,
                    plan,
                ))
            } else {
                PlannedValueType::Unknown
            }
        }
        None => PlannedValueType::Unknown,
    }
}

fn plan_enum_type(
    field: &FieldDescriptorProto,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    plan: &mut ApiPlan,
) -> PlannedEnumType {
    let Some(enumeration) = field
        .type_name
        .as_deref()
        .and_then(|type_name| descriptors.enumeration(type_name.trim_start_matches('.')))
    else {
        return PlannedEnumType {
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
        ensure_enum_plan(enumeration, plan);
    }

    PlannedEnumType {
        info: Some(PlannedTypeInfo::from_enum(enumeration)),
        name: Some(enum_name(&enumeration.full_name)),
        replacement,
    }
}
