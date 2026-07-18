//! Gates (`skip_if`, `condition`) and `dir` must take part in variable and
//! argument interpolation just like `run.commands`, so a parameterized gate is
//! not left with a literal `${var.x}`/`${arg.N}` that reaches the shell as a
//! bad substitution and silently disables the gate.

use aurora_core::ast::ConditionClause;
use aurora_core::parser::{parse, resolve_variables};

#[test]
fn skip_if_interpolates_variables() {
    let input = r#"
variable "flag" {
  default = "build.lock"
}
beam "build" {
  skip_if = "test -f ${var.flag}"
  run {
    commands = ["make"]
  }
}
"#;
    let mut bf = parse(input).unwrap();
    resolve_variables(&mut bf).unwrap();
    assert_eq!(bf.beams[0].skip_if.as_deref(), Some("test -f build.lock"));
}

#[test]
fn condition_interpolates_variables() {
    let input = r#"
variable "marker" {
  default = ".ready"
}
beam "deploy" {
  condition {
    any = [
      { shell = "test -f ${var.marker}" }
    ]
  }
  run {
    commands = ["deploy"]
  }
}
"#;
    let mut bf = parse(input).unwrap();
    resolve_variables(&mut bf).unwrap();
    let ConditionClause::Shell(clause) = &bf.beams[0].condition.as_ref().unwrap().clauses[0];
    assert_eq!(clause, "test -f .ready");
}

#[test]
fn skip_if_unknown_variable_is_error() {
    let input = r#"
beam "build" {
  skip_if = "test -f ${var.missing}"
  run {
    commands = ["make"]
  }
}
"#;
    let mut bf = parse(input).unwrap();
    assert!(resolve_variables(&mut bf).is_err());
}
