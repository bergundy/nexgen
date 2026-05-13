use std::collections::BTreeMap;

use prost_types::FieldDescriptorProto;
use prost_types::field_descriptor_proto::{Label, Type};

use crate::descriptors::{DescriptorIndex, MessageMetadata};
use crate::error::{Error, Result};
use crate::language::Language;
use crate::python;
use crate::spec::{ApiSpec, GeneratedModelSpec, TypeOverrideSpec};
use crate::typescript;

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
struct MessageUsage {
    input: bool,
    output: bool,
}

pub(crate) fn validate_type_overrides(
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    language: Language,
) -> Result<()> {
    let usages = language_message_usages(spec, descriptors, language)?;
    for (type_name, type_override) in &spec.types {
        if let Some(message) = descriptors.message(type_name) {
            validate_message_type_override(
                type_name,
                type_override,
                message,
                descriptors,
                usages.get(type_name).copied().unwrap_or_default(),
                language,
            )?;
        } else if descriptors.enumeration(type_name).is_some() {
            validate_enum_type_override(type_name, type_override)?;
        } else {
            return Err(Error::UnknownTypeOverride {
                type_name: type_name.clone(),
            });
        }
    }

    Ok(())
}

fn language_message_usages(
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    _language: Language,
) -> Result<BTreeMap<String, MessageUsage>> {
    let mut usages: BTreeMap<String, MessageUsage> = BTreeMap::new();

    for service in &spec.services {
        for operation in &service.operations {
            let input_message = descriptors
                .message(operation.input_proto())
                .ok_or_else(|| Error::UnknownOperationInputProto {
                    service: service.name.clone(),
                    operation: operation.name.clone(),
                    type_name: operation.input_proto().to_string(),
                })?;
            usages
                .entry(input_message.full_name.clone())
                .or_default()
                .input = true;

            if operation.output_transform().is_some() || operation.output_resource().is_some() {
                continue;
            }

            let output_message =
                descriptors
                    .message(operation.output_proto())
                    .ok_or_else(|| Error::UnknownOperationOutputProto {
                        service: service.name.clone(),
                        operation: operation.name.clone(),
                        type_name: operation.output_proto().to_string(),
                    })?;
            usages
                .entry(output_message.full_name.clone())
                .or_default()
                .output = true;
        }
    }

    Ok(usages)
}

fn validate_message_type_override(
    message_name: &str,
    type_override: &TypeOverrideSpec,
    message: &MessageMetadata,
    descriptors: &DescriptorIndex,
    usage: MessageUsage,
    language: Language,
) -> Result<()> {
    for field_name in &type_override.required_fields {
        validate_model_required_field(message_name, field_name, message, descriptors)?;
    }
    for field_name in &type_override.omitted_fields {
        validate_model_override_field(message_name, field_name, message)?;
    }
    if let Some(generated_model) = type_override.generated_model() {
        for field_name in &generated_model.declared_fields {
            validate_model_override_field(message_name, field_name, message)?;
        }
        validate_generated_model_fields(
            message_name,
            type_override,
            &generated_model.field_names,
            &generated_model.field_annotations,
            &generated_model.field_sources,
            message,
            usage,
            language,
        )?;
        validate_invocation_fields(
            message_name,
            type_override,
            generated_model,
            message,
            descriptors,
            usage,
            language,
        )?;
    }
    for field_name in type_override
        .required_fields
        .intersection(&type_override.omitted_fields)
    {
        return Err(Error::ConflictingTypeOverrideField {
            message: message_name.to_string(),
            field: (*field_name).clone(),
        });
    }

    Ok(())
}

