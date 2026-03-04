use aurora_core::parser::parse;

#[test]
fn test_parse_minimal_beam() {
    let input = r#"
beam "hello" {
  description = "Say hello"
  run {
    commands = ["echo hello"]
  }
}
"#;
    let bf = parse(input).unwrap();
    assert_eq!(bf.beams.len(), 1);
    assert_eq!(bf.beams[0].name, "hello");
    assert_eq!(bf.beams[0].description.as_deref(), Some("Say hello"));
    let run = bf.beams[0].run.as_ref().unwrap();
    assert_eq!(run.commands, vec!["echo hello"]);
}

#[test]
fn test_parse_aurora_block() {
    let input = r#"
aurora {
  version = "1"
  default = "qa"
}
beam "qa" {
  depends_on = ["lint", "test"]
}
"#;
    let bf = parse(input).unwrap();
    let cfg = bf.config.as_ref().unwrap();
    assert_eq!(cfg.version, "1");
    assert_eq!(cfg.default.as_deref(), Some("qa"));
    assert_eq!(bf.beams[0].depends_on, vec!["lint", "test"]);
}

#[test]
fn test_parse_environment_block() {
    let input = r#"
environment {
  BRANCH = shell("git branch --show-current")
  MODE   = "production"
}
beam "b" {
  run { commands = ["echo $MODE"] }
}
"#;
    let bf = parse(input).unwrap();
    let env = bf.environment.as_ref().unwrap();
    assert_eq!(env.vars.len(), 2);
    assert!(matches!(&env.vars[0].value, aurora_core::ast::EnvValue::Shell(_)));
    assert!(matches!(&env.vars[1].value, aurora_core::ast::EnvValue::Literal(_)));
}

#[test]
fn test_parse_variable_block() {
    let input = r#"
variable "image" {
  default     = "ubuntu:22.04"
  description = "Docker image"
}
beam "b" {
  run { commands = ["echo"] }
}
"#;
    let bf = parse(input).unwrap();
    assert_eq!(bf.variables.len(), 1);
    assert_eq!(bf.variables[0].name, "image");
    assert_eq!(bf.variables[0].default, "ubuntu:22.04");
}

#[test]
fn test_parse_executor_docker() {
    let input = r#"
beam "phpstan" {
  depends_on = ["composer"]
  skip_if    = "test -z \"$FILES\""
  run {
    commands = ["phpstan analyse"]
    executor "docker" {
      image = "omega-tools:v1"
    }
  }
}
"#;
    let bf = parse(input).unwrap();
    let beam = &bf.beams[0];
    assert_eq!(beam.skip_if.as_deref(), Some("test -z \"$FILES\""));
    let exec = beam.run.as_ref().unwrap().executor.as_ref().unwrap();
    assert_eq!(exec.name, "docker");
    assert_eq!(exec.config.get("image").unwrap(), "omega-tools:v1");
}

#[test]
fn test_parse_inputs_outputs() {
    let input = r#"
beam "composer" {
  inputs  = ["composer.json", "composer.lock"]
  outputs = ["vendor"]
  run {
    commands = ["composer install"]
  }
}
"#;
    let bf = parse(input).unwrap();
    let beam = &bf.beams[0];
    assert_eq!(beam.inputs, vec!["composer.json", "composer.lock"]);
    assert_eq!(beam.outputs, vec!["vendor"]);
}

#[test]
fn test_parse_condition_any() {
    let input = r#"
beam "deptrac" {
  condition {
    any = [
      { shell = "test -n \"$A\"" },
      { shell = "test -n \"$B\"" }
    ]
  }
  run { commands = ["deptrac"] }
}
"#;
    let bf = parse(input).unwrap();
    let cond = bf.beams[0].condition.as_ref().unwrap();
    assert!(matches!(cond.op, aurora_core::ast::ConditionOp::Any));
    assert_eq!(cond.clauses.len(), 2);
}

#[test]
fn test_parse_aggregate_beam() {
    let input = r#"
beam "all" {
  depends_on = ["a", "b", "c"]
}
"#;
    let bf = parse(input).unwrap();
    assert!(bf.beams[0].run.is_none());
    assert_eq!(bf.beams[0].depends_on.len(), 3);
}

#[test]
fn test_parse_comments_ignored() {
    let input = r#"
# This is a comment
beam "test" {
  # Another comment
  description = "Test beam"
  run {
    commands = ["echo test"] # inline comment
  }
}
"#;
    let bf = parse(input).unwrap();
    assert_eq!(bf.beams[0].description.as_deref(), Some("Test beam"));
}

#[test]
fn test_parse_empty_beamfile_fails() {
    // A beamfile with no beams should still parse (empty is valid)
    let bf = parse("").unwrap();
    assert!(bf.beams.is_empty());
}

#[test]
fn test_parse_multiple_commands() {
    let input = r#"
beam "lint" {
  run {
    commands = [
      "phpstan analyse",
      "phpcs src/"
    ]
  }
}
"#;
    let bf = parse(input).unwrap();
    assert_eq!(bf.beams[0].run.as_ref().unwrap().commands.len(), 2);
}
