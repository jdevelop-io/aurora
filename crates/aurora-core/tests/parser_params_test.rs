use aurora_core::ast::EnvValue;
use aurora_core::parser::{parse, resolve_variables};

#[test]
fn parses_param_blocks_in_declaration_order() {
    let input = r#"
beam "deploy" {
  param "version" { description = "Version to deploy" }
  param "env" { default = "staging" }
  run { commands = ["./deploy.sh ${param.version} ${param.env}"] }
}
"#;
    let bf = parse(input).unwrap();
    let beam = &bf.beams[0];
    assert_eq!(beam.params.len(), 2);
    assert_eq!(beam.params[0].name, "version");
    assert_eq!(beam.params[0].default, None);
    assert_eq!(
        beam.params[0].description.as_deref(),
        Some("Version to deploy")
    );
    assert_eq!(beam.params[1].name, "env");
    assert_eq!(beam.params[1].default.as_deref(), Some("staging"));
}

#[test]
fn rejects_non_identifier_param_names() {
    let input = r#"
beam "deploy" {
  param "a,b" {}
}
"#;
    let err = parse(input).unwrap_err().to_string();
    assert!(
        err.contains("param name 'a,b' in beam 'deploy' is not a valid identifier"),
        "got: {err}"
    );
}

#[test]
fn rejects_duplicate_param_names() {
    let input = r#"
beam "deploy" {
  param "version" {}
  param "version" { default = "1" }
}
"#;
    let err = parse(input).unwrap_err().to_string();
    assert!(err.contains("duplicate param 'version'"), "got: {err}");
}

#[test]
fn parses_bound_depends_on_entries() {
    let input = r#"
beam "build" {
  param "version" {}
  run { commands = ["cargo build"] }
}
beam "deploy" {
  param "version" {}
  depends_on = [
    "fmt",
    { beam = "build", params = { version = "${param.version}" } }
  ]
}
beam "fmt" {
  run { commands = ["cargo fmt"] }
}
"#;
    let bf = parse(input).unwrap();
    let deploy = bf.beams.iter().find(|b| b.name == "deploy").unwrap();
    assert_eq!(deploy.depends_on.len(), 2);
    assert_eq!(deploy.depends_on[0].beam, "fmt");
    assert!(deploy.depends_on[0].params.is_empty());
    assert_eq!(deploy.depends_on[1].beam, "build");
    assert_eq!(
        deploy.depends_on[1]
            .params
            .get("version")
            .map(String::as_str),
        Some("${param.version}")
    );
}

#[test]
fn parses_beam_environment_block() {
    let input = r#"
beam "deploy" {
  environment {
    DEPLOY_TARGET = "${param.env}"
    RELEASE_TAG = shell("git describe --tags")
  }
  run { commands = ["./deploy.sh"] }
}
"#;
    let bf = parse(input).unwrap();
    let env = bf.beams[0].environment.as_ref().unwrap();
    assert_eq!(env.vars.len(), 2);
    assert_eq!(env.vars[0].name, "DEPLOY_TARGET");
}

#[test]
fn beam_local_variable_block_gets_migration_diagnostic() {
    let input = r#"
beam "deploy" {
  variable "target" { default = "staging" }
  run { commands = ["echo"] }
}
"#;
    let err = parse(input).unwrap_err().to_string();
    assert!(
        err.contains("beam-local variables were replaced by `param`"),
        "got: {err}"
    );
}

#[test]
fn resolve_variables_interpolates_globals_into_edge_bindings() {
    let input = r#"
variable "channel" { default = "stable" }
beam "build" {
  param "channel" {}
  run { commands = ["cargo build"] }
}
beam "deploy" {
  depends_on = [{ beam = "build", params = { channel = "${var.channel}" } }]
  run { commands = ["./deploy.sh"] }
}
"#;
    let mut bf = parse(input).unwrap();
    resolve_variables(&mut bf).unwrap();
    let deploy = bf.beams.iter().find(|b| b.name == "deploy").unwrap();
    assert_eq!(
        deploy.depends_on[0]
            .params
            .get("channel")
            .map(String::as_str),
        Some("stable")
    );
}

#[test]
fn resolve_variables_interpolates_globals_into_beam_environment() {
    let input = r#"
variable "region" { default = "eu-west-1" }
beam "deploy" {
  environment {
    REGION = "${var.region}"
  }
  run { commands = ["./deploy.sh"] }
}
"#;
    let mut bf = parse(input).unwrap();
    resolve_variables(&mut bf).unwrap();
    let env = bf.beams[0].environment.as_ref().unwrap();
    match &env.vars[0].value {
        EnvValue::Literal(s) => assert_eq!(s, "eu-west-1"),
        other => panic!("expected a literal value, got {other:?}"),
    }
}