fn validate_generated_model_fields(
    message_name: &str,
    type_override: &TypeOverrideSpec,
    field_names: &BTreeMap<String, String>,
    field_annotations: &BTreeMap<String, String>,
    field_sources: &BTreeMap<String, String>,
    message: &MessageMetadata,
    usage: MessageUsage,
    language: Language,
) -> Result<()> {
    for field_name in field_names.keys() {
        validate_model_override_field(message_name, field_name, message)?;
        if type_override.omitted_fields.contains(field_name) {
            return Err(Error::OmittedCustomizedTypeOverrideField {
                message: message_name.to_string(),
                field: field_name.to_string(),
            });
        }
    }

    for field_name in field_annotations.keys() {
        validate_model_override_field(message_name, field_name, message)?;
        if type_override.omitted_fields.contains(field_name) {
            return Err(Error::OmittedCustomizedTypeOverrideField {
                message: message_name.to_string(),
                field: field_name.to_string(),
            });
        }
    }

    for field_name in field_sources.keys() {
        validate_model_override_field(message_name, field_name, message)?;
        if type_override.omitted_fields.contains(field_name) {
            return Err(Error::ConflictingTypeOverrideFieldProperties {
                message: message_name.to_string(),
                field: field_name.to_string(),
                property: "source",
                conflicting_property: "omit",
            });
        }
        if type_override.required_fields.contains(field_name) {
            return Err(Error::ConflictingTypeOverrideFieldProperties {
                message: message_name.to_string(),
                field: field_name.to_string(),
                property: "source",
                conflicting_property: "required",
            });
        }
        if usage.output {
            return Err(Error::UnsupportedSourcedTypeField {
                message: message_name.to_string(),
                field: field_name.to_string(),
                reason: "sourced fields are only supported on input-only generated models"
                    .to_string(),
            });
        }
    }

    for field_name in field_annotations
        .keys()
        .filter(|field_name| field_sources.contains_key(*field_name))
    {
        return Err(Error::ConflictingTypeOverrideFieldProperties {
            message: message_name.to_string(),
            field: field_name.to_string(),
            property: "type",
            conflicting_property: "source",
        });
    }

    let mut seen_generated_names: BTreeMap<String, String> = BTreeMap::new();
    for field in &message.descriptor.field {
        let proto_name = field
            .name
            .as_deref()
            .expect("descriptor fields should be named");
        if type_override.is_field_hidden(proto_name) {
            continue;
        }

        let generated_name = field_name_for_language(
            language,
            field,
            field_names.get(proto_name).map(String::as_str),
        );
        if let Some(existing) =
            seen_generated_names.insert(generated_name.clone(), proto_name.to_string())
        {
            return Err(Error::InvalidTypeOverrideField {
                message: message_name.to_string(),
                field: proto_name.to_string(),
                property: "name",
                reason: format!(
                    "generated field name `{generated_name}` conflicts with field `{existing}`"
                ),
            });
        }
    }

    Ok(())
}

