use aurora_core::parser::{parse, resolve_variables};

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
    assert_eq!(bf.beams[0].dependency_names(), vec!["lint", "test"]);
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
    assert!(matches!(
        &env.vars[0].value,
        aurora_core::ast::EnvValue::Shell(_)
    ));
    assert!(matches!(
        &env.vars[1].value,
        aurora_core::ast::EnvValue::Literal(_)
    ));
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
fn test_resolve_variables_uses_defaults() {
    let input = r#"
variable "image" { default = "old:1" }
beam "b" {
  run {
    commands = ["echo"]
    executor "docker" { image = var.image }
  }
}
"#;
    let mut bf = parse(input).unwrap();
    // Before resolution the reference is left verbatim in the config.
    let exec = bf.beams[0].run.as_ref().unwrap().executor.as_ref().unwrap();
    assert_eq!(exec.config.get("image").unwrap(), "var.image");

    resolve_variables(&mut bf).unwrap();
    let exec = bf.beams[0].run.as_ref().unwrap().executor.as_ref().unwrap();
    assert_eq!(exec.config.get("image").unwrap(), "old:1");
}

#[test]
fn test_resolve_variables_honors_overridden_default() {
    // Reproduces the --var bug: an override applied to the variable default
    // after parsing must reach the executor config, which only happens if
    // resolution runs after the override rather than during parse().
    let input = r#"
variable "image" { default = "old:1" }
beam "b" {
  run {
    commands = ["echo"]
    executor "docker" { image = var.image }
  }
}
"#;
    let mut bf = parse(input).unwrap();
    bf.variables[0].default = "new:2".to_string(); // simulates --var image=new:2
    resolve_variables(&mut bf).unwrap();

    let exec = bf.beams[0].run.as_ref().unwrap().executor.as_ref().unwrap();
    assert_eq!(exec.config.get("image").unwrap(), "new:2");
}

