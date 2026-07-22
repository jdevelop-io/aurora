use aurora_core::expand::{bind_cli_args, expand, instance_id, signature};
use aurora_core::parser::{parse, resolve_variables};
use std::collections::BTreeMap;

fn parsed(input: &str) -> aurora_core::ast::BeamFile {
    let mut bf = parse(input).unwrap();
    resolve_variables(&mut bf).unwrap();
    bf
}

const PIPELINE: &str = r#"
beam "build" {
  param "version" {}
  run { commands = ["cargo build --version ${param.version}"] }
}
beam "deploy" {
  param "version" {}
  param "env" { default = "staging" }
  depends_on = [{ beam = "build", params = { version = "${param.version}" } }]
  run { commands = ["./deploy.sh ${param.version} ${param.env}"] }
}
"#;

#[test]
fn binds_positional_then_named_cli_args() {
    let bf = parsed(PIPELINE);
    let deploy = bf.beams.iter().find(|b| b.name == "deploy").unwrap();
    let bound = bind_cli_args(deploy, &["1.2.3".into(), "env=production".into()]).unwrap();
    assert_eq!(bound.get("version").map(String::as_str), Some("1.2.3"));
    assert_eq!(bound.get("env").map(String::as_str), Some("production"));
}

#[test]
fn missing_required_param_fails_with_signature() {
    let bf = parsed(PIPELINE);
    let deploy = bf.beams.iter().find(|b| b.name == "deploy").unwrap();
    let err = bind_cli_args(deploy, &[]).unwrap_err().to_string();
    assert!(
        err.contains("missing required param 'version'"),
        "got: {err}"
    );
    assert!(err.contains("deploy <version> [env=staging]"), "got: {err}");
}

#[test]
fn param_bound_twice_fails() {
    let bf = parsed(PIPELINE);
    let deploy = bf.beams.iter().find(|b| b.name == "deploy").unwrap();
    let err = bind_cli_args(deploy, &["env=a".into(), "1.0".into(), "env=b".into()])
        .unwrap_err()
        .to_string();
    assert!(err.contains("bound twice"), "got: {err}");
}

#[test]
fn surplus_args_fail() {
    let bf = parsed(PIPELINE);
    let deploy = bf.beams.iter().find(|b| b.name == "deploy").unwrap();
    let err = bind_cli_args(deploy, &["1.0".into(), "prod".into(), "extra".into()])
        .unwrap_err()
        .to_string();
    assert!(err.contains("too many arguments"), "got: {err}");
}

