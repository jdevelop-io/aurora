use aurora_executor_api::{ExecutionInput, ExecutionOutput};
use std::collections::HashMap;
use std::path::PathBuf;

#[test]
fn test_execution_input_fields() {
    let input = ExecutionInput {
        commands: vec!["echo hello".to_string()],
        env: HashMap::from([("KEY".to_string(), "val".to_string())]),
        working_dir: PathBuf::from("/tmp"),
        config: serde_json::json!({}),
    };
    assert_eq!(input.commands.len(), 1);
    assert_eq!(input.env.get("KEY").unwrap(), "val");
    assert_eq!(input.working_dir, PathBuf::from("/tmp"));
}

#[test]
fn test_execution_output_success() {
    let output = ExecutionOutput {
        exit_code: 0,
        stdout: b"hello\n".to_vec(),
        stderr: vec![],
    };
    assert!(output.success());
}

#[test]
fn test_execution_output_failure() {
    let output = ExecutionOutput {
        exit_code: 1,
        stdout: vec![],
        stderr: b"error".to_vec(),
    };
    assert!(!output.success());
}

#[test]
fn test_execution_output_nonzero_exit() {
    let output = ExecutionOutput { exit_code: 127, stdout: vec![], stderr: vec![] };
    assert!(!output.success());
}
