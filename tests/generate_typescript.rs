use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use nexus_api_gen::generate_to_string;

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn sample_input_path(root: &std::path::Path) -> PathBuf {
    root.join("examples/input.wit")
}

fn sample_typescript_output_path(root: &std::path::Path) -> PathBuf {
    root.join("examples/typescript-validation/output.ts")
}

fn prepend_path(path: &std::path::Path) -> String {
    let existing = std::env::var("PATH").unwrap_or_default();
    format!("{}:{existing}", path.display())
}

fn generate_formatted_typescript_output(root: &std::path::Path, output_path: &std::path::Path) {
    let prettier_bin_dir = root.join("examples/typescript-validation/node_modules/.bin");
    let status = Command::new(env!("CARGO_BIN_EXE_nexus-api-gen"))
        .env("PATH", prepend_path(&prettier_bin_dir))
        .args([
            "generate",
            "--lang",
            "typescript",
            "--input",
            sample_input_path(root).to_str().unwrap(),
            "--descriptors",
            root.join("descriptors.bin").to_str().unwrap(),
            "--output",
            output_path.to_str().unwrap(),
            "--format",
        ])
        .status()
        .unwrap();

    assert!(status.success());
}

fn unique_output_path(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("nexus-api-gen-{label}-{unique}.ts"))
}

#[test]
fn sample_typescript_generation_matches_checked_in_output() {
    let root = project_root();
    let output_path = unique_output_path("sample");
    generate_formatted_typescript_output(&root, &output_path);
    let rendered = fs::read_to_string(&output_path).unwrap();
    let expected = fs::read_to_string(sample_typescript_output_path(&root)).unwrap();

    assert_eq!(rendered, expected);

    fs::remove_file(output_path).unwrap();
}

#[test]
fn cli_generates_typescript_file() {
    let root = project_root();
    let output_path = unique_output_path("cli");

    generate_formatted_typescript_output(&root, &output_path);

    let rendered = fs::read_to_string(&output_path).unwrap();
    let expected = fs::read_to_string(sample_typescript_output_path(&root)).unwrap();
    assert_eq!(rendered, expected);

    fs::remove_file(output_path).unwrap();
}

#[test]
fn typescript_validation_app_type_checks() {
    let root = project_root();
    let example_dir = root.join("examples/typescript-validation");

    if !example_dir.join("node_modules").exists() {
        let install_status = Command::new("npm")
            .current_dir(&example_dir)
            .args(["install", "--no-fund", "--no-audit"])
            .status()
            .unwrap();
        assert!(install_status.success());
    }

    let build_status = Command::new("npm")
        .current_dir(&example_dir)
        .args(["run", "build-output"])
        .status()
        .unwrap();
    assert!(build_status.success());

    let typecheck_status = Command::new("npm")
        .current_dir(&example_dir)
        .args(["run", "typecheck"])
        .status()
        .unwrap();
    assert!(typecheck_status.success());
}

