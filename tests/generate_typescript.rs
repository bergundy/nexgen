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

fn sample_typescript_output_path(fixture: &std::path::Path) -> PathBuf {
    fixture.join("typescript/output.ts")
}

#[test]
fn sample_typescript_generation_matches_checked_in_output() {
    let root = project_root();
    let fixture = sample_fixture_dir(&root);
    let rendered = generate_to_string(
        nexus_api_gen::language::Language::TypeScript,
        fixture.join("input.yaml"),
        root.join("descriptors.bin"),
    )
    .unwrap();
    let expected = fs::read_to_string(sample_typescript_output_path(&fixture)).unwrap();

    assert_eq!(rendered, expected);
}

#[test]
fn cli_generates_typescript_file() {
    let root = project_root();
    let fixture = sample_fixture_dir(&root);
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let output_path = std::env::temp_dir().join(format!("nexus-api-gen-{unique}.ts"));

    let status = Command::new(env!("CARGO_BIN_EXE_nexus-api-gen"))
        .args([
            "generate",
            "--lang",
            "typescript",
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
    let expected = fs::read_to_string(sample_typescript_output_path(&fixture)).unwrap();
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
    let fixture = sample_fixture_dir(&root);
    let rendered = generate_to_string(
        nexus_api_gen::language::Language::TypeScript,
        fixture.join("input.yaml"),
        root.join("descriptors.bin"),
    )
    .unwrap();

    assert!(rendered.contains("workflowType: WorkflowType;"));
    assert!(rendered.contains("workflowId: string;"));
    assert!(rendered.contains("taskQueue: TaskQueue;"));
    assert!(rendered.contains("signalName: string;"));
    assert!(rendered.contains("retryPolicy: common.RetryPolicy;"));
    assert!(rendered.contains("request: common.RetryPolicy,"));
    assert!(rendered.contains("// Included from support.$typescriptFile"));
    assert!(rendered.contains("export function retryPolicyFromProto("));
    assert!(!rendered.contains("SignalWithStartWorkflowExecutionRequest = {\n  fromProto("));
    assert!(!rendered.contains("export interface RetryPolicy"));
    assert!(!rendered.contains("signalWithStartWorkflow("));
    assert!(!rendered.contains("from './model_overrides.ts'"));
}
