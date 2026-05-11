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
        for (type_name, type_override) in &self.types {
            if let Some(message) = descriptors.message(type_name) {
                validate_message_type_override(type_name, type_override, message, descriptors)?;
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
    pub typescript: Option<LanguageOverrideSpec>,
}

impl TypeOverrideSpec {
    pub fn is_field_required(&self, field_name: &str) -> bool {
        self.required_fields.contains(field_name)
    }

    pub fn is_field_omitted(&self, field_name: &str) -> bool {
        self.omitted_fields.contains(field_name)
    }

    pub fn language_override(&self, language: Language) -> Option<&LanguageOverrideSpec> {
        match language {
            Language::Python => self.python.as_ref(),
            Language::TypeScript => self.typescript.as_ref(),
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
struct RawLanguageOverride {
    #[serde(rename = "type")]
    type_name: Option<String>,
    #[serde(rename = "fromProto")]
    from_proto: Option<String>,
    #[serde(rename = "toProto")]
    to_proto: Option<String>,
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
                Ok((
                    normalized_type_name.clone(),
                    TypeOverrideSpec {
                        required_fields: type_override.required.into_iter().collect(),
                        omitted_fields: type_override.omit.into_iter().collect(),
                        python: build_language_override(
                            &normalized_type_name,
                            Language::Python,
                            type_override.python.clone(),
                        )?,
                        typescript: build_language_override(
                            &normalized_type_name,
                            Language::TypeScript,
                            type_override.typescript.clone(),
                        )?,
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
) -> Result<()> {
    for field_name in &type_override.required_fields {
        validate_model_required_field(message_name, field_name, message, descriptors)?;
    }
    for field_name in &type_override.omitted_fields {
        validate_model_override_field(message_name, field_name, message)?;
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
