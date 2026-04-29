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
    pub support: SupportSpec,
    pub services: Vec<ServiceSpec>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SupportSpec {
    pub python_file: Option<String>,
    pub typescript_file: Option<String>,
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
    pub typescript_converter: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApiMethodPropertySpec {
    pub name: String,
    pub type_name: Option<String>,
    pub schema_type: Option<String>,
    pub python_type: Option<String>,
    pub python_converter: Option<String>,
    pub python_output_converter: Option<String>,
    pub python_default: Option<String>,
    pub typescript_name: Option<String>,
    pub typescript_type: Option<String>,
    pub typescript_converter: Option<String>,
    pub typescript_output_converter: Option<String>,
    pub typescript_default: Option<String>,
    pub typescript_positional: Option<bool>,
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
    pub typescript_ref: Option<String>,
    pub typescript_converter: Option<String>,
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
    #[serde(default)]
    support: RawSupportSpec,
    #[serde(default)]
    types: IndexMap<String, RawApiMethodProperty>,
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
    #[serde(rename = "$python", default)]
    python: RawPythonApiMethodInput,
    #[serde(rename = "$typescript", default)]
    typescript: RawTypeScriptApiMethodInput,
}

#[derive(Debug, Default, Clone, Deserialize)]
struct RawPythonApiMethodInput {
    converter: Option<String>,
}

#[derive(Debug, Default, Clone, Deserialize)]
struct RawTypeScriptApiMethodInput {
    converter: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawApiMethodProperty {
    #[serde(rename = "$ref")]
    reference: Option<String>,
    #[serde(rename = "type")]
    schema_type: Option<String>,
    #[serde(rename = "$python", default)]
    python: RawPythonApiMethodProperty,
    #[serde(rename = "$typescript", default)]
    typescript: RawTypeScriptApiMethodProperty,
    positional: Option<bool>,
    #[serde(
        default,
        rename = "default",
        deserialize_with = "deserialize_optional_default_value"
    )]
    default_value: Option<Option<Value>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RawPythonApiMethodProperty {
    #[serde(rename = "type")]
    type_name: Option<String>,
    converter: Option<String>,
    #[serde(rename = "outputConverter")]
    output_converter: Option<String>,
    #[serde(rename = "default")]
    default_expr: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RawTypeScriptApiMethodProperty {
    name: Option<String>,
    #[serde(rename = "type")]
    type_name: Option<String>,
    converter: Option<String>,
    #[serde(rename = "outputConverter")]
    output_converter: Option<String>,
    #[serde(rename = "default")]
    default_expr: Option<String>,
    positional: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct RawApiMethodOutput {
    #[serde(rename = "$python", default)]
    python: RawPythonApiMethodOutput,
    #[serde(rename = "$typescript", default)]
    typescript: RawTypeScriptApiMethodOutput,
}

#[derive(Debug, Default, Clone, Deserialize)]
struct RawPythonApiMethodOutput {
    #[serde(rename = "ref")]
    reference: Option<String>,
    converter: Option<String>,
}

#[derive(Debug, Default, Clone, Deserialize)]
struct RawTypeScriptApiMethodOutput {
    #[serde(rename = "ref")]
    reference: Option<String>,
    converter: Option<String>,
}

impl TryFrom<RawApiSpec> for ApiSpec {
    type Error = Error;

    fn try_from(raw: RawApiSpec) -> Result<Self> {
        let RawApiSpec {
            version,
            support,
            types,
            services,
        } = raw;
        let type_registry = ApiTypeRegistry::new(types);
        let services = services
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
                                build_api_method_property_spec(
                                    &service_name,
                                    &api_name,
                                    &property_name,
                                    &property,
                                    &type_registry,
                                )
                            })
                            .collect::<Result<Vec<_>>>()?;

                        Ok(ApiMethodSpec {
                            name: api_name,
                            operation: api.operation,
                            input: ApiMethodInputSpec {
                                schema_type: api.input.schema_type,
                                properties,
                                python_converter: api.input.python.converter,
                                typescript_converter: api.input.typescript.converter,
                            },
                            output: ApiMethodOutputSpec {
                                python_ref: api.output.python.reference,
                                python_converter: api.output.python.converter,
                                typescript_ref: api.output.typescript.reference,
                                typescript_converter: api.output.typescript.converter,
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
            version,
            support: SupportSpec {
                python_file: support.python_file,
                typescript_file: support.typescript_file,
            },
            services,
        })
    }
}

#[derive(Debug, Default)]
struct ResolvedApiMethodProperty {
    type_name: Option<String>,
    schema_type: Option<String>,
    python_type: Option<String>,
    python_converter: Option<String>,
    python_output_converter: Option<String>,
    python_default: Option<String>,
    typescript_name: Option<String>,
    typescript_type: Option<String>,
    typescript_converter: Option<String>,
    typescript_output_converter: Option<String>,
    typescript_default: Option<String>,
    typescript_positional: Option<bool>,
    default_value: Option<ApiMethodPropertyDefault>,
    positional: bool,
}

#[derive(Debug, Default)]
struct ApiTypeRegistry {
    entries: IndexMap<String, RawApiMethodProperty>,
}

impl ApiTypeRegistry {
    fn new(entries: IndexMap<String, RawApiMethodProperty>) -> Self {
        Self { entries }
    }

    fn resolve<'a>(
        &'a self,
        service: &str,
        api: &str,
        property: &str,
        reference: &str,
    ) -> Result<(&'a str, &'a RawApiMethodProperty)> {
        let referenced_type = parse_api_property_reference(service, api, property, reference)?;
        if let Some(entry) = self.entries.get_key_value(referenced_type.as_str()) {
            return Ok((entry.0.as_str(), entry.1));
        }

        let matches = self
            .entries
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case(&referenced_type))
            .map(|(name, property)| (name.as_str(), property))
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [] => Err(Error::UnknownApiInputPropertyRef {
                service: service.to_string(),
                api: api.to_string(),
                property: property.to_string(),
                reference: reference.to_string(),
            }),
            [(name, property)] => Ok((*name, *property)),
            _ => Err(Error::AmbiguousApiInputPropertyRef {
                service: service.to_string(),
                api: api.to_string(),
                property: property.to_string(),
                reference: reference.to_string(),
                matches: matches
                    .into_iter()
                    .map(|(name, _)| name.to_string())
                    .collect(),
            }),
        }
    }
}

