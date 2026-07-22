//! `phantom_beams` picks the declared beams that produced no instance, so the
//! execution sidebar can still list them (dimmed, non-launchable). A beam with a
//! required param has no default instance; an instantiated beam (including a
//! bound target) must never be listed as a phantom.

use aurora::phantom_beams;
use aurora_core::expand::expand;
use aurora_core::parser::{parse, resolve_variables};

fn parsed(input: &str) -> aurora_core::ast::BeamFile {
    let mut bf = parse(input).unwrap();
    resolve_variables(&mut bf).unwrap();
    bf
}

#[test]
fn lists_required_param_beams_without_an_instance() {
    let bf = parsed(
        r#"
beam "lint" { run { commands = ["echo lint"] } }
beam "deploy" {
  param "version" {}
  run { commands = ["echo ${param.version}"] }
}
"#,
    );
    // Invoking `lint` instantiates `lint` (and no `deploy`, which needs a value).
    let expansion = expand(&bf, "lint", &[]).unwrap();
    let phantoms = phantom_beams(&bf.beams, &expansion.instances, "");
    let names: Vec<&str> = phantoms.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, vec!["deploy"]);
}

#[test]
fn excludes_instantiated_beams_including_a_bound_target() {
    let bf = parsed(
        r#"
beam "build" {
  param "version" {}
  run { commands = ["echo ${param.version}"] }
}
beam "deploy" {
  param "version" {}
  depends_on = [{ beam = "build", params = { version = "${param.version}" } }]
  run { commands = ["echo deploy"] }
}
"#,
    );
    // Invoking `deploy 1.0` instantiates both `deploy` and `build`, even though
    // both declare a required param: an instance id maps back to its source
    // name, so neither is a phantom.
    let expansion = expand(&bf, "deploy", &["1.0".to_string()]).unwrap();
    let phantoms = phantom_beams(&bf.beams, &expansion.instances, "");
    assert!(
        phantoms.is_empty(),
        "instantiated required-param beams are not phantoms: {phantoms:?}"
    );
}

#[test]
fn honours_the_exclude_name() {
    let bf = parsed(
        r#"
beam "lint" { run { commands = ["echo lint"] } }
beam "__multi__" {
  param "x" {}
  run { commands = ["echo multi"] }
}
"#,
    );
    // `__multi__` declares a required param, so it would be a phantom, but the
    // virtual sentinel must never surface in the sidebar.
    let expansion = expand(&bf, "lint", &[]).unwrap();
    let phantoms = phantom_beams(&bf.beams, &expansion.instances, "__multi__");
    assert!(
        phantoms.iter().all(|(n, _)| n != "__multi__"),
        "the excluded name must be dropped: {phantoms:?}"
    );
}
