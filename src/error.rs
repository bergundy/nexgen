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

    #[error("python generation with `apis` entries requires `--python-support`")]
    MissingPythonSupport,

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

    #[error("service `{service}` api `{api}` input is missing `$pythonConverter`")]
    MissingApiInputPythonConverter { service: String, api: String },

    #[error(
        "service `{service}` api `{api}` input property `{property}` is missing both `type` and `$pythonType`"
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

    #[error("service `{service}` api `{api}` output is missing `$pythonRef`")]
    MissingApiOutputPythonRef { service: String, api: String },

    #[error("service `{service}` api `{api}` output is missing `$pythonConverter`")]
    MissingApiOutputPythonConverter { service: String, api: String },
}