fn validate_invocation_fields(
    message_name: &str,
    type_override: &TypeOverrideSpec,
    generated_model: &GeneratedModelSpec,
    message: &MessageMetadata,
    descriptors: &DescriptorIndex,
    usage: MessageUsage,
    language: Language,
) -> Result<()> {
    if generated_model.functions.is_empty() && generated_model.with_arguments.is_empty() {
        return Ok(());
    }

    if usage.output {
        if let Some(field) = generated_model.functions.keys().next() {
            return Err(Error::InvalidTypeOverrideField {
                message: message_name.to_string(),
                field: field.clone(),
                property: "function",
                reason: "function fields are only supported on input-only generated models"
                    .to_string(),
            });
        }
        if let Some(field) = generated_model.with_arguments.keys().next() {
            return Err(Error::InvalidTypeOverrideField {
                message: message_name.to_string(),
                field: field.clone(),
                property: "withArguments",
                reason: "withArguments fields are only supported on input-only generated models"
                    .to_string(),
            });
        }
    }

    let mut primary_field_name: Option<&str> = None;
    let mut seen_args_fields = BTreeMap::new();

    for (field_name, function) in &generated_model.functions {
        if function.primary {
            if let Some(existing) = primary_field_name {
                return Err(Error::InvalidTypeOverrideField {
                    message: message_name.to_string(),
                    field: field_name.clone(),
                    property: "function",
                    reason: format!(
                        "only one primary function field is supported; `{existing}` is already primary"
                    ),
                });
            }
            primary_field_name = Some(field_name);
        }

        if let Some((existing, _)) =
            seen_args_fields.insert(function.args_field.as_str(), (field_name, "function"))
        {
            return Err(Error::InvalidTypeOverrideField {
                message: message_name.to_string(),
                field: field_name.clone(),
                property: "function",
                reason: format!(
                    "argsField `{}` is already used by function field `{existing}`",
                    function.args_field
                ),
            });
        }
    }

    for (field_name, with_arguments) in &generated_model.with_arguments {
        if language != Language::TypeScript {
            return Err(Error::InvalidTypeOverrideField {
                message: message_name.to_string(),
                field: field_name.clone(),
                property: "withArguments",
                reason: "withArguments fields are only supported for TypeScript".to_string(),
            });
        }
        if let Some((existing, _)) = seen_args_fields.insert(
            with_arguments.args_field.as_str(),
            (field_name, "withArguments"),
        ) {
            return Err(Error::InvalidTypeOverrideField {
                message: message_name.to_string(),
                field: field_name.clone(),
                property: "withArguments",
                reason: format!(
                    "argsField `{}` is already used by field `{existing}`",
                    with_arguments.args_field
                ),
            });
        }
    }

    for (field_name, function) in &generated_model.functions {
        let callable_field = validate_model_override_field(message_name, field_name, message)?;
        if type_override.omitted_fields.contains(field_name) {
            return Err(Error::OmittedCustomizedTypeOverrideField {
                message: message_name.to_string(),
                field: field_name.clone(),
            });
        }
        if function.args_field == *field_name {
            return Err(Error::InvalidTypeOverrideField {
                message: message_name.to_string(),
                field: field_name.clone(),
                property: "function",
                reason: "argsField must point to a different field".to_string(),
            });
        }

        validate_named_invocation_field(
            message_name,
            field_name,
            callable_field,
            descriptors,
            "function",
        )?;

        let args_field =
            validate_model_override_field(message_name, &function.args_field, message)?;
        if type_override.omitted_fields.contains(&function.args_field) {
            return Err(Error::OmittedCustomizedTypeOverrideField {
                message: message_name.to_string(),
                field: function.args_field.clone(),
            });
        }
        if type_override.required_fields.contains(&function.args_field) {
            return Err(Error::ConflictingTypeOverrideFieldProperties {
                message: message_name.to_string(),
                field: function.args_field.clone(),
                property: "function",
                conflicting_property: "required",
            });
        }
        if generated_model
            .field_annotations
            .contains_key(&function.args_field)
        {
            return Err(Error::ConflictingTypeOverrideFieldProperties {
                message: message_name.to_string(),
                field: function.args_field.clone(),
                property: "function",
                conflicting_property: "type",
            });
        }
        if generated_model
            .field_sources
            .contains_key(&function.args_field)
        {
            return Err(Error::ConflictingTypeOverrideFieldProperties {
                message: message_name.to_string(),
                field: function.args_field.clone(),
                property: "function",
                conflicting_property: "source",
            });
        }
        validate_invocation_args_field(
            message_name,
            &function.args_field,
            args_field,
            descriptors,
            "function",
        )?;
    }

    for (field_name, with_arguments) in &generated_model.with_arguments {
        let value_field = validate_model_override_field(message_name, field_name, message)?;
        if type_override.omitted_fields.contains(field_name) {
            return Err(Error::OmittedCustomizedTypeOverrideField {
                message: message_name.to_string(),
                field: field_name.clone(),
            });
        }
        if with_arguments.args_field == *field_name {
            return Err(Error::InvalidTypeOverrideField {
                message: message_name.to_string(),
                field: field_name.clone(),
                property: "withArguments",
                reason: "argsField must point to a different field".to_string(),
            });
        }

        validate_named_invocation_field(
            message_name,
            field_name,
            value_field,
            descriptors,
            "withArguments",
        )?;

        let args_field =
            validate_model_override_field(message_name, &with_arguments.args_field, message)?;
        if type_override
            .omitted_fields
            .contains(&with_arguments.args_field)
        {
            return Err(Error::OmittedCustomizedTypeOverrideField {
                message: message_name.to_string(),
                field: with_arguments.args_field.clone(),
            });
        }
        if type_override
            .required_fields
            .contains(&with_arguments.args_field)
        {
            return Err(Error::ConflictingTypeOverrideFieldProperties {
                message: message_name.to_string(),
                field: with_arguments.args_field.clone(),
                property: "withArguments",
                conflicting_property: "required",
            });
        }
        if generated_model
            .field_annotations
            .contains_key(&with_arguments.args_field)
        {
            return Err(Error::ConflictingTypeOverrideFieldProperties {
                message: message_name.to_string(),
                field: with_arguments.args_field.clone(),
                property: "withArguments",
                conflicting_property: "type",
            });
        }
        if generated_model
            .field_sources
            .contains_key(&with_arguments.args_field)
        {
            return Err(Error::ConflictingTypeOverrideFieldProperties {
                message: message_name.to_string(),
                field: with_arguments.args_field.clone(),
                property: "withArguments",
                conflicting_property: "source",
            });
        }
        validate_invocation_args_field(
            message_name,
            &with_arguments.args_field,
            args_field,
            descriptors,
            "withArguments",
        )?;
    }

    Ok(())
}