#[test]
fn args_on_a_paramless_beam_fail() {
    let bf = parsed(r#"beam "fmt" { run { commands = ["cargo fmt"] } }"#);
    let err = bind_cli_args(&bf.beams[0], &["1.0".into()])
        .unwrap_err()
        .to_string();
    assert!(err.contains("takes no arguments"), "got: {err}");
}

#[test]
fn expands_target_and_dependency_into_bound_instances() {
    let bf = parsed(PIPELINE);
    let expansion = expand(&bf, "deploy", &["1.2.3".into()]).unwrap();
    assert_eq!(expansion.target_id, "deploy[env=staging,version=1.2.3]");
    let deploy = expansion
        .instances
        .iter()
        .find(|b| b.name == expansion.target_id)
        .unwrap();
    assert_eq!(
        deploy.run.as_ref().unwrap().commands[0],
        "./deploy.sh 1.2.3 staging"
    );
    assert_eq!(deploy.dependency_names(), vec!["build[version=1.2.3]"]);
    let build = expansion
        .instances
        .iter()
        .find(|b| b.name == "build[version=1.2.3]")
        .unwrap();
    assert_eq!(
        build.run.as_ref().unwrap().commands[0],
        "cargo build --version 1.2.3"
    );
    assert_eq!(
        build.bindings.get("version").map(String::as_str),
        Some("1.2.3")
    );
}

#[test]
fn identical_bindings_from_two_parents_deduplicate() {
    let bf = parsed(
        r#"
beam "build" {
  param "version" {}
  run { commands = ["echo ${param.version}"] }
}
beam "a" {
  depends_on = [{ beam = "build", params = { version = "1.0" } }]
  run { commands = ["echo a"] }
}
beam "b" {
  depends_on = [{ beam = "build", params = { version = "1.0" } }]
  run { commands = ["echo b"] }
}
beam "all" {
  depends_on = ["a", "b"]
  run { commands = ["echo all"] }
}
"#,
    );
    let expansion = expand(&bf, "all", &[]).unwrap();
    let builds: Vec<_> = expansion
        .instances
        .iter()
        .filter(|i| i.name.starts_with("build["))
        .collect();
    assert_eq!(builds.len(), 1);
}

#[test]
fn divergent_bindings_create_two_instances() {
    let bf = parsed(
        r#"
beam "build" {
  param "version" {}
  run { commands = ["echo ${param.version}"] }
}
beam "all" {
  depends_on = [
    { beam = "build", params = { version = "1.2" } },
    { beam = "build", params = { version = "1.3" } }
  ]
  run { commands = ["echo all"] }
}
"#,
    );
    let expansion = expand(&bf, "all", &[]).unwrap();
    let all = expansion
        .instances
        .iter()
        .find(|i| i.name == "all")
        .unwrap();
    assert_eq!(
        all.dependency_names(),
        vec!["build[version=1.2]", "build[version=1.3]"]
    );
}

#[test]
fn short_form_toward_required_params_fails() {
    let bf = parsed(
        r#"
beam "build" {
  param "version" {}
  run { commands = ["echo"] }
}
beam "deploy" {
  depends_on = ["build"]
  run { commands = ["echo"] }
}
"#,
    );
    let err = expand(&bf, "deploy", &[]).unwrap_err().to_string();
    assert!(err.contains("requires param 'version'"), "got: {err}");
}

#[test]
fn unknown_edge_binding_fails() {
    let bf = parsed(
        r#"
beam "build" {
  run { commands = ["echo"] }
}
beam "deploy" {
  depends_on = [{ beam = "build", params = { nope = "1" } }]
  run { commands = ["echo"] }
}
"#,
    );
    let err = expand(&bf, "deploy", &[]).unwrap_err().to_string();
    assert!(err.contains("unknown param 'nope'"), "got: {err}");
}

#[test]
fn unknown_param_reference_fails() {
    let bf = parsed(
        r#"
beam "deploy" {
  run { commands = ["echo ${param.version}"] }
}
"#,
    );
    let err = expand(&bf, "deploy", &[]).unwrap_err().to_string();
    assert!(err.contains("unknown param 'version'"), "got: {err}");
}

#[test]
fn arg_tokens_get_migration_diagnostic() {
    let bf = parsed(
        r#"
beam "deploy" {
  run { commands = ["echo ${arg.1}"] }
}
"#,
    );
    let err = expand(&bf, "deploy", &[]).unwrap_err().to_string();
    assert!(
        err.contains("arguments were replaced by params"),
        "got: {err}"
    );
}

#[test]
fn divergent_cycle_is_cut_by_depth_cap() {
    let bf = parsed(
        r#"
beam "loop" {
  param "n" {}
  depends_on = [{ beam = "loop", params = { n = "x${param.n}" } }]
  run { commands = ["echo ${param.n}"] }
}
"#,
    );
    let err = expand(&bf, "loop", &["1".into()]).unwrap_err().to_string();
    assert!(err.contains("instantiation depth"), "got: {err}");
}

#[test]
fn bound_values_are_never_reinterpolated() {
    let bf = parsed(PIPELINE);
    let expansion = expand(&bf, "deploy", &["${param.env}".into()]).unwrap();
    let build = expansion
        .instances
        .iter()
        .find(|b| b.name.starts_with("build["))
        .unwrap();
    // The literal value flows through untouched: no second expansion.
    assert_eq!(
        build.run.as_ref().unwrap().commands[0],
        "cargo build --version ${param.env}"
    );
}

#[test]
fn unreached_paramless_beams_get_default_instances() {
    let bf = parsed(
        r#"
beam "fmt" { run { commands = ["cargo fmt"] } }
beam "deploy" {
  param "version" {}
  run { commands = ["echo ${param.version}"] }
}
beam "other" { run { commands = ["echo other"] } }
"#,
    );
    let expansion = expand(&bf, "fmt", &[]).unwrap();
    let names: Vec<&str> = expansion
        .instances
        .iter()
        .map(|i| i.name.as_str())
        .collect();
    assert!(names.contains(&"fmt"));
    assert!(names.contains(&"other"));
    // A beam with required params has no default instance.
    assert!(!names.iter().any(|n| n.starts_with("deploy")));
}

#[test]
fn instance_id_format_is_stable_and_escaped() {
    let mut bindings = BTreeMap::new();
    assert_eq!(instance_id("build", &bindings), "build");
    bindings.insert("b".to_string(), "2".to_string());
    bindings.insert("a".to_string(), "1".to_string());
    assert_eq!(instance_id("build", &bindings), "build[a=1,b=2]");
    let mut hostile = BTreeMap::new();
    hostile.insert("a".to_string(), "1],x=[".to_string());
    // `escape_id_value` escapes `\`, `,` and `]`, so that distinct binding sets
    // (e.g. `{a: "1,b=2"}` vs `{a: "1", b: "2"}`) can never collapse into the
    // same instance id. For the hostile value `1],x=[`, the comma and the
    // closing bracket are both escaped.
    assert_eq!(instance_id("build", &hostile), "build[a=1\\]\\,x=[]");
}

#[test]
fn params_interpolate_into_non_command_fields() {
    // `instantiate()` must resolve `${param.x}` in every interpolatable field,
    // not just `run.commands`: `dir` and the `skip_if` gate reach a shell too,
    // so a literal token left there would become a bad substitution.
    let bf = parsed(
        r#"
beam "build" {
  param "target" {}
  dir = "packages/${param.target}"
  skip_if = "test -f ${param.target}.lock"
  run { commands = ["make"] }
}
"#,
    );
    let expansion = expand(&bf, "build", &["api".into()]).unwrap();
    let build = expansion
        .instances
        .iter()
        .find(|b| b.name == expansion.target_id)
        .unwrap();
    assert_eq!(build.dir.as_deref(), Some("packages/api"));
    assert_eq!(build.skip_if.as_deref(), Some("test -f api.lock"));
}

#[test]
fn unknown_dependency_is_preserved_verbatim_without_panicking() {
    // A dependency on a name that is not a declared beam must not panic
    // expansion: the raw name flows through unchanged so `BeamGraph::from_deps`
    // reports the unknown dependency exactly as before.
    let bf = parsed(
        r#"
beam "deploy" {
  depends_on = ["ghost"]
  run { commands = ["echo deploy"] }
}
"#,
    );
    let expansion = expand(&bf, "deploy", &[]).unwrap();
    let deploy = expansion
        .instances
        .iter()
        .find(|b| b.name == "deploy")
        .unwrap();
    assert_eq!(deploy.dependency_names(), vec!["ghost"]);
}

#[test]
fn signature_renders_required_and_defaulted_params() {
    let bf = parsed(PIPELINE);
    let deploy = bf.beams.iter().find(|b| b.name == "deploy").unwrap();
    assert_eq!(signature(deploy), "deploy <version> [env=staging]");
}

#[test]
fn out_of_closure_binding_error_explains_upfront_validation() {
    // `lint` is invoked, but `orphan` (an unrelated, param-less beam that still
    // gets a default instance) depends on a beam requiring an unbound param.
    // Aurora validates every declared beam up front, so this aborts even the
    // `lint` run: the message must name the faulty beam and say why an
    // unrelated run is being stopped, not just dump a bare "requires param".
    let bf = parsed(
        r#"
beam "lint" { run { commands = ["echo lint"] } }
beam "orphan" {
  depends_on = ["needs"]
  run { commands = ["echo orphan"] }
}
beam "needs" {
  param "req" {}
  run { commands = ["echo ${param.req}"] }
}
"#,
    );
    let err = expand(&bf, "lint", &[]).unwrap_err().to_string();
    assert!(err.contains("orphan"), "must name the faulty beam:\n{err}");
    assert!(
        err.contains("requires param 'req'"),
        "must keep the underlying cause:\n{err}"
    );
    assert!(
        err.contains("before running"),
        "must explain the up-front validation stopping an unrelated run:\n{err}"
    );
}

#[test]
fn invoked_target_binding_error_stays_unadorned() {
    // The invoked target's own closure fails with the plain message, without
    // the up-front-validation preface: the error is directly about the run the
    // user asked for.
    let bf = parsed(
        r#"
beam "build" {
  param "version" {}
  run { commands = ["echo"] }
}
beam "deploy" {
  depends_on = ["build"]
  run { commands = ["echo"] }
}
"#,
    );
    let err = expand(&bf, "deploy", &[]).unwrap_err().to_string();
    assert!(err.contains("requires param 'version'"), "got: {err}");
    assert!(
        !err.contains("before running"),
        "the invoked target must not get the up-front-validation preface:\n{err}"
    );
}
