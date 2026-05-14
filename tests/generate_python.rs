use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use heck::ToSnakeCase;
use nexus_api_gen::generate_to_string;
use nexus_api_gen::generator::{GeneratedOutputLayout, generate_files};

const PRIMARY_EXAMPLE_ID: &str = "workflow-service";
const TYPE_ROUNDTRIP_EXAMPLE_ID: &str = "type-roundtrip";

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn descriptor_path(root: &Path) -> PathBuf {
    root.join("examples/descriptors/temporal_api.bin")
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
    python_root(root).join(example_id.to_snake_case())
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
            if python_root.join(example_id.to_snake_case()).is_dir() {
                Some(example_id)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

fn read_python_package_files(dir: &Path) -> BTreeMap<PathBuf, String> {
    fn visit(root: &Path, dir: &Path, files: &mut BTreeMap<PathBuf, String>) {
        let mut entries = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                visit(root, &path, files);
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("py") {
                if path
                    .file_name()
                    .and_then(|file_name| file_name.to_str())
                    .is_some_and(|file_name| file_name.starts_with("test_"))
                {
                    continue;
                }
                files.insert(
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    fs::read_to_string(&path).unwrap(),
                );
            }
        }
    }

    let mut files = BTreeMap::new();
    visit(dir, dir, &mut files);
    files
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
            descriptor_path(root).to_str().unwrap(),
            "--output",
            output_path.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let format_status = Command::new("uv")
        .current_dir(python_root(root))
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

fn assert_python_310_syntax_compatible(package_dir: &Path) {
    let checker = r#"
import ast
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
for path in sorted(root.rglob("*.py")):
    source = path.read_text()
    try:
        ast.parse(source, filename=str(path), feature_version=(3, 10))
    except SyntaxError as exc:
        print(f"{path}: {exc}")
        raise
"#;
    let status = Command::new(
        project_root()
            .join("examples/python/.venv/bin/python")
            .to_str()
            .unwrap(),
    )
    .args(["-c", checker, package_dir.to_str().unwrap()])
    .status()
    .unwrap();
    assert!(status.success());
}

fn unique_output_path(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("nexus-api-gen-{label}-{unique}"))
}

#[test]
fn python_examples_generation_matches_checked_in_output() {
    let root = project_root();
    for example_id in python_example_ids(&root) {
        let output_path = unique_output_path(&format!("python-{example_id}"));
        generate_formatted_python_output(&root, &example_id, &output_path);
        assert_python_310_syntax_compatible(&output_path);
        let rendered = read_python_package_files(&output_path);
        let expected = read_python_package_files(&python_output_path(&root, &example_id));
        assert_eq!(rendered, expected, "snapshot mismatch for {example_id}");
        fs::remove_dir_all(output_path).unwrap();
    }
}

#[test]
fn python_example_suite_type_checks_and_runs() {
    let root = project_root();
    let example_dir = python_root(&root);

    let typecheck_status = Command::new("uv")
        .current_dir(&example_dir)
        .args(["run", "basedpyright"])
        .status()
        .unwrap();
    assert!(typecheck_status.success());

    let pytest_status = Command::new("uv")
        .current_dir(&example_dir)
        .args(["run", "pytest"])
        .status()
        .unwrap();
    assert!(pytest_status.success());
}

#[test]
fn python_request_models_are_write_only() {
    let root = project_root();
    let spec = nexus_api_gen::spec::ApiSpec::load_for_language(
        nexus_api_gen::language::Language::Python,
        &input_path(&root, PRIMARY_EXAMPLE_ID),
    )
    .unwrap();
    let descriptors =
        nexus_api_gen::descriptors::DescriptorIndex::load(&descriptor_path(&root)).unwrap();
    let generated = generate_files(
        nexus_api_gen::language::Language::Python,
        &spec,
        &descriptors,
        &nexus_api_gen::SupportFiles::default(),
    )
    .unwrap();
    assert_eq!(generated.layout, GeneratedOutputLayout::Directory);
    let models = generated
        .files
        .get(&PathBuf::from("models.py"))
        .expect("Python package should include models.py");
    let rendered = generate_to_string(
        nexus_api_gen::language::Language::Python,
        input_path(&root, PRIMARY_EXAMPLE_ID),
        &[descriptor_path(&root)],
    )
    .unwrap();

    assert!(!rendered.contains("SignalWithStartWorkflowExecutionRequest.from_proto"));
    assert!(!rendered.contains(
        "proto: temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest,\n    ) -> SignalWithStartWorkflowExecutionRequest:"
    ));
    assert!(rendered.contains("class SignalWithStartWorkflowExecutionRequest:"));
    assert!(rendered.contains("input: tuple[typing.Any, ...] | None = None"));
    assert!(!rendered.contains("(typing.TypedDict, total=False):"));
    assert!(!rendered.contains("typing.Unpack["));
    assert!(!rendered.contains("namespace: str | None = None"));
    assert!(!rendered.contains("namespace: str | None"));
    assert!(rendered.contains("message.namespace = workflow_namespace()"));
    assert!(rendered.contains("result = await handle"));
    assert!(rendered.contains(
        "return workflow.get_external_workflow_handle(request.workflow_id, run_id=result.run_id)"
    ));
    assert!(rendered.contains("async def _signal_with_start_workflow_execution("));
    assert!(rendered.contains("request: SignalWithStartWorkflowExecutionRequest"));
    assert!(rendered.contains(") -> workflow.ExternalWorkflowHandle[typing.Any]:"));
    assert!(rendered.contains("async def signal_with_start_workflow_execution("));
    assert!(rendered.contains("@typing.overload"));
    assert!(rendered.contains("workflow: str,"));
    assert!(rendered.contains("input: tuple[typing.Any, ...] | None = ...,"));
    assert!(rendered.contains(
        "workflow: collections.abc.Callable[[typing.Any], collections.abc.Awaitable[object]],"
    ));
    assert!(rendered.contains("FirstWorkflowArg = typing.TypeVar(\"FirstWorkflowArg\")"));
    assert!(rendered.contains(
        "RemainingWorkflowArgs = typing_extensions.TypeVarTuple(\"RemainingWorkflowArgs\")"
    ));
    assert!(rendered.contains(
        "input: tuple[FirstWorkflowArg, typing_extensions.Unpack[RemainingWorkflowArgs]],"
    ));
    assert!(rendered.contains(
        "signal: collections.abc.Callable[[typing.Any, SignalArg1], None | collections.abc.Awaitable[None]],"
    ));
    assert!(rendered.contains("signal_input: tuple[SignalArg1],"));
    assert!(rendered.contains("    *,"));
    assert!(rendered.contains(
        "workflow: str | collections.abc.Callable[..., collections.abc.Awaitable[object]],"
    ));
    assert!(rendered.contains(
        "signal: str | collections.abc.Callable[..., None | collections.abc.Awaitable[None]],"
    ));
    assert!(rendered.contains("input: object | tuple[object, ...] | None = None,"));
    assert!(rendered.contains("static_summary: str | None = None,"));
    assert!(rendered.contains("static_details: str | None = None,"));
    assert!(!rendered.contains("user_metadata_static_summary:"));
    assert!(!rendered.contains("user_metadata_static_details:"));
    assert!(rendered.contains("request = SignalWithStartWorkflowExecutionRequest("));
    assert!(rendered.contains("workflow=workflow,"));
    assert!(rendered.contains("def _nexus_normalize_function_args("));
    assert!(rendered.contains("normalized_input = _nexus_normalize_function_args(input)"));
    assert!(
        rendered.contains("normalized_signal_input = _nexus_normalize_function_args(signal_input)")
    );
    assert!(rendered.contains("user_metadata = ("));
    assert!(rendered.contains("if static_summary is None and static_details is None"));
    assert!(rendered.contains("static_summary=static_summary,"));
    assert!(rendered.contains("static_details=static_details,"));
    assert!(rendered.contains("input=normalized_input,"));
    assert!(rendered.contains("signal_input=normalized_signal_input,"));
    assert!(rendered.contains("user_metadata=user_metadata,"));
    assert!(rendered.contains("return await _signal_with_start_workflow_execution(request)"));
    assert!(rendered.contains("message.input.CopyFrom(payloads_to_proto(self.input))"));
    assert!(models.contains("from ._support import ("));
    assert!(models.contains("retry_policy_to_proto,"));

    let type_roundtrip_rendered = generate_to_string(
        nexus_api_gen::language::Language::Python,
        input_path(&root, TYPE_ROUNDTRIP_EXAMPLE_ID),
        &[descriptor_path(&root)],
    )
    .unwrap();
    assert!(type_roundtrip_rendered.contains("async def activity_options_operation("));
    assert!(type_roundtrip_rendered.contains("task_queue: str | None = None,"));
    assert!(type_roundtrip_rendered.contains("retry_policy: temporalio.common.RetryPolicy,"));
    assert!(type_roundtrip_rendered.contains("request = ActivityOptions("));
}