#[test]
fn typescript_renders_required_fields_and_custom_message_types() {
    let root = project_root();
    let rendered = generate_to_string(
        nexus_api_gen::language::Language::TypeScript,
        sample_input_path(&root),
        root.join("descriptors.bin"),
    )
    .unwrap();

    assert!(rendered.contains("type _RequestWithFunctionField<"));
    assert!(rendered.contains("type _RequestWithArgumentsField<"));
    assert!(rendered.contains("type SignalWithStartWorkflowExecutionRequestBase = {"));
    assert!(rendered.contains("export type SignalWithStartWorkflowExecutionRequest<"));
    assert!(rendered.contains(
        "WorkflowFn extends (...args: any[]) => Promise<any> = (...args: any[]) => Promise<any>,"
    ));
    assert!(rendered.contains(
        "SignalValue extends workflow.SignalDefinition<any[]> = workflow.SignalDefinition<any[]>"
    ));
    assert!(rendered.contains("> = _RequestWithArgumentsField<"));
    assert!(
        rendered.contains(
            "SignalValue extends workflow.SignalDefinition<infer Args, any> ? Args : never"
        )
    );
    assert!(rendered.contains(
        "_RequestWithFunctionField<WorkflowFn, SignalWithStartWorkflowExecutionRequestBase, \"workflow\", \"input\">"
    ));
    assert!(rendered.contains("workflowId: string;"));
    assert!(rendered.contains("taskQueue: string;"));
    assert!(rendered.contains("workflowRunTimeout?: common.Duration;"));
    assert!(rendered.contains("workflowIdReusePolicy?: common.WorkflowIdReusePolicy;"));
    assert!(rendered.contains("workflowIdConflictPolicy?: common.WorkflowIdConflictPolicy;"));
    assert!(rendered.contains("memo?: Record<string, unknown>;"));
    assert!(
        rendered
            .contains("searchAttributes?: common.TypedSearchAttributes | common.SearchAttributes;")
    );
    assert!(rendered.contains("versioningOverride?: common.VersioningOverride;"));
    assert!(rendered.contains("priority?: common.Priority;"));
    assert!(!rendered.contains("signal: string;"));
    assert!(rendered.contains("retryPolicy: common.RetryPolicy;"));
    assert!(rendered.contains("request: common.RetryPolicy,"));
    assert!(rendered.contains("// Included from support.$typescript"));
    assert!(rendered.contains("export function retryPolicyFromProto("));
    assert!(rendered.contains("_RequestFunctionName("));
    assert!(rendered.contains("input: _RequestArgsToPayloads(model.input),"));
    assert!(rendered.contains("signalInput: _RequestArgsToPayloads(model.signalInput),"));
    assert!(
        rendered
            .contains("signalName: ((value) => typeof value === 'string' ? value : (value.name))(")
    );
    assert!(rendered.contains("workflowType: workflowTypeToProto("));
    assert!(rendered.contains("taskQueue: taskQueueToProto("));
    assert!(rendered.contains(
        "workflowRunTimeout: model.workflowRunTimeout == null ? undefined : durationToProto(model.workflowRunTimeout),"
    ));
    assert!(rendered.contains("memo: model.memo == null ? undefined : memoToProto(model.memo),"));
    assert!(rendered.contains(
        "searchAttributes: model.searchAttributes == null ? undefined : searchAttributesToProto(model.searchAttributes),"
    ));
    assert!(rendered.contains(
        "priority: model.priority == null ? undefined : priorityToProto(model.priority),"
    ));
    assert!(rendered.contains(
        "versioningOverride: model.versioningOverride == null ? undefined : versioningOverrideToProto(model.versioningOverride),"
    ));
    assert!(rendered.contains("export function taskQueueFromProto("));
    assert!(rendered.contains("export function taskQueueToProto("));
    assert!(rendered.contains(
        "): temporal.api.workflowservice.v1.ISignalWithStartWorkflowExecutionRequest | undefined {"
    ));
    assert!(rendered.contains(
        "const result = await handle.result();\n    return workflow.getExternalWorkflowHandle(request.workflowId, result.runId ?? undefined);"
    ));
    assert!(rendered.contains("public async signalWithStartWorkflowExecution<"));
    assert!(
        rendered
            .contains("request: SignalWithStartWorkflowExecutionRequest<WorkflowFn, SignalValue>,")
    );
    assert!(rendered.contains("  ): Promise<workflow.ExternalWorkflowHandle> {"));
    assert!(!rendered.contains("SignalWithStartWorkflowExecutionRequest = {\n  fromProto("));
    assert!(!rendered.contains("export interface SignalWithStartWorkflowExecutionRequest {"));
    assert!(!rendered.contains("export interface RetryPolicy"));
    assert!(!rendered.contains("export interface WorkflowType"));
    assert!(!rendered.contains("export interface TaskQueue"));
    assert!(!rendered.contains("export interface Duration"));
    assert!(!rendered.contains("export interface Memo"));
    assert!(!rendered.contains("export interface SearchAttributes"));
    assert!(!rendered.contains("export interface Priority"));
    assert!(!rendered.contains("export interface VersioningOverride"));
    assert!(!rendered.contains("export enum WorkflowIdReusePolicy"));
    assert!(!rendered.contains("export enum WorkflowIdConflictPolicy"));
    assert!(!rendered.contains("signalWithStartWorkflow("));
    assert!(!rendered.contains("from './model_overrides.ts'"));
}
