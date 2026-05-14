use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use nexus_api_gen::generate_to_string;

const PRIMARY_EXAMPLE_ID: &str = "workflow-service";

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn descriptor_path(root: &Path) -> PathBuf {
    root.join("examples/descriptors/temporal_api.bin")
}

fn typescript_root(root: &Path) -> PathBuf {
    root.join("examples/typescript")
}

fn input_path(root: &Path, example_id: &str) -> PathBuf {
    let flat_path = root
        .join("examples/inputs")
        .join(format!("{example_id}.wit"));
    if flat_path.is_file() {
        flat_path
    } else {
        root.join("examples/inputs")
            .join(example_id)
            .join("main.wit")
    }
}

fn typescript_output_path(root: &Path, example_id: &str) -> PathBuf {
    typescript_root(root).join(example_id).join("output.ts")
}

fn typescript_example_ids(root: &Path) -> Vec<String> {
    let typescript_root = typescript_root(root);
    let mut ids = fs::read_dir(root.join("examples/inputs"))
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            let example_id = if path.is_file() {
                path.file_stem()?.to_string_lossy().into_owned()
            } else if path.join("main.wit").is_file() {
                path.file_name()?.to_string_lossy().into_owned()
            } else {
                return None;
            };
            if typescript_root.join(&example_id).is_dir() {
                Some(example_id)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

fn ensure_typescript_dependencies(root: &Path) {
    let example_dir = typescript_root(root);
    if example_dir.join("node_modules").exists() {
        return;
    }

    let install_status = Command::new("npm")
        .current_dir(&example_dir)
        .args(["install", "--no-fund", "--no-audit"])
        .status()
        .unwrap();
    assert!(install_status.success());
}

fn generate_formatted_typescript_output(root: &Path, example_id: &str, output_path: &Path) {
    ensure_typescript_dependencies(root);

    let status = Command::new(env!("CARGO_BIN_EXE_nexus-api-gen"))
        .args([
            "generate",
            "--lang",
            "typescript",
            "--input",
            input_path(root, example_id).to_str().unwrap(),
            "--descriptors",
            descriptor_path(root).to_str().unwrap(),
            "--output",
            output_path.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let format_status = Command::new("npm")
        .current_dir(typescript_root(root))
        .args([
            "exec",
            "--",
            "prettier",
            "--write",
            output_path.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(format_status.success());
}

fn unique_output_path(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("nexus-api-gen-{label}-{unique}.ts"))
}

#[test]
fn typescript_examples_generation_matches_checked_in_output() {
    let root = project_root();
    for example_id in typescript_example_ids(&root) {
        let output_path = unique_output_path(&format!("typescript-{example_id}"));
        generate_formatted_typescript_output(&root, &example_id, &output_path);
        let rendered = fs::read_to_string(&output_path).unwrap();
        let expected = fs::read_to_string(typescript_output_path(&root, &example_id)).unwrap();
        assert_eq!(rendered, expected, "snapshot mismatch for {example_id}");
        fs::remove_file(output_path).unwrap();
    }
}

#[test]
fn typescript_example_suite_typechecks_and_tests() {
    let root = project_root();
    let example_dir = typescript_root(&root);
    ensure_typescript_dependencies(&root);

    let typecheck_status = Command::new("npm")
        .current_dir(&example_dir)
        .args(["run", "typecheck"])
        .status()
        .unwrap();
    assert!(typecheck_status.success());

    let test_status = Command::new("npm")
        .current_dir(&example_dir)
        .args(["run", "test"])
        .status()
        .unwrap();
    assert!(test_status.success());
}

#[test]
fn typescript_renders_required_fields_and_custom_message_types() {
    let root = project_root();
    let rendered = generate_to_string(
        nexus_api_gen::language::Language::TypeScript,
        input_path(&root, PRIMARY_EXAMPLE_ID),
        &[descriptor_path(&root)],
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
    assert!(rendered.contains("workflowType: workflowTypeToProto("));
    assert!(rendered.contains("workflow_function_name("));
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
