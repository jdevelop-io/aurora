//! Two beams declared with the same name are a mistake (a copy-paste that
//! silently dropped a task): the scheduler collapses them by name, keeping
//! only the last definition. The parser must reject the duplicate up front.

use aurora_core::parser::parse;

#[test]
fn duplicate_beam_names_are_rejected() {
    let input = r#"
beam "build" {
  run {
    commands = ["make first"]
  }
}
beam "build" {
  run {
    commands = ["make second"]
  }
}
"#;
    let result = parse(input);
    assert!(
        result.is_err(),
        "a Beamfile with two beams named 'build' must be rejected"
    );
}

#[test]
fn distinct_beam_names_are_accepted() {
    let input = r#"
beam "build" {
  run {
    commands = ["make"]
  }
}
beam "test" {
  run {
    commands = ["ctest"]
  }
}
"#;
    assert!(parse(input).is_ok());
}