#[test]
fn test_parse_executor_docker_volumes() {
    // Volumes are declared as a comma-separated string, the only shape the
    // executor config carries end to end.
    let input = r#"
beam "b" {
  run {
    commands = ["echo"]
    executor "docker" {
      image   = "alpine:3.19"
      volumes = "/data:/data:ro,/cache:/cache:rw"
    }
  }
}
"#;
    let bf = parse(input).unwrap();
    let exec = bf.beams[0].run.as_ref().unwrap().executor.as_ref().unwrap();
    assert_eq!(
        exec.config.get("volumes").unwrap(),
        "/data:/data:ro,/cache:/cache:rw"
    );
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
fn test_parse_beam_dir() {
    let input = r#"
beam "build" {
  dir = "packages/api"
  run {
    commands = ["npm run build"]
  }
}
"#;
    let bf = parse(input).unwrap();
    assert_eq!(bf.beams[0].dir.as_deref(), Some("packages/api"));
}

#[test]
fn test_parse_beam_without_dir_is_none() {
    let input = r#"
beam "build" {
  run {
    commands = ["npm run build"]
  }
}
"#;
    let bf = parse(input).unwrap();
    assert_eq!(bf.beams[0].dir, None);
}

#[test]
fn test_dir_interpolates_variables() {
    let input = r#"
variable "pkg" {
  default = "api"
}
beam "build" {
  dir = "packages/${var.pkg}"
  run {
    commands = ["npm run build"]
  }
}
"#;
    let mut bf = parse(input).unwrap();
    resolve_variables(&mut bf).unwrap();
    assert_eq!(bf.beams[0].dir.as_deref(), Some("packages/api"));
}

#[test]
fn test_dir_unknown_variable_is_error() {
    let input = r#"
beam "build" {
  dir = "packages/${var.missing}"
  run {
    commands = ["npm run build"]
  }
}
"#;
    let mut bf = parse(input).unwrap();
    assert!(resolve_variables(&mut bf).is_err());
}

#[test]
fn test_parse_rejects_oversized_beamfile() {
    // A pathologically large Beamfile is rejected before parsing, to bound the
    // parser's memory and stack use on untrusted input.
    let huge = "x".repeat(2 * 1024 * 1024);
    assert!(parse(&huge).is_err());
}

#[test]
fn test_parse_empty_beamfile_is_valid() {
    // A Beamfile with no beams still parses: empty is valid.
    let bf = parse("").unwrap();
    assert!(bf.beams.is_empty());
}

#[test]
fn test_parse_malformed_beamfile_errors() {
    // Syntactically invalid inputs must return Err, not panic.
    for input in [
        "beam \"x\" {",                // unclosed block
        "beam {\n  run {}\n}",         // missing beam name
        "beam \"x\" { run = \"no\" }", // run is a block, not a string
        "{{{",                         // garbage
    ] {
        assert!(
            parse(input).is_err(),
            "expected a parse error for: {input:?}"
        );
    }
}

#[test]
fn test_parse_escape_sequences() {
    // Escape handling must be single-pass: an escaped backslash followed by
    // `n` (`\\n` in the source) is a literal backslash then a literal `n`, NOT
    // a newline. Chained `.replace()` calls used to mistranslate it.
    let input = r#"
beam "x" {
  description = "back\\nslash"
  run { commands = ["echo"] }
}
"#;
    let bf = parse(input).unwrap();
    assert_eq!(
        bf.beams[0].description.as_deref(),
        Some("back\\nslash"),
        "\\\\n must stay a literal backslash + n, not become a newline"
    );

    // A genuine escape sequence is still decoded.
    let input = r#"
beam "y" {
  description = "line\nbreak\ttab\"quote"
  run { commands = ["echo"] }
}
"#;
    let bf = parse(input).unwrap();
    assert_eq!(
        bf.beams[0].description.as_deref(),
        Some("line\nbreak\ttab\"quote")
    );
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

#[test]
fn test_parse_allow_failure() {
    let input = r#"
beam "container" {
  allow_failure = true
  run { commands = ["false"] }
}
beam "build" {
  run { commands = ["true"] }
}
"#;
    let bf = parse(input).unwrap();
    let container = bf.beams.iter().find(|b| b.name == "container").unwrap();
    let build = bf.beams.iter().find(|b| b.name == "build").unwrap();
    assert!(container.allow_failure, "allow_failure = true must be read");
    assert!(
        !build.allow_failure,
        "absence of the flag = false by default"
    );
}

#[test]
fn interpolates_var_in_command() {
    let input = r#"
variable "profile" { default = "release" }
beam "build" {
  run { commands = ["cargo build --profile ${var.profile} for ${var.profile}"] }
}
"#;
    let mut bf = parse(input).unwrap();
    resolve_variables(&mut bf).unwrap();
    assert_eq!(
        bf.beams[0].run.as_ref().unwrap().commands,
        vec!["cargo build --profile release for release"]
    );
}

#[test]
fn interpolation_leaves_shell_expansion_untouched() {
    let input = r#"
variable "profile" { default = "release" }
beam "build" {
  run { commands = ["echo ${HOME} ${var.profile} ${OTHER}"] }
}
"#;
    let mut bf = parse(input).unwrap();
    resolve_variables(&mut bf).unwrap();
    assert_eq!(
        bf.beams[0].run.as_ref().unwrap().commands,
        vec!["echo ${HOME} release ${OTHER}"]
    );
}

#[test]
fn interpolation_honors_overridden_default() {
    let input = r#"
variable "profile" { default = "debug" }
beam "build" { run { commands = ["build ${var.profile}"] } }
"#;
    let mut bf = parse(input).unwrap();
    // Simulate a --var override applied post-parse.
    bf.variables
        .iter_mut()
        .find(|v| v.name == "profile")
        .unwrap()
        .default = "release".to_string();
    resolve_variables(&mut bf).unwrap();
    assert_eq!(
        bf.beams[0].run.as_ref().unwrap().commands,
        vec!["build release"]
    );
}

#[test]
fn unknown_var_in_command_is_error() {
    let input = r#"
beam "build" { run { commands = ["build ${var.missing}"] } }
"#;
    let mut bf = parse(input).unwrap();
    let err = resolve_variables(&mut bf).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("missing"), "message names the variable: {msg}");
    assert!(msg.contains("build"), "message names the beam: {msg}");
}

#[test]
fn unknown_var_in_executor_config_is_error() {
    let input = r#"
beam "build" {
  run {
    commands = ["cargo build"]
    executor "docker" { image = var.missing }
  }
}
"#;
    let mut bf = parse(input).unwrap();
    let err = resolve_variables(&mut bf).unwrap_err();
    assert!(err.to_string().contains("missing"), "{}", err);
}

#[test]
fn test_beam_local_variable_block_is_a_migration_diagnostic() {
    // Beam-local `variable {}` was replaced by `param` with a `default`. The
    // grammar still parses the block (so the failure is a clear, named
    // diagnostic rather than a cryptic grammar error), but `parse()` must
    // reject it.
    let input = r#"
beam "deploy" {
  variable "strategy" { default = "rolling" }
  run { commands = ["deploy.sh --strategy ${var.strategy}"] }
}
"#;
    let err = parse(input).unwrap_err().to_string();
    assert!(
        err.contains("beam-local variables were replaced by `param`"),
        "got: {err}"
    );
    assert!(err.contains("deploy"), "names the beam: {err}");
}

#[test]
fn test_beam_local_variable_block_is_rejected_even_alongside_a_global() {
    // Same diagnostic fires regardless of whether a global of the same name
    // exists: there is no shadowing behavior to preserve anymore.
    let input = r#"
variable "strategy" { default = "global" }
beam "deploy" {
  variable "strategy" { default = "local" }
  run { commands = ["echo ${var.strategy}"] }
}
"#;
    let err = parse(input).unwrap_err().to_string();
    assert!(
        err.contains("beam-local variables were replaced by `param`"),
        "got: {err}"
    );
}

#[test]
fn test_beam_local_variable_block_is_rejected_in_any_beam() {
    // The diagnostic is per-beam: each offending beam's own name is reported,
    // not just the first one encountered.
    let input = r#"
beam "build"  { variable "s" { default = "fast" }    run { commands = ["echo ${var.s}"] } }
beam "deploy" { variable "s" { default = "rolling" } run { commands = ["echo ${var.s}"] } }
"#;
    let err = parse(input).unwrap_err().to_string();
    assert!(
        err.contains("beam-local variables were replaced by `param`"),
        "got: {err}"
    );
    assert!(
        err.contains("build"),
        "names the first offending beam: {err}"
    );
}
