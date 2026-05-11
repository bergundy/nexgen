use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use nexus_api_gen::generate_to_string;

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn sample_fixture_dir(root: &std::path::Path) -> PathBuf {
    root.join("tests/fixtures/sample")
}

fn sample_python_output_path(fixture: &std::path::Path) -> PathBuf {
    fixture.join("python/output.py")
}

fn uv_cache_dir() -> &'static str {
    "/tmp/nexus-api-gen-uv-cache"
}

#[test]
fn sample_generation_matches_checked_in_output() {
    let root = project_root();
    let fixture = sample_fixture_dir(&root);
    let rendered = generate_to_string(
        nexus_api_gen::language::Language::Python,
        fixture.join("input.yaml"),
        root.join("descriptors.bin"),
    )
    .unwrap();
    let expected = fs::read_to_string(sample_python_output_path(&fixture)).unwrap();

    assert_eq!(rendered, expected);
}

#[test]
fn cli_generates_python_file() {
    let root = project_root();
    let fixture = sample_fixture_dir(&root);
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let output_path = std::env::temp_dir().join(format!("nexus-api-gen-{unique}.py"));

    let status = Command::new(env!("CARGO_BIN_EXE_nexus-api-gen"))
        .args([
            "generate",
            "--lang",
            "python",
            "--input",
            fixture.join("input.yaml").to_str().unwrap(),
            "--descriptors",
            root.join("descriptors.bin").to_str().unwrap(),
            "--output",
            output_path.to_str().unwrap(),
        ])
        .status()
        .unwrap();

    assert!(status.success());

    let rendered = fs::read_to_string(&output_path).unwrap();
    let expected = fs::read_to_string(sample_python_output_path(&fixture)).unwrap();
    assert_eq!(rendered, expected);

    fs::remove_file(output_path).unwrap();
}

#[test]
fn python_validation_app_type_checks_and_runs() {
    let root = project_root();
    let example_dir = root.join("examples/python-validation");

    let build_status = Command::new("uv")
        .current_dir(&example_dir)
        .env("UV_CACHE_DIR", uv_cache_dir())
        .args(["run", "build_output.py"])
        .status()
        .unwrap();
    assert!(build_status.success());

    let typecheck_status = Command::new("uv")
        .current_dir(&example_dir)
        .env("UV_CACHE_DIR", uv_cache_dir())
        .args(["run", "basedpyright"])
        .status()
        .unwrap();
    assert!(typecheck_status.success());

    let run_status = Command::new("uv")
        .current_dir(&example_dir)
        .env("UV_CACHE_DIR", uv_cache_dir())
        .args(["run", "main.py"])
        .status()
        .unwrap();
    assert!(run_status.success());
}

#[test]
fn python_request_models_are_write_only() {
    let root = project_root();
    let fixture = sample_fixture_dir(&root);
    let rendered = generate_to_string(
        nexus_api_gen::language::Language::Python,
        fixture.join("input.yaml"),
        root.join("descriptors.bin"),
    )
    .unwrap();

    assert!(!rendered.contains("SignalWithStartWorkflowExecutionRequest.from_proto"));
    assert!(!rendered.contains(
        "proto: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest,\n    ) -> SignalWithStartWorkflowExecutionRequest:"
    ));
    assert!(rendered.contains("class SignalWithStartWorkflowExecutionRequest[*WorkflowArgs]:"));
    assert!(rendered.contains("input: tuple[*WorkflowArgs] | None = None"));
    assert!(rendered.contains(
        "class SignalWithStartWorkflowExecutionRequestArgs[*WorkflowArgs](typing.TypedDict, total=False):"
    ));
    assert!(rendered.contains("workflow_id: typing.Required[str]"));
    assert!(!rendered.contains("namespace: str | None = None"));
    assert!(!rendered.contains("namespace: str | None"));
    assert!(rendered.contains("message.namespace = workflow.info().namespace"));
    assert!(rendered.contains("async def signal_with_start_workflow_execution[*WorkflowArgs]("));
    assert!(rendered.contains("request: SignalWithStartWorkflowExecutionRequest[*WorkflowArgs]"));
    assert!(
        rendered.contains("async def signal_with_start_workflow_execution_args[*WorkflowArgs](")
    );
    assert!(rendered.contains(
        "**kwargs: typing.Unpack[SignalWithStartWorkflowExecutionRequestArgs[*WorkflowArgs]],"
    ));
    assert!(rendered.contains("message.input.CopyFrom(payloads_to_proto(self.input))"));
}