fn build_api_method_property_spec(
    service: &str,
    api: &str,
    property_name: &str,
    property: &RawApiMethodProperty,
    type_registry: &ApiTypeRegistry,
) -> Result<ApiMethodPropertySpec> {
    let mut stack = Vec::new();
    let resolved = resolve_api_method_property(
        service,
        api,
        property_name,
        property,
        type_registry,
        &mut stack,
    )?;
    if resolved.schema_type.is_none()
        && resolved.python_type.is_none()
        && resolved.typescript_type.is_none()
    {
        return Err(Error::MissingApiInputPropertyType {
            service: service.to_string(),
            api: api.to_string(),
            property: property_name.to_string(),
        });
    }

    Ok(ApiMethodPropertySpec {
        name: property_name.to_string(),
        type_name: resolved.type_name,
        schema_type: resolved.schema_type,
        python_type: resolved.python_type,
        python_converter: resolved.python_converter,
        python_output_converter: resolved.python_output_converter,
        python_default: resolved.python_default,
        typescript_name: resolved.typescript_name,
        typescript_type: resolved.typescript_type,
        typescript_converter: resolved.typescript_converter,
        typescript_output_converter: resolved.typescript_output_converter,
        typescript_default: resolved.typescript_default,
        typescript_positional: resolved.typescript_positional,
        default_value: resolved.default_value,
        positional: resolved.positional,
    })
}

