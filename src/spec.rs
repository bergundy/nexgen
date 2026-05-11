use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use heck::{ToLowerCamelCase, ToSnakeCase};
use indexmap::IndexMap;
use prost_types::FieldDescriptorProto;
use prost_types::field_descriptor_proto::{Label, Type};
use serde::Deserialize;

use crate::descriptors::{DescriptorIndex, MessageMetadata};
use crate::error::{Error, Result};
use crate::language::Language;

#[derive(Debug, Clone, PartialEq)]
pub struct ApiSpec {
    pub version: String,
    pub support: SupportSpec,
    pub services: Vec<ServiceSpec>,
    pub types: BTreeMap<String, TypeOverrideSpec>,
}

impl ApiSpec {
    pub fn load(path: &Path) -> Result<Self> {
        let input = fs::read_to_string(path).map_err(|source| Error::ReadFile {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse(&input, path.to_path_buf())
    }

    pub fn parse(input: &str, path: PathBuf) -> Result<Self> {
        let raw: RawApiSpec =
            serde_yaml::from_str(input).map_err(|source| Error::YamlParse { path, source })?;
        Self::try_from(raw)
    }

    pub fn type_override(&self, type_name: &str) -> Option<&TypeOverrideSpec> {
        self.types.get(type_name.trim_start_matches('.'))
    }

    pub fn validate_type_overrides(&self, descriptors: &DescriptorIndex) -> Result<()> {
        let python_usages = self.language_message_usages(descriptors, Language::Python)?;
        let typescript_usages = self.language_message_usages(descriptors, Language::TypeScript)?;
        for (type_name, type_override) in &self.types {
            if let Some(message) = descriptors.message(type_name) {
                validate_message_type_override(
                    type_name,
                    type_override,
                    message,
                    descriptors,
                    python_usages.get(type_name).copied().unwrap_or_default(),
                    typescript_usages
                        .get(type_name)
                        .copied()
                        .unwrap_or_default(),
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
        &self,
        descriptors: &DescriptorIndex,
        language: Language,
    ) -> Result<BTreeMap<String, MessageUsage>> {
        let mut usages: BTreeMap<String, MessageUsage> = BTreeMap::new();

        for service in &self.services {
            for operation in &service.operations {
                if let Some(reference) = operation.language_ref(language, Direction::Input) {
                    let message = resolve_message_for_language(descriptors, language, reference)?;
                    usages.entry(message.full_name.clone()).or_default().input = true;
                }
                if let Some(reference) = operation.language_ref(language, Direction::Output) {
                    let message = resolve_message_for_language(descriptors, language, reference)?;
                    usages.entry(message.full_name.clone()).or_default().output = true;
                }
            }
        }

        Ok(usages)
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
struct MessageUsage {
    input: bool,
    output: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServiceSpec {
    pub name: String,
    pub endpoint: Option<String>,
    pub operations: Vec<OperationSpec>,
}

impl ServiceSpec {
    pub fn operation(&self, name: &str) -> Option<&OperationSpec> {
        self.operations
            .iter()
            .find(|operation| operation.name == name)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SupportSpec {
    pub python_file: Option<String>,
    pub typescript_file: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OperationSpec {
    pub name: String,
    pub input_refs: LanguageRefMap,
    pub output_refs: LanguageRefMap,
}

impl OperationSpec {
    pub fn language_ref(&self, language: Language, direction: Direction) -> Option<&str> {
        match direction {
            Direction::Input => self.input_refs.get(&language),
            Direction::Output => self.output_refs.get(&language),
        }
        .map(String::as_str)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TypeOverrideSpec {
    pub required_fields: BTreeSet<String>,
    pub omitted_fields: BTreeSet<String>,
    pub python: Option<LanguageOverrideSpec>,
    pub python_model: PythonGeneratedModelSpec,
    pub typescript: Option<LanguageOverrideSpec>,
    pub typescript_model: TypeScriptGeneratedModelSpec,
}

impl TypeOverrideSpec {
    pub fn is_field_required(&self, field_name: &str) -> bool {
        self.required_fields.contains(field_name)
    }

    pub fn is_field_omitted(&self, field_name: &str) -> bool {
        self.omitted_fields.contains(field_name)
    }

    pub fn is_field_hidden(&self, language: Language, field_name: &str) -> bool {
        self.omitted_fields.contains(field_name)
            || self.field_source(language, field_name).is_some()
    }

    pub fn language_override(&self, language: Language) -> Option<&LanguageOverrideSpec> {
        match language {
            Language::Python => self.python.as_ref(),
            Language::TypeScript => self.typescript.as_ref(),
            _ => None,
        }
    }

    pub fn python_generated_model(&self) -> Option<&PythonGeneratedModelSpec> {
        if self.python_model.is_empty() {
            None
        } else {
            Some(&self.python_model)
        }
    }

    pub fn typescript_generated_model(&self) -> Option<&TypeScriptGeneratedModelSpec> {
        if self.typescript_model.is_empty() {
            None
        } else {
            Some(&self.typescript_model)
        }
    }

    pub fn field_source(&self, language: Language, field_name: &str) -> Option<&str> {
        match language {
            Language::Python => self.python_generated_model()?.field_source(field_name),
            Language::TypeScript => self.typescript_generated_model()?.field_source(field_name),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageOverrideSpec {
    pub type_name: String,
    pub from_proto: String,
    pub to_proto: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PythonGeneratedModelSpec {
    pub type_parameters: Vec<PythonTypeParameterSpec>,
    pub field_annotations: BTreeMap<String, String>,
    pub field_sources: BTreeMap<String, String>,
}

impl PythonGeneratedModelSpec {
    pub fn is_empty(&self) -> bool {
        self.type_parameters.is_empty()
            && self.field_annotations.is_empty()
            && self.field_sources.is_empty()
    }

    pub fn field_annotation(&self, field_name: &str) -> Option<&str> {
        self.field_annotations.get(field_name).map(String::as_str)
    }

    pub fn field_source(&self, field_name: &str) -> Option<&str> {
        self.field_sources.get(field_name).map(String::as_str)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TypeScriptGeneratedModelSpec {
    pub field_annotations: BTreeMap<String, String>,
    pub field_sources: BTreeMap<String, String>,
}

impl TypeScriptGeneratedModelSpec {
    pub fn is_empty(&self) -> bool {
        self.field_annotations.is_empty() && self.field_sources.is_empty()
    }

    pub fn field_annotation(&self, field_name: &str) -> Option<&str> {
        self.field_annotations.get(field_name).map(String::as_str)
    }

    pub fn field_source(&self, field_name: &str) -> Option<&str> {
        self.field_sources.get(field_name).map(String::as_str)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonTypeParameterSpec {
    pub name: String,
    pub kind: PythonTypeParameterKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PythonTypeParameterKind {
    TypeVar,
    TypeVarTuple,
}

pub type LanguageRefMap = BTreeMap<Language, String>;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Direction {
    Input,
    Output,
}

impl Direction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Output => "output",
        }
    }
}

fn resolve_message_for_language<'a>(
    descriptors: &'a DescriptorIndex,
    language: Language,
    reference: &str,
) -> Result<&'a MessageMetadata> {
    match language {
        Language::Python => descriptors.resolve_python_ref(reference),
        Language::TypeScript => descriptors.resolve_typescript_ref(reference),
        language => Err(Error::UnsupportedLanguage { language }),
    }
}

#[derive(Debug, Deserialize)]
struct RawApiSpec {
    #[serde(rename = "nexusrpc")]
    version: String,
    #[serde(default)]
    support: RawSupportSpec,
    #[serde(default)]
    types: IndexMap<String, RawTypeOverride>,
    services: IndexMap<String, RawService>,
}

#[derive(Debug, Default, Deserialize)]
struct RawSupportSpec {
    #[serde(rename = "$pythonFile")]
    python_file: Option<String>,
    #[serde(rename = "$typescriptFile")]
    typescript_file: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawService {
    endpoint: Option<String>,
    operations: IndexMap<String, RawOperation>,
}

#[derive(Debug, Deserialize)]
struct RawOperation {
    input: RawLanguageRefs,
    output: RawLanguageRefs,
}

#[derive(Debug, Default, Deserialize)]
struct RawLanguageRefs {
    #[serde(rename = "$dotnetRef")]
    dotnet_ref: Option<String>,
    #[serde(rename = "$goRef")]
    go_ref: Option<String>,
    #[serde(rename = "$javaRef")]
    java_ref: Option<String>,
    #[serde(rename = "$pythonRef")]
    python_ref: Option<String>,
    #[serde(rename = "$rubyRef")]
    ruby_ref: Option<String>,
    #[serde(rename = "$typescriptRef")]
    typescript_ref: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTypeOverride {
    #[serde(default)]
    required: Vec<String>,
    #[serde(default)]
    omit: Vec<String>,
    #[serde(rename = "$python", default)]
    python: RawLanguageOverride,
    #[serde(rename = "$typescript", default)]
    typescript: RawLanguageOverride,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLanguageOverride {
    #[serde(rename = "type")]
    type_name: Option<String>,
    #[serde(rename = "fromProto")]
    from_proto: Option<String>,
    #[serde(rename = "toProto")]
    to_proto: Option<String>,
    #[serde(rename = "typeParameters", default)]
    type_parameters: Vec<RawPythonTypeParameter>,
    #[serde(default)]
    fields: IndexMap<String, RawLanguageFieldOverride>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLanguageFieldOverride {
    #[serde(rename = "type")]
    type_name: Option<String>,
    source: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPythonTypeParameter {
    name: String,
    kind: RawPythonTypeParameterKind,
}

#[derive(Debug, Clone, Copy, Deserialize)]
enum RawPythonTypeParameterKind {
    #[serde(rename = "TypeVar")]
    TypeVar,
    #[serde(rename = "TypeVarTuple")]
    TypeVarTuple,
}

impl TryFrom<RawApiSpec> for ApiSpec {
    type Error = Error;

    fn try_from(raw: RawApiSpec) -> Result<Self> {
        let services = raw
            .services
            .into_iter()
            .map(|(service_name, service)| {
                Ok(ServiceSpec {
                    name: service_name,
                    endpoint: service.endpoint,
                    operations: service
                        .operations
                        .into_iter()
                        .map(|(name, operation)| OperationSpec {
                            name,
                            input_refs: operation.input.into_language_map(),
                            output_refs: operation.output.into_language_map(),
                        })
                        .collect(),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let types = raw
            .types
            .into_iter()
            .map(|(type_name, type_override)| {
                let normalized_type_name = type_name.trim_start_matches('.').to_string();
                let python = build_language_override(
                    &normalized_type_name,
                    Language::Python,
                    type_override.python.clone(),
                )?;
                let python_model =
                    build_python_generated_model(&normalized_type_name, &type_override.python)?;
                let typescript_model = build_typescript_generated_model(
                    &normalized_type_name,
                    &type_override.typescript,
                )?;
                Ok((
                    normalized_type_name.clone(),
                    TypeOverrideSpec {
                        required_fields: type_override.required.into_iter().collect(),
                        omitted_fields: type_override.omit.into_iter().collect(),
                        python,
                        python_model,
                        typescript: build_language_override(
                            &normalized_type_name,
                            Language::TypeScript,
                            type_override.typescript.clone(),
                        )?,
                        typescript_model,
                    },
                ))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;

        Ok(Self {
            version: raw.version,
            support: SupportSpec {
                python_file: raw.support.python_file,
                typescript_file: raw.support.typescript_file,
            },
            services,
            types,
        })
    }
}

fn build_python_generated_model(
    type_name: &str,
    raw: &RawLanguageOverride,
) -> Result<PythonGeneratedModelSpec> {
    if raw.type_name.is_some() && !raw.type_parameters.is_empty() {
        return Err(Error::ConflictingTypeOverrideLanguageProperties {
            type_name: type_name.to_string(),
            language: Language::Python,
            property: "typeParameters",
            conflicting_property: "type",
        });
    }
    if raw.type_name.is_some() && !raw.fields.is_empty() {
        return Err(Error::ConflictingTypeOverrideLanguageProperties {
            type_name: type_name.to_string(),
            language: Language::Python,
            property: "fields",
            conflicting_property: "type",
        });
    }

    let mut seen_parameter_names = BTreeSet::new();
    let mut type_parameters = Vec::with_capacity(raw.type_parameters.len());
    for parameter in &raw.type_parameters {
        if !seen_parameter_names.insert(parameter.name.clone()) {
            return Err(Error::DuplicateTypeOverrideLanguageParameter {
                type_name: type_name.to_string(),
                language: Language::Python,
                parameter: parameter.name.clone(),
            });
        }
        type_parameters.push(PythonTypeParameterSpec {
            name: parameter.name.clone(),
            kind: match parameter.kind {
                RawPythonTypeParameterKind::TypeVar => PythonTypeParameterKind::TypeVar,
                RawPythonTypeParameterKind::TypeVarTuple => PythonTypeParameterKind::TypeVarTuple,
            },
        });
    }

    let (field_annotations, field_sources) =
        build_generated_model_fields(type_name, Language::Python, &raw.fields)?;

    Ok(PythonGeneratedModelSpec {
        type_parameters,
        field_annotations,
        field_sources,
    })
}

fn build_typescript_generated_model(
    type_name: &str,
    raw: &RawLanguageOverride,
) -> Result<TypeScriptGeneratedModelSpec> {
    if !raw.type_parameters.is_empty() {
        return Err(Error::UnsupportedLanguageTypeOverrideProperty {
            type_name: type_name.to_string(),
            language: Language::TypeScript,
            property: "typeParameters",
        });
    }
    if raw.type_name.is_some() && !raw.fields.is_empty() {
        return Err(Error::ConflictingTypeOverrideLanguageProperties {
            type_name: type_name.to_string(),
            language: Language::TypeScript,
            property: "fields",
            conflicting_property: "type",
        });
    }

    let (field_annotations, field_sources) =
        build_generated_model_fields(type_name, Language::TypeScript, &raw.fields)?;

    Ok(TypeScriptGeneratedModelSpec {
        field_annotations,
        field_sources,
    })
}

fn build_generated_model_fields(
    type_name: &str,
    language: Language,
    fields: &IndexMap<String, RawLanguageFieldOverride>,
) -> Result<(BTreeMap<String, String>, BTreeMap<String, String>)> {
    let mut field_annotations = BTreeMap::new();
    let mut field_sources = BTreeMap::new();

    for (field_name, field_override) in fields {
        match (&field_override.type_name, &field_override.source) {
            (Some(_), Some(_)) => {
                return Err(Error::ConflictingTypeOverrideLanguageFieldProperties {
                    message: type_name.to_string(),
                    field: field_name.clone(),
                    language,
                    property: "type",
                    conflicting_property: "source",
                });
            }
            (Some(type_name_override), None) => {
                field_annotations.insert(field_name.clone(), type_name_override.clone());
            }
            (None, Some(source)) => {
                field_sources.insert(field_name.clone(), source.clone());
            }
            (None, None) => {
                return Err(Error::IncompleteTypeOverrideLanguageField {
                    type_name: type_name.to_string(),
                    field: field_name.clone(),
                    language,
                });
            }
        }
    }

    Ok((field_annotations, field_sources))
}

fn build_language_override(
    type_name: &str,
    language: Language,
    raw: RawLanguageOverride,
) -> Result<Option<LanguageOverrideSpec>> {
    build_language_override_inner(
        raw,
        || Error::IncompleteTypeLanguageOverride {
            type_name: type_name.to_string(),
            language,
        },
        type_name,
        language,
    )
}

fn build_language_override_inner<F>(
    raw: RawLanguageOverride,
    missing_type_error: F,
    target_name: &str,
    language: Language,
) -> Result<Option<LanguageOverrideSpec>>
where
    F: FnOnce() -> Error,
{
    let has_values = raw.type_name.is_some() || raw.from_proto.is_some() || raw.to_proto.is_some();
    if !has_values {
        return Ok(None);
    }

    let Some(type_name) = raw.type_name else {
        return Err(missing_type_error());
    };

    Ok(Some(LanguageOverrideSpec {
        type_name,
        from_proto: raw.from_proto.unwrap_or_else(|| {
            default_converter_name(target_name, language, ConverterDirection::FromProto)
        }),
        to_proto: raw.to_proto.unwrap_or_else(|| {
            default_converter_name(target_name, language, ConverterDirection::ToProto)
        }),
    }))
}

#[derive(Copy, Clone)]
enum ConverterDirection {
    FromProto,
    ToProto,
}

fn default_converter_name(name: &str, language: Language, direction: ConverterDirection) -> String {
    let name = name
        .rsplit('.')
        .next()
        .expect("converter name source should have a final segment");
    match language {
        Language::Python => match direction {
            ConverterDirection::FromProto => format!("{}_from_proto", name.to_snake_case()),
            ConverterDirection::ToProto => format!("{}_to_proto", name.to_snake_case()),
        },
        Language::TypeScript => match direction {
            ConverterDirection::FromProto => format!("{}FromProto", name.to_lower_camel_case()),
            ConverterDirection::ToProto => format!("{}ToProto", name.to_lower_camel_case()),
        },
        _ => unreachable!("unsupported language override"),
    }
}

fn validate_message_type_override(
    message_name: &str,
    type_override: &TypeOverrideSpec,
    message: &MessageMetadata,
    descriptors: &DescriptorIndex,
    python_usage: MessageUsage,
    typescript_usage: MessageUsage,
) -> Result<()> {
    for field_name in &type_override.required_fields {
        validate_model_required_field(message_name, field_name, message, descriptors)?;
    }
    for field_name in &type_override.omitted_fields {
        validate_model_override_field(message_name, field_name, message)?;
    }
    if let Some(python_model) = type_override.python_generated_model() {
        validate_language_generated_model_fields(
            message_name,
            type_override,
            &python_model.field_annotations,
            &python_model.field_sources,
            message,
            Language::Python,
            python_usage,
        )?;
    }
    if let Some(typescript_model) = type_override.typescript_generated_model() {
        validate_language_generated_model_fields(
            message_name,
            type_override,
            &typescript_model.field_annotations,
            &typescript_model.field_sources,
            message,
            Language::TypeScript,
            typescript_usage,
        )?;
    }
    for field_name in type_override
        .required_fields
        .intersection(&type_override.omitted_fields)
    {
        return Err(Error::ConflictingTypeOverrideField {
            message: message_name.to_string(),
            field: field_name.to_string(),
        });
    }

    Ok(())
}

fn validate_language_generated_model_fields(
    message_name: &str,
    type_override: &TypeOverrideSpec,
    field_annotations: &BTreeMap<String, String>,
    field_sources: &BTreeMap<String, String>,
    message: &MessageMetadata,
    language: Language,
    usage: MessageUsage,
) -> Result<()> {
    for field_name in field_annotations.keys() {
        validate_model_override_field(message_name, field_name, message)?;
        if type_override.omitted_fields.contains(field_name) {
            return Err(Error::OmittedCustomizedTypeOverrideField {
                message: message_name.to_string(),
                field: field_name.to_string(),
                language,
            });
        }
    }

    for field_name in field_sources.keys() {
        validate_model_override_field(message_name, field_name, message)?;
        if type_override.omitted_fields.contains(field_name) {
            return Err(Error::ConflictingTypeOverrideLanguageFieldProperties {
                message: message_name.to_string(),
                field: field_name.to_string(),
                language,
                property: "source",
                conflicting_property: "omit",
            });
        }
        if type_override.required_fields.contains(field_name) {
            return Err(Error::ConflictingTypeOverrideLanguageFieldProperties {
                message: message_name.to_string(),
                field: field_name.to_string(),
                language,
                property: "source",
                conflicting_property: "required",
            });
        }
        if usage.output {
            return Err(Error::UnsupportedSourcedTypeField {
                message: message_name.to_string(),
                field: field_name.to_string(),
                language,
                reason: "sourced fields are only supported on input-only generated models"
                    .to_string(),
            });
        }
    }

    for field_name in field_annotations
        .keys()
        .filter(|field_name| field_sources.contains_key(*field_name))
    {
        return Err(Error::ConflictingTypeOverrideLanguageFieldProperties {
            message: message_name.to_string(),
            field: field_name.to_string(),
            language,
            property: "type",
            conflicting_property: "source",
        });
    }

    Ok(())
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
    if let Some(python_model) = type_override.python_generated_model() {
        if !python_model.type_parameters.is_empty() {
            return Err(Error::UnsupportedLanguageTypeOverrideProperty {
                type_name: enumeration_name.to_string(),
                language: Language::Python,
                property: "typeParameters",
            });
        }
        if !python_model.field_annotations.is_empty() {
            return Err(Error::UnsupportedLanguageTypeOverrideProperty {
                type_name: enumeration_name.to_string(),
                language: Language::Python,
                property: "fields",
            });
        }
    }
    if let Some(typescript_model) = type_override.typescript_generated_model() {
        if !typescript_model.field_annotations.is_empty()
            || !typescript_model.field_sources.is_empty()
        {
            return Err(Error::UnsupportedLanguageTypeOverrideProperty {
                type_name: enumeration_name.to_string(),
                language: Language::TypeScript,
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

impl RawLanguageRefs {
    fn into_language_map(self) -> LanguageRefMap {
        let mut refs = LanguageRefMap::new();

        if let Some(value) = self.dotnet_ref {
            refs.insert(Language::Dotnet, value);
        }
        if let Some(value) = self.go_ref {
            refs.insert(Language::Go, value);
        }
        if let Some(value) = self.java_ref {
            refs.insert(Language::Java, value);
        }
        if let Some(value) = self.python_ref {
            refs.insert(Language::Python, value);
        }
        if let Some(value) = self.ruby_ref {
            refs.insert(Language::Ruby, value);
        }
        if let Some(value) = self.typescript_ref {
            refs.insert(Language::TypeScript, value);
        }

        refs
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::descriptors::DescriptorIndex;
    use crate::error::Error;

    use super::ApiSpec;

    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    #[test]
    fn parses_model_overrides() {
        let yaml = r#"
nexusrpc: 1.0.0
support:
  $pythonFile: python/model_overrides.py
  $typescriptFile: typescript/model_overrides.ts
types:
  temporal.api.common.v1.RetryPolicy:
    $python:
      type: temporalio.common.RetryPolicy
    $typescript:
      type: common.RetryPolicy
  temporal.api.common.v1.Payloads:
    $python:
      type: collections.abc.Sequence[typing.Any]
  temporal.api.activity.v1.ActivityOptions:
    required:
      - retry_policy
  temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest:
    omit:
      - header
    $python:
      typeParameters:
        - name: WorkflowArgs
          kind: TypeVarTuple
      fields:
        namespace:
          source: workflow.info().namespace
        workflow_type:
          type: str | collections.abc.Callable[[typing.Any, *WorkflowArgs], collections.abc.Awaitable[typing.Any]]
        input:
          type: tuple[*WorkflowArgs]
    $typescript:
      fields:
        namespace:
          source: workflow.workflowInfo().namespace
  temporal.api.enums.v1.WorkflowIdReusePolicy:
    $python:
      type: temporalio.common.WorkflowIDReusePolicy
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      RetryPolicyOperation:
        input:
          $pythonRef: temporalio.api.common.v1.RetryPolicy
          $typescriptRef: "@temporalio/api/common/v1.RetryPolicy"
        output:
          $pythonRef: temporalio.api.common.v1.RetryPolicy
          $typescriptRef: "@temporalio/api/common/v1.RetryPolicy"
"#;

        let spec = ApiSpec::parse(yaml, PathBuf::from("inline.yaml")).unwrap();

        assert_eq!(
            spec.support.python_file.as_deref(),
            Some("python/model_overrides.py")
        );
        assert_eq!(
            spec.support.typescript_file.as_deref(),
            Some("typescript/model_overrides.ts")
        );
        let retry_policy = spec
            .type_override("temporal.api.common.v1.RetryPolicy")
            .unwrap();
        assert_eq!(
            retry_policy.python.as_ref().unwrap().type_name,
            "temporalio.common.RetryPolicy"
        );
        assert_eq!(
            retry_policy.typescript.as_ref().unwrap().from_proto,
            "retryPolicyFromProto"
        );
        assert_eq!(
            retry_policy.python.as_ref().unwrap().to_proto,
            "retry_policy_to_proto"
        );

        let activity_options = spec
            .type_override("temporal.api.activity.v1.ActivityOptions")
            .unwrap();
        assert!(activity_options.is_field_required("retry_policy"));
        let signal_with_start = spec
            .type_override(
                "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest",
            )
            .unwrap();
        assert!(signal_with_start.is_field_omitted("header"));
        let python_model = signal_with_start.python_generated_model().unwrap();
        assert_eq!(python_model.type_parameters.len(), 1);
        assert_eq!(python_model.type_parameters[0].name, "WorkflowArgs");
        assert_eq!(
            python_model.field_annotation("workflow_type"),
            Some(
                "str | collections.abc.Callable[[typing.Any, *WorkflowArgs], collections.abc.Awaitable[typing.Any]]"
            )
        );
        assert_eq!(
            python_model.field_annotation("input"),
            Some("tuple[*WorkflowArgs]")
        );
        assert_eq!(
            python_model.field_source("namespace"),
            Some("workflow.info().namespace")
        );
        let typescript_model = signal_with_start.typescript_generated_model().unwrap();
        assert_eq!(
            typescript_model.field_source("namespace"),
            Some("workflow.workflowInfo().namespace")
        );
        let workflow_id_reuse_policy = spec
            .type_override("temporal.api.enums.v1.WorkflowIdReusePolicy")
            .unwrap();
        assert_eq!(
            workflow_id_reuse_policy.python.as_ref().unwrap().type_name,
            "temporalio.common.WorkflowIDReusePolicy"
        );
        assert_eq!(
            workflow_id_reuse_policy.python.as_ref().unwrap().to_proto,
            "workflow_id_reuse_policy_to_proto"
        );
        let payloads = spec
            .type_override("temporal.api.common.v1.Payloads")
            .unwrap();
        assert_eq!(
            payloads.python.as_ref().unwrap().type_name,
            "collections.abc.Sequence[typing.Any]"
        );
        assert_eq!(
            payloads.python.as_ref().unwrap().to_proto,
            "payloads_to_proto"
        );
    }

    #[test]
    fn derives_missing_converter_names() {
        let yaml = r#"
nexusrpc: 1.0.0
types:
  temporal.api.common.v1.RetryPolicy:
    $python:
      type: temporalio.common.RetryPolicy
      fromProto: custom_retry_policy_from_proto
    $typescript:
      type: common.RetryPolicy
      toProto: customRetryPolicyToProto
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      RetryPolicyOperation:
        input:
          $pythonRef: temporalio.api.common.v1.RetryPolicy
          $typescriptRef: "@temporalio/api/common/v1.RetryPolicy"
        output:
          $pythonRef: temporalio.api.common.v1.RetryPolicy
          $typescriptRef: "@temporalio/api/common/v1.RetryPolicy"
"#;

        let spec = ApiSpec::parse(yaml, PathBuf::from("inline.yaml")).unwrap();
        let retry_policy = spec
            .type_override("temporal.api.common.v1.RetryPolicy")
            .unwrap();

        assert_eq!(
            retry_policy.python.as_ref().unwrap().from_proto,
            "custom_retry_policy_from_proto"
        );
        assert_eq!(
            retry_policy.python.as_ref().unwrap().to_proto,
            "retry_policy_to_proto"
        );
        assert_eq!(
            retry_policy.typescript.as_ref().unwrap().from_proto,
            "retryPolicyFromProto"
        );
        assert_eq!(
            retry_policy.typescript.as_ref().unwrap().to_proto,
            "customRetryPolicyToProto"
        );
    }

    #[test]
    fn derives_missing_enum_converter_names() {
        let yaml = r#"
nexusrpc: 1.0.0
types:
  temporal.api.enums.v1.WorkflowIdReusePolicy:
    $python:
      type: temporalio.common.WorkflowIDReusePolicy
      fromProto: custom_workflow_id_reuse_policy_from_proto
    $typescript:
      type: common.WorkflowIdReusePolicy
      toProto: customWorkflowIdReusePolicyToProto
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      SignalWithStartWorkflowExecution:
        input:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
          $typescriptRef: "@temporalio/api/workflowservice/v1.SignalWithStartWorkflowExecutionRequest"
        output:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
          $typescriptRef: "@temporalio/api/workflowservice/v1.SignalWithStartWorkflowExecutionResponse"
"#;

        let spec = ApiSpec::parse(yaml, PathBuf::from("inline.yaml")).unwrap();
        let workflow_id_reuse_policy = spec
            .type_override("temporal.api.enums.v1.WorkflowIdReusePolicy")
            .unwrap();

        assert_eq!(
            workflow_id_reuse_policy.python.as_ref().unwrap().from_proto,
            "custom_workflow_id_reuse_policy_from_proto"
        );
        assert_eq!(
            workflow_id_reuse_policy.python.as_ref().unwrap().to_proto,
            "workflow_id_reuse_policy_to_proto"
        );
        assert_eq!(
            workflow_id_reuse_policy
                .typescript
                .as_ref()
                .unwrap()
                .from_proto,
            "workflowIdReusePolicyFromProto"
        );
        assert_eq!(
            workflow_id_reuse_policy
                .typescript
                .as_ref()
                .unwrap()
                .to_proto,
            "customWorkflowIdReusePolicyToProto"
        );
    }

    #[test]
    fn rejects_language_override_without_type() {
        let yaml = r#"
nexusrpc: 1.0.0
types:
  temporal.api.common.v1.RetryPolicy:
    $python:
      fromProto: retry_policy_from_proto
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      RetryPolicyOperation:
        input:
          $pythonRef: temporalio.api.common.v1.RetryPolicy
        output:
          $pythonRef: temporalio.api.common.v1.RetryPolicy
"#;

        let err = ApiSpec::parse(yaml, PathBuf::from("inline.yaml")).unwrap_err();

        assert!(matches!(
            err,
            Error::IncompleteTypeLanguageOverride {
                type_name,
                language: crate::language::Language::Python,
            } if type_name == "temporal.api.common.v1.RetryPolicy"
        ));
    }

    #[test]
    fn rejects_enum_language_override_without_type() {
        let yaml = r#"
nexusrpc: 1.0.0
types:
  temporal.api.enums.v1.WorkflowIdReusePolicy:
    $python:
      fromProto: workflow_id_reuse_policy_from_proto
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      SignalWithStartWorkflowExecution:
        input:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
        output:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
"#;

        let err = ApiSpec::parse(yaml, PathBuf::from("inline.yaml")).unwrap_err();

        assert!(matches!(
            err,
            Error::IncompleteTypeLanguageOverride {
                type_name,
                language: crate::language::Language::Python,
            } if type_name == "temporal.api.enums.v1.WorkflowIdReusePolicy"
        ));
    }

    #[test]
    fn rejects_model_override_unknown_keys() {
        let yaml = r#"
nexusrpc: 1.0.0
types:
  temporal.api.common.v1.Payloads:
    fields:
      input:
        $python:
          toProto: payloads_to_proto
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      SignalWithStartWorkflowExecution:
        input:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
        output:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
"#;

        let err = ApiSpec::parse(yaml, PathBuf::from("inline.yaml")).unwrap_err();

        assert!(matches!(err, Error::YamlParse { .. }));
    }

    #[test]
    fn rejects_conflicting_typescript_field_customization_properties() {
        let yaml = r#"
nexusrpc: 1.0.0
types:
  temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest:
    $typescript:
      fields:
        namespace:
          type: string
          source: workflow.workflowInfo().namespace
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      SignalWithStartWorkflowExecution:
        input:
          $typescriptRef: "@temporalio/api/workflowservice/v1.SignalWithStartWorkflowExecutionRequest"
        output:
          $typescriptRef: "@temporalio/api/workflowservice/v1.SignalWithStartWorkflowExecutionResponse"
"#;

        let err = ApiSpec::parse(yaml, PathBuf::from("inline.yaml")).unwrap_err();

        assert!(matches!(
            err,
            Error::ConflictingTypeOverrideLanguageFieldProperties {
                message: type_name,
                field,
                language: crate::language::Language::TypeScript,
                property,
                conflicting_property,
            } if type_name == "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest"
                && field == "namespace"
                && property == "type"
                && conflicting_property == "source"
        ));
    }

    #[test]
    fn rejects_python_model_customization_with_whole_type_override() {
        let yaml = r#"
nexusrpc: 1.0.0
types:
  temporal.api.common.v1.Payloads:
    $python:
      type: collections.abc.Sequence[typing.Any]
      typeParameters:
        - name: PayloadArgs
          kind: TypeVarTuple
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      SignalWithStartWorkflowExecution:
        input:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
        output:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
"#;

        let err = ApiSpec::parse(yaml, PathBuf::from("inline.yaml")).unwrap_err();

        assert!(matches!(
            err,
            Error::ConflictingTypeOverrideLanguageProperties {
                type_name,
                language: crate::language::Language::Python,
                property,
                conflicting_property,
            } if type_name == "temporal.api.common.v1.Payloads"
                && property == "typeParameters"
                && conflicting_property == "type"
        ));
    }

    #[test]
    fn validates_required_model_fields() {
        let yaml = r#"
nexusrpc: 1.0.0
types:
  temporal.api.activity.v1.ActivityOptions:
    required:
      - retry_policy
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      ActivityOptionsOperation:
        input:
          $pythonRef: temporalio.api.activity.v1.ActivityOptions
        output:
          $pythonRef: temporalio.api.activity.v1.ActivityOptions
"#;

        let spec = ApiSpec::parse(yaml, PathBuf::from("inline.yaml")).unwrap();
        let descriptors = DescriptorIndex::load(&root().join("descriptors.bin")).unwrap();

        spec.validate_type_overrides(&descriptors).unwrap();
    }

    #[test]
    fn validates_python_model_generics_and_field_annotations() {
        let yaml = r#"
nexusrpc: 1.0.0
types:
  temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest:
    $python:
      typeParameters:
        - name: WorkflowArgs
          kind: TypeVarTuple
      fields:
        workflow_type:
          type: str | collections.abc.Callable[[typing.Any, *WorkflowArgs], collections.abc.Awaitable[typing.Any]]
        input:
          type: tuple[*WorkflowArgs]
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      SignalWithStartWorkflowExecution:
        input:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
        output:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
"#;

        let spec = ApiSpec::parse(yaml, PathBuf::from("inline.yaml")).unwrap();
        let descriptors = DescriptorIndex::load(&root().join("descriptors.bin")).unwrap();

        spec.validate_type_overrides(&descriptors).unwrap();
    }

    #[test]
    fn validates_required_string_model_fields() {
        let yaml = r#"
nexusrpc: 1.0.0
types:
  temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest:
    required:
      - workflow_id
      - signal_name
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      SignalWithStartWorkflowExecution:
        input:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
        output:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
"#;

        let spec = ApiSpec::parse(yaml, PathBuf::from("inline.yaml")).unwrap();
        let descriptors = DescriptorIndex::load(&root().join("descriptors.bin")).unwrap();

        spec.validate_type_overrides(&descriptors).unwrap();
    }

    #[test]
    fn validates_python_payloads_override() {
        let yaml = r#"
nexusrpc: 1.0.0
types:
  temporal.api.common.v1.Payloads:
    $python:
      type: collections.abc.Sequence[typing.Any]
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      SignalWithStartWorkflowExecution:
        input:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
        output:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
"#;

        let spec = ApiSpec::parse(yaml, PathBuf::from("inline.yaml")).unwrap();
        let descriptors = DescriptorIndex::load(&root().join("descriptors.bin")).unwrap();

        spec.validate_type_overrides(&descriptors).unwrap();
    }

    #[test]
    fn validates_enum_overrides() {
        let yaml = r#"
nexusrpc: 1.0.0
types:
  temporal.api.enums.v1.WorkflowIdReusePolicy:
    $python:
      type: temporalio.common.WorkflowIDReusePolicy
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      SignalWithStartWorkflowExecution:
        input:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
        output:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
"#;

        let spec = ApiSpec::parse(yaml, PathBuf::from("inline.yaml")).unwrap();
        let descriptors = DescriptorIndex::load(&root().join("descriptors.bin")).unwrap();

        spec.validate_type_overrides(&descriptors).unwrap();
    }

    #[test]
    fn rejects_enum_type_override_field_policies() {
        let yaml = r#"
nexusrpc: 1.0.0
types:
  temporal.api.enums.v1.WorkflowIdReusePolicy:
    required:
      - ignored
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      SignalWithStartWorkflowExecution:
        input:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
        output:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
"#;

        let spec = ApiSpec::parse(yaml, PathBuf::from("inline.yaml")).unwrap();
        let descriptors = DescriptorIndex::load(&root().join("descriptors.bin")).unwrap();
        let err = spec.validate_type_overrides(&descriptors).unwrap_err();

        assert!(matches!(
            err,
            Error::UnsupportedEnumTypeOverrideProperty {
                enumeration,
                property,
            } if enumeration == "temporal.api.enums.v1.WorkflowIdReusePolicy"
                && property == "required"
        ));
    }

    #[test]
    fn validates_omitted_model_fields() {
        let yaml = r#"
nexusrpc: 1.0.0
types:
  temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest:
    omit:
      - header
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      SignalWithStartWorkflowExecution:
        input:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
        output:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
"#;

        let spec = ApiSpec::parse(yaml, PathBuf::from("inline.yaml")).unwrap();
        let descriptors = DescriptorIndex::load(&root().join("descriptors.bin")).unwrap();

        spec.validate_type_overrides(&descriptors).unwrap();
    }

    #[test]
    fn validates_sourced_model_fields() {
        let yaml = r#"
nexusrpc: 1.0.0
types:
  temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest:
    $python:
      fields:
        namespace:
          source: workflow.info().namespace
    $typescript:
      fields:
        namespace:
          source: workflow.workflowInfo().namespace
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      SignalWithStartWorkflowExecution:
        input:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
          $typescriptRef: "@temporalio/api/workflowservice/v1.SignalWithStartWorkflowExecutionRequest"
        output:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
          $typescriptRef: "@temporalio/api/workflowservice/v1.SignalWithStartWorkflowExecutionResponse"
"#;

        let spec = ApiSpec::parse(yaml, PathBuf::from("inline.yaml")).unwrap();
        let descriptors = DescriptorIndex::load(&root().join("descriptors.bin")).unwrap();

        spec.validate_type_overrides(&descriptors).unwrap();
    }

    #[test]
    fn rejects_omitted_sourced_field() {
        let yaml = r#"
nexusrpc: 1.0.0
types:
  temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest:
    omit:
      - namespace
    $python:
      fields:
        namespace:
          source: workflow.info().namespace
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      SignalWithStartWorkflowExecution:
        input:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
        output:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
"#;

        let spec = ApiSpec::parse(yaml, PathBuf::from("inline.yaml")).unwrap();
        let descriptors = DescriptorIndex::load(&root().join("descriptors.bin")).unwrap();
        let err = spec.validate_type_overrides(&descriptors).unwrap_err();

        assert!(matches!(
            err,
            Error::ConflictingTypeOverrideLanguageFieldProperties {
                message,
                field,
                language: crate::language::Language::Python,
                property,
                conflicting_property,
            } if message == "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest"
                && field == "namespace"
                && property == "source"
                && conflicting_property == "omit"
        ));
    }

    #[test]
    fn rejects_sourced_field_on_output_model() {
        let yaml = r#"
nexusrpc: 1.0.0
types:
  temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse:
    $python:
      fields:
        run_id:
          source: workflow.info().workflow_id
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      SignalWithStartWorkflowExecution:
        input:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
        output:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
"#;

        let spec = ApiSpec::parse(yaml, PathBuf::from("inline.yaml")).unwrap();
        let descriptors = DescriptorIndex::load(&root().join("descriptors.bin")).unwrap();
        let err = spec.validate_type_overrides(&descriptors).unwrap_err();

        assert!(matches!(
            err,
            Error::UnsupportedSourcedTypeField {
                message,
                field,
                language: crate::language::Language::Python,
                reason,
            } if message == "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse"
                && field == "run_id"
                && reason == "sourced fields are only supported on input-only generated models"
        ));
    }

    #[test]
    fn rejects_omitted_python_customized_field() {
        let yaml = r#"
nexusrpc: 1.0.0
types:
  temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest:
    omit:
      - input
    $python:
      fields:
        input:
          type: tuple[*WorkflowArgs]
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      SignalWithStartWorkflowExecution:
        input:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
        output:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
"#;

        let spec = ApiSpec::parse(yaml, PathBuf::from("inline.yaml")).unwrap();
        let descriptors = DescriptorIndex::load(&root().join("descriptors.bin")).unwrap();
        let err = spec.validate_type_overrides(&descriptors).unwrap_err();

        assert!(matches!(
            err,
            Error::OmittedCustomizedTypeOverrideField {
                message,
                field,
                language: crate::language::Language::Python,
            } if message == "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest"
                && field == "input"
        ));
    }

    #[test]
    fn rejects_unknown_type_override() {
        let yaml = r#"
nexusrpc: 1.0.0
types:
  temporal.api.common.v1.DoesNotExist:
    $python:
      type: temporalio.common.RetryPolicy
      fromProto: retry_policy_from_proto
      toProto: retry_policy_to_proto
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      RetryPolicyOperation:
        input:
          $pythonRef: temporalio.api.common.v1.RetryPolicy
        output:
          $pythonRef: temporalio.api.common.v1.RetryPolicy
"#;

        let spec = ApiSpec::parse(yaml, PathBuf::from("inline.yaml")).unwrap();
        let descriptors = DescriptorIndex::load(&root().join("descriptors.bin")).unwrap();
        let err = spec.validate_type_overrides(&descriptors).unwrap_err();

        assert!(matches!(
            err,
            Error::UnknownTypeOverride { type_name }
                if type_name == "temporal.api.common.v1.DoesNotExist"
        ));
    }

    #[test]
    fn rejects_unknown_enum_type_override() {
        let yaml = r#"
nexusrpc: 1.0.0
types:
  temporal.api.enums.v1.DoesNotExist:
    $python:
      type: temporalio.common.WorkflowIDReusePolicy
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      SignalWithStartWorkflowExecution:
        input:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
        output:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
"#;

        let spec = ApiSpec::parse(yaml, PathBuf::from("inline.yaml")).unwrap();
        let descriptors = DescriptorIndex::load(&root().join("descriptors.bin")).unwrap();
        let err = spec.validate_type_overrides(&descriptors).unwrap_err();

        assert!(matches!(
            err,
            Error::UnknownTypeOverride { type_name }
                if type_name == "temporal.api.enums.v1.DoesNotExist"
        ));
    }

    #[test]
    fn rejects_unknown_type_override_field() {
        let yaml = r#"
nexusrpc: 1.0.0
types:
  temporal.api.activity.v1.ActivityOptions:
    required:
      - does_not_exist
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      ActivityOptionsOperation:
        input:
          $pythonRef: temporalio.api.activity.v1.ActivityOptions
        output:
          $pythonRef: temporalio.api.activity.v1.ActivityOptions
"#;

        let spec = ApiSpec::parse(yaml, PathBuf::from("inline.yaml")).unwrap();
        let descriptors = DescriptorIndex::load(&root().join("descriptors.bin")).unwrap();
        let err = spec.validate_type_overrides(&descriptors).unwrap_err();

        assert!(matches!(
            err,
            Error::UnknownTypeOverrideField { message, field }
                if message == "temporal.api.activity.v1.ActivityOptions"
                    && field == "does_not_exist"
        ));
    }

    #[test]
    fn rejects_unknown_omitted_type_override_field() {
        let yaml = r#"
nexusrpc: 1.0.0
types:
  temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest:
    omit:
      - does_not_exist
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      SignalWithStartWorkflowExecution:
        input:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
        output:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
"#;

        let spec = ApiSpec::parse(yaml, PathBuf::from("inline.yaml")).unwrap();
        let descriptors = DescriptorIndex::load(&root().join("descriptors.bin")).unwrap();
        let err = spec.validate_type_overrides(&descriptors).unwrap_err();

        assert!(matches!(
            err,
            Error::UnknownTypeOverrideField { message, field }
                if message == "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest"
                    && field == "does_not_exist"
        ));
    }

    #[test]
    fn rejects_conflicting_type_override_field_policies() {
        let yaml = r#"
nexusrpc: 1.0.0
types:
  temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest:
    required:
      - signal_name
    omit:
      - signal_name
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      SignalWithStartWorkflowExecution:
        input:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
        output:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
"#;

        let spec = ApiSpec::parse(yaml, PathBuf::from("inline.yaml")).unwrap();
        let descriptors = DescriptorIndex::load(&root().join("descriptors.bin")).unwrap();
        let err = spec.validate_type_overrides(&descriptors).unwrap_err();

        assert!(matches!(
            err,
            Error::ConflictingTypeOverrideField { message, field }
                if message == "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest"
                    && field == "signal_name"
        ));
    }

    #[test]
    fn rejects_required_field_without_presence() {
        let yaml = r#"
nexusrpc: 1.0.0
types:
  temporal.api.common.v1.RetryPolicy:
    required:
      - maximum_attempts
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      RetryPolicyOperation:
        input:
          $pythonRef: temporalio.api.common.v1.RetryPolicy
        output:
          $pythonRef: temporalio.api.common.v1.RetryPolicy
"#;

        let spec = ApiSpec::parse(yaml, PathBuf::from("inline.yaml")).unwrap();
        let descriptors = DescriptorIndex::load(&root().join("descriptors.bin")).unwrap();
        let err = spec.validate_type_overrides(&descriptors).unwrap_err();

        assert!(matches!(
            err,
            Error::UnsupportedRequiredTypeField {
                message,
                field,
                reason,
            } if message == "temporal.api.common.v1.RetryPolicy"
                && field == "maximum_attempts"
                && reason == "field must support presence or be a string/bytes scalar"
        ));
    }
}
