use std::collections::{BTreeMap, BTreeSet};

use heck::{ToKebabCase, ToUpperCamelCase};
use prost_types::FieldDescriptorProto;
use prost_types::field_descriptor_proto::Type;

use crate::descriptors::DescriptorIndex;
use crate::error::{Error, Result};
use crate::spec::{
    ApiSpec, ResourceFieldSpec, ResourceMethodSpec, ResourceResultSpec, ServiceSpec,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedServiceResources {
    pub resources: Vec<ResolvedResourceSpec>,
    pub operation_returns: BTreeMap<String, ResolvedResourceReturnSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedResourceSpec {
    pub name: String,
    pub fields: Vec<ResourceFieldSpec>,
    pub methods: Vec<ResolvedResourceMethodSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedResourceMethodSpec {
    pub name: String,
    pub params: Vec<ResourceFieldSpec>,
    pub result: Option<ResourceResultSpec>,
    pub binding: ResolvedResourceMethodBinding,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedResourceMethodBinding {
    Operation {
        operation_name: String,
        request_plan: RequestPlan,
    },
    Stub,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedResourceReturnSpec {
    pub resource_name: String,
    pub bindings: Vec<ResolvedResourceFieldBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedResourceFieldBinding {
    pub field_name: String,
    pub optional: bool,
    pub source: ResolvedResourceBindingSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedResourceBindingSource {
    RequestField {
        field_name: String,
        proto_field_name: String,
        hidden: bool,
    },
    ResultField {
        field_name: String,
        proto_field_name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestPlan {
    Source(RequestPlanSource),
    Construct {
        message_name: String,
        fields: Vec<RequestPlanField>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestPlanField {
    pub field_name: String,
    pub value: RequestPlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestPlanSource {
    ResourceField(String),
    MethodParam(String),
}

#[derive(Debug, Clone)]
struct MessageFieldInfo {
    proto_name: String,
    wit_name: String,
    required: bool,
    hidden: bool,
    message_name: Option<String>,
}

pub(crate) fn resolve_service_resources(
    spec: &ApiSpec,
    service: &ServiceSpec,
    descriptors: &DescriptorIndex,
) -> Result<ResolvedServiceResources> {
    let resources = service
        .resources
        .iter()
        .map(|resource| resolve_resource_methods(spec, service, resource, descriptors))
        .collect::<Result<Vec<_>>>()?;

    let mut operation_returns = BTreeMap::new();
    for operation in &service.operations {
        let Some(resource_name) = operation.output_resource() else {
            continue;
        };
        let resource = service
            .resource(resource_name)
            .ok_or_else(|| Error::InvalidResource {
                service: service.name.clone(),
                resource: resource_name.to_string(),
                reason: format!(
                    "operation `{}` returns unknown resource `{resource_name}`",
                    operation.name
                ),
            })?;
        let bindings = resource
            .fields
            .iter()
            .map(|field| {
                let source = bind_resource_return_field(
                    spec,
                    &service.name,
                    &resource.name,
                    descriptors,
                    operation.input_proto(),
                    operation.output_proto(),
                    &field.name,
                )?;
                Ok(ResolvedResourceFieldBinding {
                    field_name: field.name.clone(),
                    optional: field.optional,
                    source,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        operation_returns.insert(
            operation.name.clone(),
            ResolvedResourceReturnSpec {
                resource_name: resource.name.clone(),
                bindings,
            },
        );
    }

    Ok(ResolvedServiceResources {
        resources,
        operation_returns,
    })
}

fn resolve_resource_methods(
    spec: &ApiSpec,
    service: &ServiceSpec,
    resource: &crate::spec::ResourceSpec,
    descriptors: &DescriptorIndex,
) -> Result<ResolvedResourceSpec> {
    let methods = resource
        .methods
        .iter()
        .map(|method| resolve_resource_method(spec, service, resource, method, descriptors))
        .collect::<Result<Vec<_>>>()?;
    Ok(ResolvedResourceSpec {
        name: resource.name.clone(),
        fields: resource.fields.clone(),
        methods,
    })
}

fn resolve_resource_method(
    spec: &ApiSpec,
    service: &ServiceSpec,
    resource: &crate::spec::ResourceSpec,
    method: &ResourceMethodSpec,
    descriptors: &DescriptorIndex,
) -> Result<ResolvedResourceMethodSpec> {
    let mut environment = BTreeMap::new();
    for field in &resource.fields {
        environment.insert(
            field.name.clone(),
            RequestPlanSource::ResourceField(field.name.clone()),
        );
    }
    for param in &method.params {
        environment.insert(
            param.name.clone(),
            RequestPlanSource::MethodParam(param.name.clone()),
        );
    }

    let mut matching_operations = Vec::new();
    for operation in &service.operations {
        let Some(request_plan) =
            synthesize_request_plan(spec, descriptors, operation.input_proto(), &environment)?
        else {
            continue;
        };

        if let Some(result) = &method.result {
            if let Some(resource_name) = &result.resource {
                if operation.output_resource() != Some(resource_name.as_str()) {
                    continue;
                }
            } else if let Some(proto_name) = &result.proto {
                if operation.output_proto() != proto_name {
                    continue;
                }
            }
        }

        matching_operations.push((operation.name.clone(), request_plan));
    }

    let binding = match matching_operations.len() {
        0 => ResolvedResourceMethodBinding::Stub,
        1 => {
            let (operation_name, request_plan) = matching_operations.pop().expect("len checked");
            ResolvedResourceMethodBinding::Operation {
                operation_name,
                request_plan,
            }
        }
        _ => {
            let matches = matching_operations
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(Error::InvalidResourceMethod {
                service: service.name.clone(),
                resource: resource.name.to_upper_camel_case(),
                method: method.name.to_string(),
                reason: format!("matches multiple operations: {matches}"),
            });
        }
    };

    Ok(ResolvedResourceMethodSpec {
        name: method.name.clone(),
        params: method.params.clone(),
        result: method.result.clone(),
        binding,
    })
}

fn bind_resource_return_field(
    spec: &ApiSpec,
    service_name: &str,
    resource_name: &str,
    descriptors: &DescriptorIndex,
    input_message_name: &str,
    output_message_name: &str,
    field_name: &str,
) -> Result<ResolvedResourceBindingSource> {
    if let Some(field) = find_message_field(spec, descriptors, input_message_name, field_name)? {
        return Ok(ResolvedResourceBindingSource::RequestField {
            field_name: field.wit_name,
            proto_field_name: field.proto_name,
            hidden: field.hidden,
        });
    }
    if let Some(field) =
        find_visible_message_field(spec, descriptors, output_message_name, field_name)?
    {
        return Ok(ResolvedResourceBindingSource::ResultField {
            field_name: field.wit_name,
            proto_field_name: field.proto_name,
        });
    }
    Err(Error::InvalidResource {
        service: service_name.to_string(),
        resource: resource_name.to_upper_camel_case(),
        reason: format!(
            "could not bind resource field `{field_name}` from operation input or output"
        ),
    })
}

fn synthesize_request_plan(
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    message_name: &str,
    environment: &BTreeMap<String, RequestPlanSource>,
) -> Result<Option<RequestPlan>> {
    let mut fields = Vec::new();
    for field in visible_message_fields(spec, descriptors, message_name)? {
        if let Some(source) = environment.get(&field.wit_name) {
            fields.push(RequestPlanField {
                field_name: field.wit_name,
                value: RequestPlan::Source(source.clone()),
            });
            continue;
        }

        if let Some(child_message_name) = field.message_name.as_deref() {
            let child_generated_model = spec
                .type_override(child_message_name)
                .and_then(|type_override| type_override.generated_model());
            if child_generated_model.is_some()
                && let Some(value) =
                    synthesize_request_plan(spec, descriptors, child_message_name, environment)?
            {
                fields.push(RequestPlanField {
                    field_name: field.wit_name,
                    value,
                });
                continue;
            }
        }

        if field.required {
            return Ok(None);
        }
    }

    Ok(Some(RequestPlan::Construct {
        message_name: message_name.to_string(),
        fields,
    }))
}

fn visible_message_fields(
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    message_name: &str,
) -> Result<Vec<MessageFieldInfo>> {
    Ok(all_message_fields(spec, descriptors, message_name)?
        .into_iter()
        .filter(|field| !field.hidden)
        .collect())
}

fn find_visible_message_field(
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    message_name: &str,
    field_name: &str,
) -> Result<Option<MessageFieldInfo>> {
    Ok(visible_message_fields(spec, descriptors, message_name)?
        .into_iter()
        .find(|field| field.wit_name == field_name))
}

fn find_message_field(
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    message_name: &str,
    field_name: &str,
) -> Result<Option<MessageFieldInfo>> {
    Ok(all_message_fields(spec, descriptors, message_name)?
        .into_iter()
        .find(|field| field.wit_name == field_name))
}

fn all_message_fields(
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    message_name: &str,
) -> Result<Vec<MessageFieldInfo>> {
    let message = descriptors
        .message(message_name)
        .ok_or_else(|| Error::UnknownTypeOverride {
            type_name: message_name.to_string(),
        })?;
    let type_override = spec.type_override(message_name);
    let generated_model = type_override.and_then(|type_override| type_override.generated_model());

    message
        .descriptor
        .field
        .iter()
        .map(|field| build_message_field_info(field, type_override, generated_model, descriptors))
        .collect()
}

fn build_message_field_info(
    field: &FieldDescriptorProto,
    type_override: Option<&crate::spec::TypeOverrideSpec>,
    generated_model: Option<&crate::spec::GeneratedModelSpec>,
    descriptors: &DescriptorIndex,
) -> Result<MessageFieldInfo> {
    let proto_name = field
        .name
        .as_deref()
        .expect("descriptor fields should be named");
    let wit_name = generated_model
        .and_then(|generated_model| generated_model.field_name_override(proto_name))
        .map(str::to_string)
        .unwrap_or_else(|| proto_name.to_kebab_case());
    let required =
        type_override.is_some_and(|type_override| type_override.is_field_required(proto_name));
    let hidden =
        type_override.is_some_and(|type_override| type_override.is_field_hidden(proto_name));
    let message_name = field_message_name(field, descriptors);
    Ok(MessageFieldInfo {
        proto_name: proto_name.to_string(),
        wit_name,
        required,
        hidden,
        message_name,
    })
}

fn field_message_name(
    field: &FieldDescriptorProto,
    descriptors: &DescriptorIndex,
) -> Option<String> {
    if field.r#type() != Type::Message {
        return None;
    }
    let type_name = field.type_name.as_deref()?.trim_start_matches('.');
    descriptors.message(type_name)?;
    Some(type_name.to_string())
}

pub(crate) fn ensure_unique_resource_names(spec: &ApiSpec) -> Result<()> {
    let mut seen_names = BTreeSet::new();
    for service in &spec.services {
        for resource in &service.resources {
            let generated_name = resource.name.to_upper_camel_case();
            if !seen_names.insert(generated_name.clone()) {
                return Err(Error::InvalidResource {
                    service: service.name.clone(),
                    resource: generated_name,
                    reason: "another resource uses the same generated name".to_string(),
                });
            }
        }
    }
    Ok(())
}