fn validate_named_invocation_field(
    message_name: &str,
    field_name: &str,
    field: &FieldDescriptorProto,
    descriptors: &DescriptorIndex,
    property: &'static str,
) -> Result<()> {
    match field_type(field) {
        Some(Type::String) => Ok(()),
        Some(Type::Message) => {
            let Some(type_name) = field.type_name.as_deref() else {
                return Err(Error::InvalidTypeOverrideField {
                    message: message_name.to_string(),
                    field: field_name.to_string(),
                    property,
                    reason: "field message type is missing a descriptor name".to_string(),
                });
            };
            let Some(message) = descriptors.message(type_name.trim_start_matches('.')) else {
                return Err(Error::InvalidTypeOverrideField {
                    message: message_name.to_string(),
                    field: field_name.to_string(),
                    property,
                    reason: "field message type is not available in the descriptors".to_string(),
                });
            };
            let has_name_field = message.descriptor.field.iter().any(|field| {
                field.name.as_deref() == Some("name")
                    && field_label(field) != Some(Label::Repeated)
                    && field_type(field) == Some(Type::String)
            });
            if has_name_field {
                Ok(())
            } else {
                Err(Error::InvalidTypeOverrideField {
                    message: message_name.to_string(),
                    field: field_name.to_string(),
                    property,
                    reason: "field messages must expose a singular string `name` field".to_string(),
                })
            }
        }
        _ => Err(Error::InvalidTypeOverrideField {
            message: message_name.to_string(),
            field: field_name.to_string(),
            property,
            reason: "fields must be either a string field or a message with a `name` field"
                .to_string(),
        }),
    }
}

fn validate_invocation_args_field(
    message_name: &str,
    field_name: &str,
    field: &FieldDescriptorProto,
    descriptors: &DescriptorIndex,
    property: &'static str,
) -> Result<()> {
    if field_label(field) == Some(Label::Repeated) {
        return Err(Error::InvalidTypeOverrideField {
            message: message_name.to_string(),
            field: field_name.to_string(),
            property,
            reason: "argsField must point to a singular Payloads field".to_string(),
        });
    }

    let Some(type_name) = field.type_name.as_deref() else {
        return Err(Error::InvalidTypeOverrideField {
            message: message_name.to_string(),
            field: field_name.to_string(),
            property,
            reason: "argsField must point to temporal.api.common.v1.Payloads".to_string(),
        });
    };
    let normalized_type_name = type_name.trim_start_matches('.');
    if normalized_type_name == "temporal.api.common.v1.Payloads" {
        return Ok(());
    }

    if descriptors.message(normalized_type_name).is_none() {
        return Err(Error::InvalidTypeOverrideField {
            message: message_name.to_string(),
            field: field_name.to_string(),
            property,
            reason: "argsField message type is not available in the descriptors".to_string(),
        });
    }

    Err(Error::InvalidTypeOverrideField {
        message: message_name.to_string(),
        field: field_name.to_string(),
        property,
        reason: "argsField must point to temporal.api.common.v1.Payloads".to_string(),
    })
}

