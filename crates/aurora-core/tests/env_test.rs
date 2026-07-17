use aurora_core::ast::{EnvValue, EnvVar, Environment};
use aurora_core::env::{base_env, evaluate};
use std::path::Path;

/// The whole point of the allowlist is that an untrusted Beamfile never sees
/// ambient secrets. This test sets a fake secret and a locale variable, then
/// asserts the allowlist keeps the former out and lets the latter (and PATH)
/// through. All env mutations live in a single test to avoid races with the
/// process-global environment.
#[test]
fn base_env_filters_secrets_but_keeps_allowlisted_and_locale() {
    std::env::set_var("AURORA_TEST_SECRET", "leak-me");
    std::env::set_var("LC_TEST_LOCALE", "fr_FR.UTF-8");

    let env = base_env();

    assert!(
        !env.contains_key("AURORA_TEST_SECRET"),
        "a non-allowlisted secret must never be carried over"
    );
    assert!(env.contains_key("PATH"), "PATH must be carried over");
    assert!(
        env.contains_key("LC_TEST_LOCALE"),
        "LC_* locale variables must be carried over"
    );

    std::env::remove_var("AURORA_TEST_SECRET");
    std::env::remove_var("LC_TEST_LOCALE");
}

/// A non-UTF-8 variable in the ambient environment must not crash Aurora.
/// `std::env::vars()` panics while iterating over such a variable, even one
/// that would be filtered out; `base_env` must tolerate it.
#[cfg(unix)]
#[test]
fn base_env_survives_non_utf8_ambient_var() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let bad = OsStr::from_bytes(&[b'a', 0xff, b'b']);
    std::env::set_var("AURORA_TEST_NON_UTF8", bad);

    // Must not panic even though the value is not valid UTF-8.
    let env = base_env();

    // Not allowlisted, so it must not be carried over either.
    assert!(!env.contains_key("AURORA_TEST_NON_UTF8"));

    std::env::remove_var("AURORA_TEST_NON_UTF8");
}

/// An empty `environment {}` block still applies the allowlist: it never
/// leaks the full process environment.
#[test]
fn evaluate_empty_block_applies_allowlist() {
    std::env::set_var("AURORA_TEST_SECRET2", "leak-me");
    let block = Environment { vars: vec![] };

    let env = evaluate(&block, Path::new(".")).unwrap();

    assert!(!env.contains_key("AURORA_TEST_SECRET2"));
    std::env::remove_var("AURORA_TEST_SECRET2");
}

/// A `shell(...)` command that exits non-zero is a configuration error and
/// must fail loudly, not silently produce an empty variable.
#[test]
fn evaluate_fails_when_shell_command_exits_non_zero() {
    let block = Environment {
        vars: vec![EnvVar {
            name: "BROKEN".to_string(),
            value: EnvValue::Shell("exit 3".to_string()),
        }],
    };

    let result = evaluate(&block, Path::new("."));

    assert!(
        result.is_err(),
        "a failing environment shell command must return an error"
    );
}

/// `shell(...)` variables are evaluated sequentially and each is visible to
/// the following ones.
#[test]
fn evaluate_chains_shell_variables_sequentially() {
    let block = Environment {
        vars: vec![
            EnvVar {
                name: "A".to_string(),
                value: EnvValue::Shell("echo one".to_string()),
            },
            EnvVar {
                name: "B".to_string(),
                value: EnvValue::Shell("echo \"$A-two\"".to_string()),
            },
        ],
    };

    let env = evaluate(&block, Path::new(".")).unwrap();

    assert_eq!(env.get("A").map(String::as_str), Some("one"));
    assert_eq!(env.get("B").map(String::as_str), Some("one-two"));
}