fn resolve_api_method_property(
    service: &str,
    api: &str,
    property_name: &str,
    property: &RawApiMethodProperty,
    type_registry: &ApiTypeRegistry,
    stack: &mut Vec<String>,
) -> Result<ResolvedApiMethodProperty> {
    let mut resolved = if let Some(reference) = property.reference.as_deref() {
        let (type_name, referenced_property) =
            type_registry.resolve(service, api, property_name, reference)?;
        if stack.iter().any(|entry| entry == type_name) {
            let mut cycle = stack.clone();
            cycle.push(type_name.to_string());
            return Err(Error::RecursiveApiInputPropertyRef {
                service: service.to_string(),
                api: api.to_string(),
                property: property_name.to_string(),
                reference: reference.to_string(),
                cycle,
            });
        }

        stack.push(type_name.to_string());
        let mut resolved = resolve_api_method_property(
            service,
            api,
            property_name,
            referenced_property,
            type_registry,
            stack,
        )?;
        if resolved.type_name.is_none() {
            resolved.type_name = Some(type_name.to_string());
        }
        stack.pop();
        resolved
    } else {
        ResolvedApiMethodProperty::default()
    };

    if let Some(schema_type) = property.schema_type.as_ref() {
        resolved.schema_type = Some(schema_type.clone());
    }
    if let Some(python_type) = property.python.type_name.as_ref() {
        resolved.python_type = Some(python_type.clone());
    }
    if let Some(python_converter) = property.python.converter.as_ref() {
        resolved.python_converter = Some(python_converter.clone());
    }
    if let Some(python_output_converter) = property.python.output_converter.as_ref() {
        resolved.python_output_converter = Some(python_output_converter.clone());
    }
    if let Some(python_default) = property.python.default_expr.as_ref() {
        resolved.python_default = Some(python_default.clone());
    }
    if let Some(typescript_type) = property.typescript.type_name.as_ref() {
        resolved.typescript_type = Some(typescript_type.clone());
    }
    if let Some(typescript_name) = property.typescript.name.as_ref() {
        resolved.typescript_name = Some(typescript_name.clone());
    }
    if let Some(typescript_converter) = property.typescript.converter.as_ref() {
        resolved.typescript_converter = Some(typescript_converter.clone());
    }
    if let Some(typescript_output_converter) = property.typescript.output_converter.as_ref() {
        resolved.typescript_output_converter = Some(typescript_output_converter.clone());
    }
    if let Some(typescript_default) = property.typescript.default_expr.as_ref() {
        resolved.typescript_default = Some(typescript_default.clone());
    }
    if let Some(typescript_positional) = property.typescript.positional {
        resolved.typescript_positional = Some(typescript_positional);
    }
    if let Some(default_value) = property.default_value.as_ref() {
        resolved.default_value = Some(match default_value {
            None => ApiMethodPropertyDefault::Null,
            Some(value) => parse_api_property_default(service, api, property_name, value.clone())?,
        });
    }
    if let Some(positional) = property.positional {
        resolved.positional = positional;
    }

    Ok(resolved)
}