fn field_name_for_language(
    language: Language,
    field: &FieldDescriptorProto,
    explicit_name: Option<&str>,
) -> String {
    match language {
        Language::Python => python::python_field_name(
            explicit_name
                .or_else(|| field.name.as_deref())
                .expect("descriptor fields should be named"),
        ),
        Language::TypeScript => typescript::field_name(field, explicit_name),
        _ => explicit_name
            .or_else(|| field.name.as_deref())
            .expect("descriptor fields should be named")
            .to_string(),
    }
}

fn validate_enum_type_override(
    enumeration_name: &str,
    type_override: &TypeOverrideSpec,
) -> Result<()> {
    if !type_override.required_fields.is_empty() {
        return Err(Error::UnsupportedEnumTypeOverrideProperty {
            enumeration: enumeration_name.to_string(),
            property: "required",
        });
    }
    if !type_override.omitted_fields.is_empty() {
        return Err(Error::UnsupportedEnumTypeOverrideProperty {
            enumeration: enumeration_name.to_string(),
            property: "omit",
        });
    }
    if let Some(generated_model) = type_override.generated_model() {
        if !generated_model.field_names.is_empty()
            || !generated_model.declared_fields.is_empty()
            || !generated_model.field_annotations.is_empty()
            || !generated_model.field_sources.is_empty()
            || !generated_model.functions.is_empty()
            || !generated_model.with_arguments.is_empty()
        {
            return Err(Error::UnsupportedTypeOverrideProperty {
                type_name: enumeration_name.to_string(),
                property: "fields",
            });
        }
    }

    Ok(())
}

fn validate_model_override_field<'a>(
    message_name: &str,
    field_name: &str,
    message: &'a MessageMetadata,
) -> Result<&'a FieldDescriptorProto> {
    message
        .descriptor
        .field
        .iter()
        .find(|field| field.name.as_deref() == Some(field_name))
        .ok_or_else(|| Error::UnknownTypeOverrideField {
            message: message_name.to_string(),
            field: field_name.to_string(),
        })
}

fn validate_model_required_field(
    message_name: &str,
    field_name: &str,
    message: &MessageMetadata,
    descriptors: &DescriptorIndex,
) -> Result<()> {
    let field = validate_model_override_field(message_name, field_name, message)?;
    let field_type = field_type(field);

    if field_label(field) == Some(Label::Repeated) {
        let reason = if field_is_map(field, descriptors) {
            "map fields cannot be marked required"
        } else {
            "repeated fields cannot be marked required"
        };
        return Err(Error::UnsupportedRequiredTypeField {
            message: message_name.to_string(),
            field: field_name.to_string(),
            reason: reason.to_string(),
        });
    }

    if field_has_presence(field, field_type) || field_supports_required_without_presence(field_type)
    {
        return Ok(());
    }

    Err(Error::UnsupportedRequiredTypeField {
        message: message_name.to_string(),
        field: field_name.to_string(),
        reason: "field must support presence or be a string/bytes scalar".to_string(),
    })
}

fn field_is_map(field: &FieldDescriptorProto, descriptors: &DescriptorIndex) -> bool {
    if field_label(field) != Some(Label::Repeated) || field_type(field) != Some(Type::Message) {
        return false;
    }

    let Some(entry_name) = field.type_name.as_deref() else {
        return false;
    };
    let Some(entry) = descriptors.message(entry_name.trim_start_matches('.')) else {
        return false;
    };

    entry
        .descriptor
        .options
        .as_ref()
        .and_then(|options| options.map_entry)
        .unwrap_or(false)
}

fn field_has_presence(field: &FieldDescriptorProto, field_type: Option<Type>) -> bool {
    matches!(field_type, Some(Type::Message))
        || field.proto3_optional.unwrap_or(false)
        || field.oneof_index.is_some()
}

fn field_supports_required_without_presence(field_type: Option<Type>) -> bool {
    matches!(field_type, Some(Type::String | Type::Bytes))
}

fn field_label(field: &FieldDescriptorProto) -> Option<Label> {
    field.label.and_then(|label| Label::try_from(label).ok())
}

fn field_type(field: &FieldDescriptorProto) -> Option<Type> {
    field
        .r#type
        .and_then(|field_type| Type::try_from(field_type).ok())
}
