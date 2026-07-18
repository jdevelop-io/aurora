use aurora_core::ast::{EnvValue, EnvVar, Environment};
use aurora_core::env::{base_env, evaluate_overlay};
use std::path::Path;

#[test]
fn overlay_evaluates_sequentially_with_base_visible() {
    let block = Environment {
        vars: vec![
            EnvVar {
                name: "A".to_string(),
                value: EnvValue::Literal("one".to_string()),
            },
            EnvVar {
                name: "B".to_string(),
                value: EnvValue::Shell("echo \"$A-two\"".to_string()),
            },
        ],
    };
    let overlay = evaluate_overlay(&block, &base_env(), Path::new(".")).unwrap();
    assert_eq!(overlay.get("A").map(String::as_str), Some("one"));
    assert_eq!(overlay.get("B").map(String::as_str), Some("one-two"));
}

#[test]
fn overlay_shell_failure_is_a_hard_error() {
    let block = Environment {
        vars: vec![EnvVar {
            name: "BAD".to_string(),
            value: EnvValue::Shell("exit 3".to_string()),
        }],
    };
    let err = evaluate_overlay(&block, &base_env(), Path::new("."))
        .unwrap_err()
        .to_string();
    assert!(err.contains("BAD"), "got: {err}");
}

#[test]
fn overlay_sees_global_declared_values() {
    let mut base = base_env();
    base.insert("GLOBAL".to_string(), "g".to_string());
    let block = Environment {
        vars: vec![EnvVar {
            name: "FROM_GLOBAL".to_string(),
            value: EnvValue::Shell("echo \"$GLOBAL\"".to_string()),
        }],
    };
    let overlay = evaluate_overlay(&block, &base, Path::new(".")).unwrap();
    assert_eq!(overlay.get("FROM_GLOBAL").map(String::as_str), Some("g"));
}
