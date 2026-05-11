use std::io;
use std::path::PathBuf;

use thiserror::Error;

use crate::language::Language;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("failed to read `{path}`: {source}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("failed to write `{path}`: {source}")]
    WriteFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("failed to parse YAML from `{path}`: {source}")]
    YamlParse {
        path: PathBuf,
        #[source]
        source: serde_yaml::Error,
    },

    #[error("failed to decode descriptor set from `{path}`: {source}")]
    DescriptorDecode {
        path: PathBuf,
        #[source]
        source: prost::DecodeError,
    },

    #[error("language `{language}` is not implemented yet")]
    UnsupportedLanguage { language: Language },

    #[error("service `{service}` is missing an endpoint")]
    MissingServiceEndpoint { service: String },

    #[error(
        "service `{service}` operation `{operation}` is missing `{language}` {direction} reference"
    )]
    MissingLanguageRef {
        service: String,
        operation: String,
        language: Language,
        direction: &'static str,
    },

    #[error(
        "service `{service}` operation `{operation}` output is missing required `{language}` `type` or `transform` field"
    )]
    IncompleteOperationOutputTransform {
        service: String,
        operation: String,
        language: Language,
    },

    #[error("invalid Python reference `{reference}`: expected `module.path.TypeName`")]
    InvalidPythonRef { reference: String },

    #[error("Python reference `{reference}` could not be resolved to a proto message")]
    UnresolvedPythonRef { reference: String },

    #[error("Python reference `{reference}` is ambiguous; matched {matches:?}")]
    AmbiguousPythonRef {
        reference: String,
        matches: Vec<String>,
    },

    #[error(
        "invalid TypeScript reference `{reference}`: expected `@scope/module.path.TypeName` or `package.path.TypeName`"
    )]
    InvalidTypeScriptRef { reference: String },

    #[error("TypeScript reference `{reference}` could not be resolved to a proto message")]
    UnresolvedTypeScriptRef { reference: String },

    #[error("TypeScript reference `{reference}` is ambiguous; matched {matches:?}")]
    AmbiguousTypeScriptRef {
        reference: String,
        matches: Vec<String>,
    },

    #[error(
        "type override `{type_name}` is missing required `{language}` `type` field; `fromProto` and `toProto` default when omitted"
    )]
    IncompleteTypeLanguageOverride {
        type_name: String,
        language: Language,
    },

    #[error("type override references unknown proto type `{type_name}`")]
    UnknownTypeOverride { type_name: String },

    #[error("type override for `{message}` references unknown field `{field}`")]
    UnknownTypeOverrideField { message: String, field: String },

    #[error("type override for `{message}.{field}` cannot be both required and omitted")]
    ConflictingTypeOverrideField { message: String, field: String },

    #[error(
        "type override for `{message}.{field}` cannot be omitted and customized with `{language}` field annotations"
    )]
    OmittedCustomizedTypeOverrideField {
        message: String,
        field: String,
        language: Language,
    },

    #[error(
        "type override for `{type_name}.{field}` is missing required `{language}` field customization; expected one of `type`, `source`, or `workflow_function`"
    )]
    IncompleteTypeOverrideLanguageField {
        type_name: String,
        field: String,
        language: Language,
    },

    #[error(
        "type override for `{message}.{field}` cannot combine `{language}` field `{property}` with `{conflicting_property}`"
    )]
    ConflictingTypeOverrideLanguageFieldProperties {
        message: String,
        field: String,
        language: Language,
        property: &'static str,
        conflicting_property: &'static str,
    },

    #[error("type override for `{message}.{field}` cannot use `{language}` field `{property}`")]
    UnsupportedTypeOverrideLanguageFieldProperty {
        message: String,
        field: String,
        language: Language,
        property: &'static str,
    },

    #[error(
        "type override for `{message}.{field}` has invalid Python field `{property}`: {reason}"
    )]
    InvalidPythonTypeOverrideField {
        message: String,
        field: String,
        property: &'static str,
        reason: String,
    },

    #[error("type override for `{message}.{field}` cannot be marked required: {reason}")]
    UnsupportedRequiredTypeField {
        message: String,
        field: String,
        reason: String,
    },

    #[error(
        "type override for `{message}.{field}` cannot use `{language}` field `source`: {reason}"
    )]
    UnsupportedSourcedTypeField {
        message: String,
        field: String,
        language: Language,
        reason: String,
    },

    #[error("type override for enum `{enumeration}` cannot use `{property}`")]
    UnsupportedEnumTypeOverrideProperty {
        enumeration: String,
        property: &'static str,
    },

    #[error("type override `{type_name}` cannot use `{language}` `{property}`")]
    UnsupportedLanguageTypeOverrideProperty {
        type_name: String,
        language: Language,
        property: &'static str,
    },

    #[error(
        "type override `{type_name}` cannot combine `{language}` `{property}` with `{conflicting_property}`"
    )]
    ConflictingTypeOverrideLanguageProperties {
        type_name: String,
        language: Language,
        property: &'static str,
        conflicting_property: &'static str,
    },

    #[error("type override `{type_name}` uses duplicate `{language}` type parameter `{parameter}`")]
    DuplicateTypeOverrideLanguageParameter {
        type_name: String,
        language: Language,
        parameter: String,
    },
}
