use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde::Deserialize;
use serde_yaml::{Mapping, Value};

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
    pub fn load_for_language(language: Language, path: &Path) -> Result<Self> {
        let input = fs::read_to_string(path).map_err(|source| Error::ReadFile {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse_for_language(language, &input, path.to_path_buf())
    }

    pub fn parse_for_language(language: Language, input: &str, path: PathBuf) -> Result<Self> {
        let raw_value: Value = serde_yaml::from_str(input).map_err(|source| Error::YamlParse {
            path: path.clone(),
            source,
        })?;
        let projected_value = project_value_for_language(&raw_value, language)?
            .unwrap_or_else(|| Value::Mapping(Mapping::new()));
        let raw: RawApiSpec = serde_yaml::from_value(projected_value)
            .map_err(|source| Error::YamlParse { path, source })?;
        Self::from_raw(raw)
    }

    pub fn type_override(&self, type_name: &str) -> Option<&TypeOverrideSpec> {
        self.types.get(type_name.trim_start_matches('.'))
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
    pub file: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OperationSpec {
    pub name: String,
    pub input_ref: String,
    pub output_ref: String,
    pub output_transform: Option<OperationOutputTransformSpec>,
}

impl OperationSpec {
    pub fn reference(&self, direction: Direction) -> &str {
        match direction {
            Direction::Input => &self.input_ref,
            Direction::Output => &self.output_ref,
        }
    }

    pub fn output_transform(&self) -> Option<&OperationOutputTransformSpec> {
        self.output_transform.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationOutputTransformSpec {
    pub type_name: String,
    pub transform: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TypeOverrideSpec {
    pub required_fields: BTreeSet<String>,
    pub omitted_fields: BTreeSet<String>,
    pub replacement: Option<TypeReplacementSpec>,
    pub generated_model: GeneratedModelSpec,
}

impl TypeOverrideSpec {
    pub fn is_field_required(&self, field_name: &str) -> bool {
        self.required_fields.contains(field_name)
    }

    pub fn is_field_omitted(&self, field_name: &str) -> bool {
        self.omitted_fields.contains(field_name)
    }

    pub fn is_field_hidden(&self, field_name: &str) -> bool {
        self.omitted_fields.contains(field_name) || self.field_source(field_name).is_some()
    }

    pub fn replacement(&self) -> Option<&TypeReplacementSpec> {
        self.replacement.as_ref()
    }

    pub fn generated_model(&self) -> Option<&GeneratedModelSpec> {
        if self.generated_model.is_empty() {
            None
        } else {
            Some(&self.generated_model)
        }
    }

    pub fn field_source(&self, field_name: &str) -> Option<&str> {
        self.generated_model()?.field_source(field_name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeReplacementSpec {
    pub type_name: String,
    pub from_proto: Option<String>,
    pub to_proto: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GeneratedModelSpec {
    pub field_names: BTreeMap<String, String>,
    pub field_annotations: BTreeMap<String, String>,
    pub field_sources: BTreeMap<String, String>,
    pub functions: BTreeMap<String, FunctionFieldSpec>,
}

impl GeneratedModelSpec {
    pub fn is_empty(&self) -> bool {
        self.field_names.is_empty()
            && self.field_annotations.is_empty()
            && self.field_sources.is_empty()
            && self.functions.is_empty()
    }

    pub fn field_name_override(&self, field_name: &str) -> Option<&str> {
        self.field_names.get(field_name).map(String::as_str)
    }

    pub fn field_annotation(&self, field_name: &str) -> Option<&str> {
        self.field_annotations.get(field_name).map(String::as_str)
    }

    pub fn field_source(&self, field_name: &str) -> Option<&str> {
        self.field_sources.get(field_name).map(String::as_str)
    }

    pub fn function(&self, field_name: &str) -> Option<&FunctionFieldSpec> {
        self.functions.get(field_name)
    }

    pub fn function_for_args_field(&self, field_name: &str) -> Option<&FunctionFieldSpec> {
        self.functions
            .values()
            .find(|function| function.args_field == field_name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionFieldSpec {
    pub primary: bool,
    pub result_type: String,
    pub args_field: String,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Direction {
    Input,
    Output,
}

fn project_value_for_language(value: &Value, language: Language) -> Result<Option<Value>> {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {
            Ok(Some(value.clone()))
        }
        Value::Sequence(values) => Ok(Some(Value::Sequence(
            values
                .iter()
                .filter_map(|value| project_value_for_language(value, language).transpose())
                .collect::<Result<Vec<_>>>()?,
        ))),
        Value::Mapping(mapping) => project_mapping_for_language(mapping, language),
        Value::Tagged(tagged) => project_value_for_language(&tagged.value, language),
    }
}

fn project_mapping_for_language(mapping: &Mapping, language: Language) -> Result<Option<Value>> {
    let mut base = Mapping::new();
    let mut overlay: Option<Value> = None;
    let mut saw_regular_key = false;
    let mut saw_language_key = false;

    for (key, value) in mapping {
        let Some(key_str) = key.as_str() else {
            if let Some(projected) = project_value_for_language(value, language)? {
                saw_regular_key = true;
                base.insert(key.clone(), projected);
            }
            continue;
        };

        if let Some(selector_language) = parse_language_selector(key_str)? {
            saw_language_key = true;
            if selector_language == language {
                if let Some(projected_overlay) = project_value_for_language(value, language)? {
                    overlay = Some(match overlay {
                        Some(existing) => merge_projected_values(existing, projected_overlay),
                        None => projected_overlay,
                    });
                }
            }
            continue;
        }

        if let Some(projected) = project_value_for_language(value, language)? {
            saw_regular_key = true;
            base.insert(key.clone(), projected);
        }
    }

    match (saw_regular_key, overlay) {
        (false, None) if saw_language_key => Ok(None),
        (false, Some(overlay_value)) => Ok(Some(overlay_value)),
        (true, Some(overlay_value)) => Ok(Some(merge_projected_values(
            Value::Mapping(base),
            overlay_value,
        ))),
        _ => Ok(Some(Value::Mapping(base))),
    }
}

fn parse_language_selector(key: &str) -> Result<Option<Language>> {
    if !key.starts_with('$') {
        return Ok(None);
    }

    let language = match key {
        "$dotnet" => Language::Dotnet,
        "$go" => Language::Go,
        "$java" => Language::Java,
        "$python" => Language::Python,
        "$ruby" => Language::Ruby,
        "$typescript" => Language::TypeScript,
        _ => {
            return Err(Error::UnknownLanguageSelector {
                selector: key.to_string(),
            });
        }
    };

    Ok(Some(language))
}

fn merge_projected_values(base: Value, overlay: Value) -> Value {
    match (base, overlay) {
        (Value::Mapping(mut base_map), Value::Mapping(overlay_map)) => {
            for (key, overlay_value) in overlay_map {
                let merged_value = match base_map.remove(&key) {
                    Some(base_value) => merge_projected_values(base_value, overlay_value),
                    None => overlay_value,
                };
                base_map.insert(key, merged_value);
            }
            Value::Mapping(base_map)
        }
        (_, overlay_value) => overlay_value,
    }
}

#[derive(Debug, Deserialize)]
struct RawApiSpec {
    #[serde(rename = "nexusrpc")]
    version: String,
    #[serde(default)]
    support: Option<String>,
    #[serde(default)]
    types: IndexMap<String, RawTypeOverride>,
    services: IndexMap<String, RawService>,
}

#[derive(Debug, Deserialize)]
struct RawService {
    endpoint: Option<String>,
    operations: IndexMap<String, RawOperation>,
}

#[derive(Debug, Deserialize)]
struct RawOperation {
    input: String,
    output: RawOperationOutput,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum RawOperationOutput {
    Ref(String),
    Structured(RawOperationOutputObject),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawOperationOutputObject {
    #[serde(rename = "ref")]
    reference: String,
    #[serde(rename = "type")]
    type_name: Option<String>,
    transform: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTypeOverride {
    #[serde(default)]
    required: Vec<String>,
    #[serde(default)]
    omit: Vec<String>,
    #[serde(rename = "type")]
    type_name: Option<String>,
    #[serde(rename = "fromProto")]
    from_proto: Option<String>,
    #[serde(rename = "toProto")]
    to_proto: Option<String>,
    #[serde(default)]
    fields: IndexMap<String, RawLanguageFieldOverride>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLanguageFieldOverride {
    name: Option<String>,
    #[serde(rename = "type")]
    type_name: Option<String>,
    source: Option<String>,
    #[serde(rename = "function", alias = "workflow_function")]
    function: Option<RawFunctionField>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFunctionField {
    #[serde(default)]
    primary: bool,
    result: String,
    #[serde(rename = "argsField")]
    args_field: String,
}

impl ApiSpec {
    fn from_raw(raw: RawApiSpec) -> Result<Self> {
        let services = raw
            .services
            .into_iter()
            .map(|(service_name, service)| {
                let operations = service
                    .operations
                    .into_iter()
                    .map(|(name, operation)| {
                        let (output_ref, output_transform) =
                            build_operation_output(&service_name, &name, &operation.output)?;
                        Ok(OperationSpec {
                            name,
                            input_ref: operation.input,
                            output_ref,
                            output_transform,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;

                Ok(ServiceSpec {
                    name: service_name,
                    endpoint: service.endpoint,
                    operations,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let types = raw
            .types
            .into_iter()
            .map(|(type_name, type_override)| {
                let normalized_type_name = type_name.trim_start_matches('.').to_string();
                let replacement = build_type_replacement(&normalized_type_name, &type_override)?;
                let generated_model = build_generated_model(&normalized_type_name, &type_override)?;
                Ok((
                    normalized_type_name.clone(),
                    TypeOverrideSpec {
                        required_fields: type_override.required.into_iter().collect(),
                        omitted_fields: type_override.omit.into_iter().collect(),
                        replacement,
                        generated_model,
                    },
                ))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;

        Ok(Self {
            version: raw.version,
            support: SupportSpec { file: raw.support },
            services,
            types,
        })
    }
}

fn build_generated_model(type_name: &str, raw: &RawTypeOverride) -> Result<GeneratedModelSpec> {
    if raw.type_name.is_some() && !raw.fields.is_empty() {
        return Err(Error::ConflictingTypeOverrideProperties {
            type_name: type_name.to_string(),
            property: "fields",
            conflicting_property: "type",
        });
    }

    let mut field_annotations = BTreeMap::new();
    let mut field_names = BTreeMap::new();
    let mut field_sources = BTreeMap::new();
    let mut functions = BTreeMap::new();

    for (field_name, field_override) in &raw.fields {
        if field_override.type_name.is_some() && field_override.source.is_some() {
            return Err(Error::ConflictingTypeOverrideFieldProperties {
                message: type_name.to_string(),
                field: field_name.clone(),
                property: "type",
                conflicting_property: "source",
            });
        }
        if field_override.type_name.is_some() && field_override.function.is_some() {
            return Err(Error::ConflictingTypeOverrideFieldProperties {
                message: type_name.to_string(),
                field: field_name.clone(),
                property: "type",
                conflicting_property: "function",
            });
        }
        if field_override.source.is_some() && field_override.function.is_some() {
            return Err(Error::ConflictingTypeOverrideFieldProperties {
                message: type_name.to_string(),
                field: field_name.clone(),
                property: "source",
                conflicting_property: "function",
            });
        }
        if field_override.name.is_some() && field_override.source.is_some() {
            return Err(Error::ConflictingTypeOverrideFieldProperties {
                message: type_name.to_string(),
                field: field_name.clone(),
                property: "name",
                conflicting_property: "source",
            });
        }

        if let Some(name) = &field_override.name {
            field_names.insert(field_name.clone(), name.clone());
        }
        if let Some(type_name_override) = &field_override.type_name {
            field_annotations.insert(field_name.clone(), type_name_override.clone());
        }
        if let Some(source) = &field_override.source {
            field_sources.insert(field_name.clone(), source.clone());
        }
        if let Some(function) = &field_override.function {
            functions.insert(
                field_name.clone(),
                FunctionFieldSpec {
                    primary: function.primary,
                    result_type: function.result.clone(),
                    args_field: function.args_field.clone(),
                },
            );
        }

        if field_override.name.is_none()
            && field_override.type_name.is_none()
            && field_override.source.is_none()
            && field_override.function.is_none()
        {
            return Err(Error::IncompleteTypeOverrideField {
                type_name: type_name.to_string(),
                field: field_name.clone(),
            });
        }
    }

    Ok(GeneratedModelSpec {
        field_names,
        field_annotations,
        field_sources,
        functions,
    })
}

fn build_operation_output(
    service_name: &str,
    operation_name: &str,
    output: &RawOperationOutput,
) -> Result<(String, Option<OperationOutputTransformSpec>)> {
    match output {
        RawOperationOutput::Ref(reference) => Ok((reference.clone(), None)),
        RawOperationOutput::Structured(output) => Ok((
            output.reference.clone(),
            build_operation_output_transform(service_name, operation_name, output)?,
        )),
    }
}

fn build_operation_output_transform(
    service_name: &str,
    operation_name: &str,
    raw: &RawOperationOutputObject,
) -> Result<Option<OperationOutputTransformSpec>> {
    let has_values = raw.type_name.is_some() || raw.transform.is_some();
    if !has_values {
        return Ok(None);
    }

    let (Some(type_name), Some(transform)) = (raw.type_name.clone(), raw.transform.clone()) else {
        return Err(Error::IncompleteOperationOutputTransform {
            service: service_name.to_string(),
            operation: operation_name.to_string(),
        });
    };

    Ok(Some(OperationOutputTransformSpec {
        type_name,
        transform,
    }))
}

fn build_type_replacement(
    type_name: &str,
    raw: &RawTypeOverride,
) -> Result<Option<TypeReplacementSpec>> {
    let has_values = raw.type_name.is_some() || raw.from_proto.is_some() || raw.to_proto.is_some();
    if !has_values {
        return Ok(None);
    }

    let Some(type_name_override) = raw.type_name.clone() else {
        return Err(Error::IncompleteTypeOverride {
            type_name: type_name.to_string(),
        });
    };

    Ok(Some(TypeReplacementSpec {
        type_name: type_name_override,
        from_proto: raw.from_proto.clone(),
        to_proto: raw.to_proto.clone(),
    }))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::descriptors::DescriptorIndex;
    use crate::error::Error;
    use crate::language::Language;

    use super::{ApiSpec, Direction};

    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    fn descriptors() -> DescriptorIndex {
        DescriptorIndex::load(&root().join("descriptors.bin")).unwrap()
    }

    fn parse(language: Language, yaml: &str) -> ApiSpec {
        ApiSpec::parse_for_language(language, yaml, PathBuf::from("inline.yaml")).unwrap()
    }

    fn validate(language: Language, yaml: &str) -> Result<(), Error> {
        let spec = parse(language, yaml);
        let descriptors = descriptors();
        crate::validation::validate_type_overrides(&spec, &descriptors, language)
    }

    #[test]
    fn projects_language_specific_yaml() {
        let yaml = r#"
nexusrpc: 1.0.0
support:
  $python: python-validation/model_overrides.py
  $typescript: typescript-validation/model_overrides.ts
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
    fields:
      workflow_type:
        name: workflow
    $python:
      fields:
        namespace:
          source: workflow.info().namespace
        workflow_type:
          function:
            primary: true
            result: collections.abc.Awaitable[typing.Any]
            argsField: input
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
      SignalWithStartWorkflowExecution:
        input:
          $python: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
          $typescript: "@temporalio/api/workflowservice/v1.SignalWithStartWorkflowExecutionRequest"
        output:
          ref:
            $python: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
            $typescript: "@temporalio/api/workflowservice/v1.SignalWithStartWorkflowExecutionResponse"
          $python:
            type: workflow.ExternalWorkflowHandle[typing.Any]
            transform: workflow.get_external_workflow_handle(request.workflow_id, run_id=result.run_id)
          $typescript:
            type: workflow.ExternalWorkflowHandle
            transform: workflow.getExternalWorkflowHandle(request.workflowId, result.runId ?? undefined)
      RetryPolicyOperation:
        input:
          $python: temporalio.api.common.v1.RetryPolicy
          $typescript: "@temporalio/api/common/v1.RetryPolicy"
        output:
          ref:
            $python: temporalio.api.common.v1.RetryPolicy
            $typescript: "@temporalio/api/common/v1.RetryPolicy"
"#;

        let python_spec = parse(Language::Python, yaml);
        let typescript_spec = parse(Language::TypeScript, yaml);

        assert_eq!(
            python_spec.support.file.as_deref(),
            Some("python-validation/model_overrides.py")
        );
        assert_eq!(
            typescript_spec.support.file.as_deref(),
            Some("typescript-validation/model_overrides.ts")
        );

        let python_retry_policy = python_spec
            .type_override("temporal.api.common.v1.RetryPolicy")
            .unwrap();
        assert_eq!(
            python_retry_policy.replacement().unwrap().type_name,
            "temporalio.common.RetryPolicy"
        );
        assert_eq!(python_retry_policy.replacement().unwrap().to_proto, None);

        let typescript_retry_policy = typescript_spec
            .type_override("temporal.api.common.v1.RetryPolicy")
            .unwrap();
        assert_eq!(
            typescript_retry_policy.replacement().unwrap().type_name,
            "common.RetryPolicy"
        );
        assert_eq!(
            typescript_retry_policy.replacement().unwrap().from_proto,
            None
        );

        let activity_options = python_spec
            .type_override("temporal.api.activity.v1.ActivityOptions")
            .unwrap();
        assert!(activity_options.is_field_required("retry_policy"));

        let python_signal_with_start = python_spec
            .type_override(
                "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest",
            )
            .unwrap();
        assert!(python_signal_with_start.is_field_omitted("header"));
        let python_model = python_signal_with_start.generated_model().unwrap();
        assert_eq!(
            python_model.field_name_override("workflow_type"),
            Some("workflow")
        );
        assert_eq!(
            python_model.field_source("namespace"),
            Some("workflow.info().namespace")
        );
        let function = python_model.function("workflow_type").unwrap();
        assert!(function.primary);
        assert_eq!(
            function.result_type,
            "collections.abc.Awaitable[typing.Any]"
        );
        assert_eq!(function.args_field, "input");

        let typescript_model = typescript_spec
            .type_override(
                "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest",
            )
            .unwrap()
            .generated_model()
            .unwrap();
        assert_eq!(
            typescript_model.field_source("namespace"),
            Some("workflow.workflowInfo().namespace")
        );

        let python_operation = python_spec
            .services
            .first()
            .unwrap()
            .operation("SignalWithStartWorkflowExecution")
            .unwrap();
        assert_eq!(
            python_operation.reference(Direction::Input),
            "temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest"
        );
        assert_eq!(
            python_operation.reference(Direction::Output),
            "temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse"
        );
        assert_eq!(
            python_operation.output_transform().unwrap().type_name,
            "workflow.ExternalWorkflowHandle[typing.Any]"
        );

        let typescript_operation = typescript_spec
            .services
            .first()
            .unwrap()
            .operation("SignalWithStartWorkflowExecution")
            .unwrap();
        assert_eq!(
            typescript_operation.reference(Direction::Input),
            "@temporalio/api/workflowservice/v1.SignalWithStartWorkflowExecutionRequest"
        );
        assert_eq!(
            typescript_operation.reference(Direction::Output),
            "@temporalio/api/workflowservice/v1.SignalWithStartWorkflowExecutionResponse"
        );
        assert_eq!(
            typescript_operation.output_transform().unwrap().transform,
            "workflow.getExternalWorkflowHandle(request.workflowId, result.runId ?? undefined)"
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
          $python: temporalio.api.common.v1.RetryPolicy
          $typescript: "@temporalio/api/common/v1.RetryPolicy"
        output:
          ref:
            $python: temporalio.api.common.v1.RetryPolicy
            $typescript: "@temporalio/api/common/v1.RetryPolicy"
"#;

        let python_spec = parse(Language::Python, yaml);
        let python_override = python_spec
            .type_override("temporal.api.common.v1.RetryPolicy")
            .unwrap();
        let typescript_spec = parse(Language::TypeScript, yaml);
        let typescript_override = typescript_spec
            .type_override("temporal.api.common.v1.RetryPolicy")
            .unwrap();

        assert_eq!(
            python_override.replacement().unwrap().from_proto.as_deref(),
            Some("custom_retry_policy_from_proto")
        );
        assert_eq!(python_override.replacement().unwrap().to_proto, None);
        assert_eq!(typescript_override.replacement().unwrap().from_proto, None);
        assert_eq!(
            typescript_override
                .replacement()
                .unwrap()
                .to_proto
                .as_deref(),
            Some("customRetryPolicyToProto")
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
          $python: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
          $typescript: "@temporalio/api/workflowservice/v1.SignalWithStartWorkflowExecutionRequest"
        output:
          ref:
            $python: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
            $typescript: "@temporalio/api/workflowservice/v1.SignalWithStartWorkflowExecutionResponse"
"#;

        let python_spec = parse(Language::Python, yaml);
        let python_override = python_spec
            .type_override("temporal.api.enums.v1.WorkflowIdReusePolicy")
            .unwrap();
        let typescript_spec = parse(Language::TypeScript, yaml);
        let typescript_override = typescript_spec
            .type_override("temporal.api.enums.v1.WorkflowIdReusePolicy")
            .unwrap();

        assert_eq!(
            python_override.replacement().unwrap().from_proto.as_deref(),
            Some("custom_workflow_id_reuse_policy_from_proto")
        );
        assert_eq!(python_override.replacement().unwrap().to_proto, None);
        assert_eq!(typescript_override.replacement().unwrap().from_proto, None);
        assert_eq!(
            typescript_override
                .replacement()
                .unwrap()
                .to_proto
                .as_deref(),
            Some("customWorkflowIdReusePolicyToProto")
        );
    }

    #[test]
    fn rejects_unknown_language_selector() {
        let yaml = r#"
nexusrpc: 1.0.0
support:
  $wat: nope
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      RetryPolicyOperation:
        input: temporalio.api.common.v1.RetryPolicy
        output:
          ref: temporalio.api.common.v1.RetryPolicy
"#;

        let err = ApiSpec::parse_for_language(Language::Python, yaml, PathBuf::from("inline.yaml"))
            .unwrap_err();

        assert!(matches!(
            err,
            Error::UnknownLanguageSelector { selector } if selector == "$wat"
        ));
    }

    #[test]
    fn rejects_incomplete_operation_output_transform() {
        let yaml = r#"
nexusrpc: 1.0.0
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      SignalWithStartWorkflowExecution:
        input: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
        output:
          ref: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
          type: workflow.ExternalWorkflowHandle[typing.Any]
"#;

        let err = ApiSpec::parse_for_language(Language::Python, yaml, PathBuf::from("inline.yaml"))
            .unwrap_err();

        assert!(matches!(
            err,
            Error::IncompleteOperationOutputTransform { service, operation }
                if service == "WorkflowService" && operation == "SignalWithStartWorkflowExecution"
        ));
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
        input: temporalio.api.common.v1.RetryPolicy
        output:
          ref: temporalio.api.common.v1.RetryPolicy
"#;

        let err = ApiSpec::parse_for_language(Language::Python, yaml, PathBuf::from("inline.yaml"))
            .unwrap_err();

        assert!(matches!(
            err,
            Error::IncompleteTypeOverride { type_name }
                if type_name == "temporal.api.common.v1.RetryPolicy"
        ));
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
        input: "@temporalio/api/workflowservice/v1.SignalWithStartWorkflowExecutionRequest"
        output:
          ref: "@temporalio/api/workflowservice/v1.SignalWithStartWorkflowExecutionResponse"
"#;

        let err =
            ApiSpec::parse_for_language(Language::TypeScript, yaml, PathBuf::from("inline.yaml"))
                .unwrap_err();

        assert!(matches!(
            err,
            Error::ConflictingTypeOverrideFieldProperties {
                message,
                field,
                property,
                conflicting_property,
            } if message == "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest"
                && field == "namespace"
                && property == "type"
                && conflicting_property == "source"
        ));
    }

    #[test]
    fn rejects_removed_type_parameters_property() {
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
        input: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
        output:
          ref: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
"#;

        let err = ApiSpec::parse_for_language(Language::Python, yaml, PathBuf::from("inline.yaml"))
            .unwrap_err();

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
        input: temporalio.api.activity.v1.ActivityOptions
        output:
          ref: temporalio.api.activity.v1.ActivityOptions
"#;

        validate(Language::Python, yaml).unwrap();
    }

    #[test]
    fn validates_python_function_fields() {
        let yaml = r#"
nexusrpc: 1.0.0
types:
  temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest:
    $python:
      fields:
        workflow_type:
          function:
            primary: true
            result: collections.abc.Awaitable[typing.Any]
            argsField: input
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      SignalWithStartWorkflowExecution:
        input: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
        output:
          ref: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
"#;

        validate(Language::Python, yaml).unwrap();
    }

    #[test]
    fn parses_secondary_function_fields() {
        let yaml = r#"
nexusrpc: 1.0.0
types:
  temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest:
    $python:
      fields:
        signal_name:
          function:
            result: None | collections.abc.Awaitable[None]
            argsField: signal_input
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      SignalWithStartWorkflowExecution:
        input: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
        output:
          ref: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
"#;

        let spec = parse(Language::Python, yaml);
        let function = spec
            .type_override(
                "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest",
            )
            .unwrap()
            .generated_model()
            .unwrap()
            .function("signal_name")
            .unwrap();

        assert!(!function.primary);
        assert_eq!(
            function.result_type,
            "None | collections.abc.Awaitable[None]"
        );
        assert_eq!(function.args_field, "signal_input");
    }

    #[test]
    fn rejects_python_function_unknown_args_field() {
        let yaml = r#"
nexusrpc: 1.0.0
types:
  temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest:
    $python:
      fields:
        workflow_type:
          function:
            primary: true
            result: collections.abc.Awaitable[typing.Any]
            argsField: missing_input
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      SignalWithStartWorkflowExecution:
        input: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
        output:
          ref: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
"#;

        let err = validate(Language::Python, yaml).unwrap_err();
        assert!(matches!(
            err,
            Error::UnknownTypeOverrideField { message, field }
                if message == "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest"
                    && field == "missing_input"
        ));
    }

    #[test]
    fn rejects_conflicting_generated_field_aliases() {
        let yaml = r#"
nexusrpc: 1.0.0
types:
  temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest:
    fields:
      workflow_type:
        name: workflow
      workflow_id:
        name: workflow
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      SignalWithStartWorkflowExecution:
        input: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
        output:
          ref: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
"#;

        let err = validate(Language::Python, yaml).unwrap_err();
        match err {
            Error::InvalidTypeOverrideField {
                message,
                field,
                property,
                ..
            } => {
                assert_eq!(
                    message,
                    "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest"
                );
                assert_eq!(property, "name");
                assert!(field == "workflow_id" || field == "workflow_type");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn rejects_removed_function_args_property() {
        let yaml = r#"
nexusrpc: 1.0.0
types:
  temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest:
    $python:
      fields:
        workflow_type:
          function:
            primary: true
            args: WorkflowArgs
            result: collections.abc.Awaitable[typing.Any]
            argsField: input
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      SignalWithStartWorkflowExecution:
        input: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
        output:
          ref: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
"#;

        let err = ApiSpec::parse_for_language(Language::Python, yaml, PathBuf::from("inline.yaml"))
            .unwrap_err();
        assert!(matches!(err, Error::YamlParse { .. }));
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
          $python: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
          $typescript: "@temporalio/api/workflowservice/v1.SignalWithStartWorkflowExecutionRequest"
        output:
          ref:
            $python: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
            $typescript: "@temporalio/api/workflowservice/v1.SignalWithStartWorkflowExecutionResponse"
"#;

        validate(Language::Python, yaml).unwrap();
        validate(Language::TypeScript, yaml).unwrap();
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
        input: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
        output:
          ref: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
"#;

        let err = validate(Language::Python, yaml).unwrap_err();
        assert!(matches!(
            err,
            Error::UnsupportedSourcedTypeField {
                message,
                field,
                reason,
            } if message == "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse"
                && field == "run_id"
                && reason == "sourced fields are only supported on input-only generated models"
        ));
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
        input: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
        output:
          ref: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
"#;

        validate(Language::Python, yaml).unwrap();
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
        input: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
        output:
          ref: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
"#;

        let err = validate(Language::Python, yaml).unwrap_err();
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
    fn rejects_unknown_type_override() {
        let yaml = r#"
nexusrpc: 1.0.0
types:
  temporal.api.common.v1.DoesNotExist:
    $python:
      type: temporalio.common.RetryPolicy
services:
  WorkflowService:
    endpoint: __temporal_system
    operations:
      RetryPolicyOperation:
        input: temporalio.api.common.v1.RetryPolicy
        output:
          ref: temporalio.api.common.v1.RetryPolicy
"#;

        let err = validate(Language::Python, yaml).unwrap_err();
        assert!(matches!(
            err,
            Error::UnknownTypeOverride { type_name }
                if type_name == "temporal.api.common.v1.DoesNotExist"
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
        input: temporalio.api.activity.v1.ActivityOptions
        output:
          ref: temporalio.api.activity.v1.ActivityOptions
"#;

        let err = validate(Language::Python, yaml).unwrap_err();
        assert!(matches!(
            err,
            Error::UnknownTypeOverrideField { message, field }
                if message == "temporal.api.activity.v1.ActivityOptions"
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
        input: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest
        output:
          ref: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse
"#;

        let err = validate(Language::Python, yaml).unwrap_err();
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
        input: temporalio.api.common.v1.RetryPolicy
        output:
          ref: temporalio.api.common.v1.RetryPolicy
"#;

        let err = validate(Language::Python, yaml).unwrap_err();
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
