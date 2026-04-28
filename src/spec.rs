use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde::{Deserialize, Deserializer};
use serde_yaml::Value;

use crate::error::{Error, Result};
use crate::language::Language;

#[derive(Debug, Clone, PartialEq)]
pub struct ApiSpec {
    pub version: String,
    pub services: Vec<ServiceSpec>,
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
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServiceSpec {
    pub name: String,
    pub endpoint: Option<String>,
    pub operations: Vec<OperationSpec>,
    pub apis: Vec<ApiMethodSpec>,
}

impl ServiceSpec {
    pub fn operation(&self, name: &str) -> Option<&OperationSpec> {
        self.operations
            .iter()
            .find(|operation| operation.name == name)
    }
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

#[derive(Debug, Clone, PartialEq)]
pub struct ApiMethodSpec {
    pub name: String,
    pub operation: String,
    pub input: ApiMethodInputSpec,
    pub output: ApiMethodOutputSpec,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApiMethodInputSpec {
    pub schema_type: String,
    pub properties: Vec<ApiMethodPropertySpec>,
    pub python_converter: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApiMethodPropertySpec {
    pub name: String,
    pub schema_type: Option<String>,
    pub python_type: Option<String>,
    pub default_value: Option<ApiMethodPropertyDefault>,
    pub positional: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ApiMethodPropertyDefault {
    Null,
    Bool(bool),
    Integer(i64),
    Float(String),
    String(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApiMethodOutputSpec {
    pub python_ref: Option<String>,
    pub python_converter: Option<String>,
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

#[derive(Debug, Deserialize)]
struct RawApiSpec {
    #[serde(rename = "nexusrpc")]
    version: String,
    services: IndexMap<String, RawService>,
}

#[derive(Debug, Deserialize)]
struct RawService {
    endpoint: Option<String>,
    operations: IndexMap<String, RawOperation>,
    #[serde(default)]
    apis: IndexMap<String, RawApiMethod>,
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

#[derive(Debug, Deserialize)]
struct RawApiMethod {
    operation: String,
    input: RawApiMethodInput,
    output: RawApiMethodOutput,
}

#[derive(Debug, Deserialize)]
struct RawApiMethodInput {
    #[serde(rename = "type")]
    schema_type: String,
    #[serde(default)]
    properties: IndexMap<String, RawApiMethodProperty>,
    #[serde(rename = "$pythonConverter")]
    python_converter: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawApiMethodProperty {
    #[serde(rename = "type")]
    schema_type: Option<String>,
    #[serde(rename = "$pythonType")]
    python_type: Option<String>,
    #[serde(default)]
    positional: bool,
    #[serde(
        default,
        rename = "default",
        deserialize_with = "deserialize_optional_default_value"
    )]
    default_value: Option<Option<Value>>,
}

#[derive(Debug, Deserialize)]
struct RawApiMethodOutput {
    #[serde(rename = "$pythonRef")]
    python_ref: Option<String>,
    #[serde(rename = "$pythonConverter")]
    python_converter: Option<String>,
}

impl TryFrom<RawApiSpec> for ApiSpec {
    type Error = Error;

    fn try_from(raw: RawApiSpec) -> Result<Self> {
        let services = raw
            .services
            .into_iter()
            .map(|(service_name, service)| {
                let apis = service
                    .apis
                    .into_iter()
                    .map(|(api_name, api)| {
                        let properties = api
                            .input
                            .properties
                            .into_iter()
                            .map(|(property_name, property)| {
                                Ok(ApiMethodPropertySpec {
                                    name: property_name.clone(),
                                    schema_type: property.schema_type,
                                    python_type: property.python_type,
                                    default_value: property
                                        .default_value
                                        .map(|value| match value {
                                            None => Ok(ApiMethodPropertyDefault::Null),
                                            Some(value) => parse_api_property_default(
                                                &service_name,
                                                &api_name,
                                                &property_name,
                                                value,
                                            ),
                                        })
                                        .transpose()?,
                                    positional: property.positional,
                                })
                            })
                            .collect::<Result<Vec<_>>>()?;

                        Ok(ApiMethodSpec {
                            name: api_name,
                            operation: api.operation,
                            input: ApiMethodInputSpec {
                                schema_type: api.input.schema_type,
                                properties,
                                python_converter: api.input.python_converter,
                            },
                            output: ApiMethodOutputSpec {
                                python_ref: api.output.python_ref,
                                python_converter: api.output.python_converter,
                            },
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;

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
                    apis,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            version: raw.version,
            services,
        })
    }
}

fn parse_api_property_default(
    service: &str,
    api: &str,
    property: &str,
    value: Value,
) -> Result<ApiMethodPropertyDefault> {
    match value {
        Value::Null => Ok(ApiMethodPropertyDefault::Null),
        Value::Bool(value) => Ok(ApiMethodPropertyDefault::Bool(value)),
        Value::Number(number) => {
            if let Some(value) = number.as_i64() {
                Ok(ApiMethodPropertyDefault::Integer(value))
            } else if let Some(value) = number.as_f64() {
                Ok(ApiMethodPropertyDefault::Float(value.to_string()))
            } else {
                Err(Error::UnsupportedApiInputPropertyDefault {
                    service: service.to_string(),
                    api: api.to_string(),
                    property: property.to_string(),
                })
            }
        }
        Value::String(value) => Ok(ApiMethodPropertyDefault::String(value)),
        _ => Err(Error::UnsupportedApiInputPropertyDefault {
            service: service.to_string(),
            api: api.to_string(),
            property: property.to_string(),
        }),
    }
}

fn deserialize_optional_default_value<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<Option<Value>>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Some(Option::<Value>::deserialize(deserializer)?))
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

    use super::ApiSpec;

    #[test]
    fn parses_service_apis() {
        let yaml = r#"
nexusrpc: 1.0.0
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      SignalWithStartWorkflowExecution:
        input:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
        output:
          $pythonRef: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
    apis:
      SignalWithStartWorkflow:
        operation: SignalWithStartWorkflowExecution
        input:
          type: object
          properties:
            workflow_id:
              type: string
              positional: true
            workflow:
              type: string
              $pythonType: str | collections.abc.Callable[..., collections.abc.Awaitable[object]]
            retry_policy:
              $pythonType: temporalio.common.RetryPolicy | None
              default: null
          $pythonConverter: build_signal_with_start_workflow_request
        output:
          $pythonRef: workflow.ExternalWorkflowHandle[object]
          $pythonConverter: signal_with_start_workflow_response_to_handle
"#;
        let spec = ApiSpec::parse(yaml, PathBuf::from("inline.yaml")).unwrap();
        let service = &spec.services[0];
        let api = &service.apis[0];

        assert_eq!(service.name, "WorkflowService");
        assert_eq!(service.apis.len(), 1);
        assert_eq!(api.name, "SignalWithStartWorkflow");
        assert_eq!(api.operation, "SignalWithStartWorkflowExecution");
        assert_eq!(api.input.schema_type, "object");
        assert_eq!(api.input.properties.len(), 3);
        assert_eq!(api.input.properties[0].name, "workflow_id");
        assert_eq!(
            api.input.properties[0].schema_type.as_deref(),
            Some("string")
        );
        assert!(api.input.properties[0].positional);
        assert_eq!(
            api.input.properties[1].python_type.as_deref(),
            Some("str | collections.abc.Callable[..., collections.abc.Awaitable[object]]")
        );
        assert!(!api.input.properties[1].positional);
        assert_eq!(api.input.properties[2].name, "retry_policy");
        assert_eq!(api.input.properties[2].schema_type, None);
        assert_eq!(
            api.input.properties[2].python_type.as_deref(),
            Some("temporalio.common.RetryPolicy | None")
        );
        assert_eq!(
            api.input.properties[2].default_value,
            Some(super::ApiMethodPropertyDefault::Null)
        );
        assert!(!api.input.properties[2].positional);
        assert_eq!(
            api.input.python_converter.as_deref(),
            Some("build_signal_with_start_workflow_request")
        );
        assert_eq!(
            api.output.python_ref.as_deref(),
            Some("workflow.ExternalWorkflowHandle[object]")
        );
        assert_eq!(
            api.output.python_converter.as_deref(),
            Some("signal_with_start_workflow_response_to_handle")
        );
    }
}
