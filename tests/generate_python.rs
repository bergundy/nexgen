use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use nexus_api_gen::generate_to_string;

const PRIMARY_EXAMPLE_ID: &str = "workflow-service";

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn python_root(root: &Path) -> PathBuf {
    root.join("examples/python")
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

fn python_output_path(root: &Path, example_id: &str) -> PathBuf {
    python_root(root).join(example_id).join("output.py")
}

fn python_example_ids(root: &Path) -> Vec<String> {
    let python_root = python_root(root);
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
            if python_root.join(&example_id).is_dir() {
                Some(example_id)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

fn uv_cache_dir() -> &'static str {
    "/tmp/nexus-api-gen-uv-cache"
}

fn ruff_cache_dir() -> &'static str {
    "/tmp/nexus-api-gen-ruff-cache"
}

fn generate_formatted_python_output(root: &Path, example_id: &str, output_path: &Path) {
    let status = Command::new(env!("CARGO_BIN_EXE_nexus-api-gen"))
        .args([
            "generate",
            "--lang",
            "python",
            "--input",
            input_path(root, example_id).to_str().unwrap(),
            "--descriptors",
            root.join("descriptors.bin").to_str().unwrap(),
            "--output",
            output_path.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let format_status = Command::new("uv")
        .current_dir(python_root(root))
        .env("UV_CACHE_DIR", uv_cache_dir())
        .env("RUFF_CACHE_DIR", ruff_cache_dir())
        .args([
            "run",
            "ruff",
            "format",
            "--config",
            "pyproject.toml",
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
    std::env::temp_dir().join(format!("nexus-api-gen-{label}-{unique}.py"))
}

#[test]
fn python_examples_generation_matches_checked_in_output() {
    let root = project_root();
    for example_id in python_example_ids(&root) {
        let output_path = unique_output_path(&format!("python-{example_id}"));
        generate_formatted_python_output(&root, &example_id, &output_path);
        let rendered = fs::read_to_string(&output_path).unwrap();
        let expected = fs::read_to_string(python_output_path(&root, &example_id)).unwrap();
        assert_eq!(rendered, expected, "snapshot mismatch for {example_id}");
        fs::remove_file(output_path).unwrap();
    }
}

#[test]
fn python_example_suite_type_checks_and_runs() {
    let root = project_root();
    let example_dir = python_root(&root);

    let build_status = Command::new("uv")
        .current_dir(&example_dir)
        .env("UV_CACHE_DIR", uv_cache_dir())
        .env("RUFF_CACHE_DIR", ruff_cache_dir())
        .env("NEXUS_API_GEN_BIN", env!("CARGO_BIN_EXE_nexus-api-gen"))
        .args(["run", "build_outputs.py"])
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

    let pytest_status = Command::new("uv")
        .current_dir(&example_dir)
        .env("UV_CACHE_DIR", uv_cache_dir())
        .args(["run", "pytest"])
        .status()
        .unwrap();
    assert!(pytest_status.success());
}

#[test]
fn python_request_models_are_write_only() {
    let root = project_root();
    let rendered = generate_to_string(
        nexus_api_gen::language::Language::Python,
        input_path(&root, PRIMARY_EXAMPLE_ID),
        root.join("descriptors.bin"),
    )
    .unwrap();

    assert!(!rendered.contains("SignalWithStartWorkflowExecutionRequest.from_proto"));
    assert!(!rendered.contains(
        "proto: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest,\n    ) -> SignalWithStartWorkflowExecutionRequest:"
    ));
    assert!(rendered.contains("class SignalWithStartWorkflowExecutionRequest[*WorkflowArgs]:"));
    assert!(rendered.contains("input: tuple[typing.Any, ...] | None = None"));
    assert!(!rendered.contains("(typing.TypedDict, total=False):"));
    assert!(!rendered.contains("typing.Unpack["));
    assert!(!rendered.contains("namespace: str | None = None"));
    assert!(!rendered.contains("namespace: str | None"));
    assert!(rendered.contains("message.namespace = workflow.info().namespace"));
    assert!(rendered.contains("result = await handle"));
    assert!(rendered.contains(
        "return workflow.get_external_workflow_handle(request.workflow_id, run_id=result.run_id)"
    ));
    assert!(rendered.contains("async def signal_with_start_workflow_execution[*WorkflowArgs]("));
    assert!(rendered.contains("request: SignalWithStartWorkflowExecutionRequest[*WorkflowArgs]"));
    assert!(rendered.contains(") -> workflow.ExternalWorkflowHandle[typing.Any]:"));
    assert!(rendered.contains("async def signal_with_start_workflow_execution_args("));
    assert!(rendered.contains("    @typing.overload"));
    assert!(rendered.contains("workflow: str,"));
    assert!(rendered.contains("input: tuple[typing.Any, ...] | None = ...,"));
    assert!(rendered.contains(
        "workflow: collections.abc.Callable[[typing.Any], collections.abc.Awaitable[typing.Any]],"
    ));
    assert!(rendered.contains(
        "async def signal_with_start_workflow_execution_args[FirstWorkflowArg, *RemainingWorkflowArgs]("
    ));
    assert!(rendered.contains("input: tuple[FirstWorkflowArg, *RemainingWorkflowArgs],"));
    assert!(rendered.contains(
        "signal: collections.abc.Callable[[typing.Any, SignalArg1], None | collections.abc.Awaitable[None]],"
    ));
    assert!(rendered.contains("signal_input: tuple[SignalArg1],"));
    assert!(rendered.contains("        *,"));
    assert!(rendered.contains(
        "workflow: str | collections.abc.Callable[..., collections.abc.Awaitable[typing.Any]],"
    ));
    assert!(rendered.contains(
        "signal: str | collections.abc.Callable[..., None | collections.abc.Awaitable[None]],"
    ));
    assert!(rendered.contains("input: tuple[typing.Any, ...] | None = None,"));
    assert!(
        rendered.contains(
            "request = SignalWithStartWorkflowExecutionRequest[*tuple[typing.Any, ...]]("
        )
    );
    assert!(rendered.contains("workflow=workflow,"));
    assert!(rendered.contains("input=input,"));
    assert!(rendered.contains("return await self.signal_with_start_workflow_execution(request)"));
    assert!(rendered.contains("async def activity_options_operation_args("));
    assert!(rendered.contains("task_queue: str | None = None,"));
    assert!(rendered.contains("retry_policy: temporalio.common.RetryPolicy,"));
    assert!(rendered.contains("request = ActivityOptions("));
    assert!(rendered.contains("message.input.CopyFrom(payloads_to_proto(self.input))"));
}