fn parse_api_property_reference(
    service: &str,
    api: &str,
    property: &str,
    reference: &str,
) -> Result<String> {
    let Some(pointer) = reference.strip_prefix("#/types/") else {
        return Err(Error::InvalidApiInputPropertyRef {
            service: service.to_string(),
            api: api.to_string(),
            property: property.to_string(),
            reference: reference.to_string(),
        });
    };
    if pointer.is_empty() {
        return Err(Error::InvalidApiInputPropertyRef {
            service: service.to_string(),
            api: api.to_string(),
            property: property.to_string(),
            reference: reference.to_string(),
        });
    }

    Ok(pointer.replace("~1", "/").replace("~0", "~"))
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

    use crate::error::Error;

    use super::{ApiMethodPropertyDefault, ApiSpec};

    #[test]
    fn parses_service_apis() {
        let yaml = r#"
nexusrpc: 1.0.0
support:
  $pythonFile: python/support.py
types:
  RetryPolicy:
    $python:
      type: temporalio.common.RetryPolicy | None
      converter: sdk_retry_policy_to_model
      outputConverter: retry_policy_model_to_sdk
    default: null
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
            workflow:
              type: string
              $python:
                type: str | collections.abc.Callable[..., collections.abc.Awaitable[object]]
              $typescript:
                name: workflowTypeOrFunc
              positional: true
            id:
              type: string
            retry_policy:
              $ref: '#/types/retrypolicy'
            signal:
              type: string
              $typescript:
                positional: false
          $python:
            converter: build_signal_with_start_workflow_request
        output:
          $python:
            ref: workflow.ExternalWorkflowHandle[object]
            converter: signal_with_start_workflow_response_to_handle
"#;
        let spec = ApiSpec::parse(yaml, PathBuf::from("inline.yaml")).unwrap();
        assert_eq!(
            spec.support.python_file.as_deref(),
            Some("python/support.py")
        );
        let service = &spec.services[0];
        let api = &service.apis[0];

        assert_eq!(service.name, "WorkflowService");
        assert_eq!(service.apis.len(), 1);
        assert_eq!(api.name, "SignalWithStartWorkflow");
        assert_eq!(api.operation, "SignalWithStartWorkflowExecution");
        assert_eq!(api.input.schema_type, "object");
        assert_eq!(api.input.properties.len(), 4);
        assert_eq!(api.input.properties[0].name, "workflow");
        assert_eq!(
            api.input.properties[0].python_type.as_deref(),
            Some("str | collections.abc.Callable[..., collections.abc.Awaitable[object]]")
        );
        assert_eq!(
            api.input.properties[0].typescript_name.as_deref(),
            Some("workflowTypeOrFunc")
        );
        assert!(api.input.properties[0].positional);
        assert_eq!(api.input.properties[1].name, "id");
        assert_eq!(
            api.input.properties[1].schema_type.as_deref(),
            Some("string")
        );
        assert!(!api.input.properties[1].positional);
        assert_eq!(api.input.properties[2].name, "retry_policy");
        assert_eq!(
            api.input.properties[2].type_name.as_deref(),
            Some("RetryPolicy")
        );
        assert_eq!(api.input.properties[2].schema_type, None);
        assert_eq!(
            api.input.properties[2].python_type.as_deref(),
            Some("temporalio.common.RetryPolicy | None")
        );
        assert_eq!(
            api.input.properties[2].python_converter.as_deref(),
            Some("sdk_retry_policy_to_model")
        );
        assert_eq!(
            api.input.properties[2].python_output_converter.as_deref(),
            Some("retry_policy_model_to_sdk")
        );
        assert_eq!(api.input.properties[2].python_default, None);
        assert_eq!(
            api.input.properties[2].default_value,
            Some(ApiMethodPropertyDefault::Null)
        );
        assert!(!api.input.properties[2].positional);
        assert_eq!(api.input.properties[3].name, "signal");
        assert_eq!(api.input.properties[3].typescript_positional, Some(false));
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

    #[test]
    fn rejects_unknown_type_reference() {
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
            retry_policy:
              $ref: '#/types/RetryPolicy'
          $python:
            converter: build_signal_with_start_workflow_request
        output:
          $python:
            ref: workflow.ExternalWorkflowHandle[object]
            converter: signal_with_start_workflow_response_to_handle
"#;

        let error = ApiSpec::parse(yaml, PathBuf::from("inline.yaml")).unwrap_err();
        assert!(matches!(
            error,
            Error::UnknownApiInputPropertyRef {
                property,
                reference,
                ..
            } if property == "retry_policy" && reference == "#/types/RetryPolicy"
        ));
    }
}
