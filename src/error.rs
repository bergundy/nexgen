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

    #[error(
        "python generation with `apis` entries requires `support.$pythonFile` in the input yaml"
    )]
    MissingPythonSupport,

    #[error(
        "typescript generation with renderable `apis` entries requires `support.$typescriptFile` in the input yaml"
    )]
    MissingTypeScriptSupport,

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

    #[error("service `{service}` api `{api}` references unknown operation `{operation}`")]
    UnknownApiOperation {
        service: String,
        api: String,
        operation: String,
    },

    #[error(
        "service `{service}` api `{api}` input type `{input_type}` is not supported; expected `object`"
    )]
    UnsupportedApiInputType {
        service: String,
        api: String,
        input_type: String,
    },

    #[error("service `{service}` api `{api}` input is missing `$python.converter`")]
    MissingApiInputPythonConverter { service: String, api: String },

    #[error(
        "service `{service}` api `{api}` input property `{property}` has invalid `$ref` `{reference}`; expected `#/types/<name>`"
    )]
    InvalidApiInputPropertyRef {
        service: String,
        api: String,
        property: String,
        reference: String,
    },

    #[error(
        "service `{service}` api `{api}` input property `{property}` references unknown type `{reference}`"
    )]
    UnknownApiInputPropertyRef {
        service: String,
        api: String,
        property: String,
        reference: String,
    },

    #[error(
        "service `{service}` api `{api}` input property `{property}` references ambiguous type `{reference}`; matched {matches:?}"
    )]
    AmbiguousApiInputPropertyRef {
        service: String,
        api: String,
        property: String,
        reference: String,
        matches: Vec<String>,
    },

    #[error(
        "service `{service}` api `{api}` input property `{property}` references type `{reference}` recursively via {cycle:?}"
    )]
    RecursiveApiInputPropertyRef {
        service: String,
        api: String,
        property: String,
        reference: String,
        cycle: Vec<String>,
    },

    #[error(
        "service `{service}` api `{api}` input property `{property}` is missing `type`, `$python.type`, and `$typescript.type`"
    )]
    MissingApiInputPropertyType {
        service: String,
        api: String,
        property: String,
    },

    #[error(
        "service `{service}` api `{api}` input property `{property}` has an unsupported `default`; expected null, bool, number, or string"
    )]
    UnsupportedApiInputPropertyDefault {
        service: String,
        api: String,
        property: String,
    },

    #[error("service `{service}` api `{api}` output is missing `$python.ref`")]
    MissingApiOutputPythonRef { service: String, api: String },

    #[error("service `{service}` api `{api}` output is missing `$python.converter`")]
    MissingApiOutputPythonConverter { service: String, api: String },

    #[error("service `{service}` api `{api}` output is missing `$typescript.ref`")]
    MissingApiOutputTypeScriptRef { service: String, api: String },

    #[error("service `{service}` api `{api}` input is missing `$typescript.converter`")]
    MissingApiInputTypeScriptConverter { service: String, api: String },

    #[error("service `{service}` api `{api}` output is missing `$typescript.converter`")]
    MissingApiOutputTypeScriptConverter { service: String, api: String },
}
