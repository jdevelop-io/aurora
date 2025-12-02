//! AST to Beamfile conversion.

use std::path::Path;

use aurora_core::{AuroraError, Beam, Beamfile, Condition, Hook, Result, RunBlock, Variable};

use crate::ast::*;
use crate::combinators;
use crate::lexer::span;

/// Parses a Beamfile from source content.
pub fn parse_beamfile(content: &str, path: &Path) -> Result<Beamfile> {
    let input = span(content);

    let (_, ast) = combinators::beamfile(input).map_err(|e| AuroraError::Parse {
        message: format!("Parse error: {:?}", e),
        span: None,
    })?;

    convert_ast(ast, path)
}

/// Converts the AST to a Beamfile.
fn convert_ast(ast: AstBeamfile, path: &Path) -> Result<Beamfile> {
    let mut beamfile = Beamfile::new(path);

    for item in ast.items {
        match item {
            AstItem::Variable(var) => {
                beamfile.add_variable(convert_variable(var));
            }
            AstItem::Beam(beam) => {
                beamfile.add_beam(convert_beam(beam));
            }
            AstItem::Default(name) => {
                beamfile.set_default_beam(name);
            }
        }
    }

    Ok(beamfile)
}

/// Converts an AST variable to a Variable.
fn convert_variable(ast: AstVariable) -> Variable {
    let mut var = Variable::new(ast.name);

    if let Some(default) = ast.body.get("default").and_then(|v| v.as_string()) {
        var = var.with_default(default);
    }

    if let Some(desc) = ast.body.get("description").and_then(|v| v.as_string()) {
        var = var.with_description(desc);
    }

    var
}

/// Converts an AST beam to a Beam.
fn convert_beam(ast: AstBeam) -> Beam {
    let mut beam = Beam::new(ast.name);

    for item in ast.body {
        match item {
            AstBeamItem::Description(desc) => {
                beam = beam.with_description(desc);
            }
            AstBeamItem::DependsOn(deps) => {
                beam = beam.with_depends_on(deps);
            }
            AstBeamItem::Condition(cond) => {
                beam = beam.with_condition(convert_condition(cond));
            }
            AstBeamItem::Env(env) => {
                beam = beam.with_env(env);
            }
            AstBeamItem::PreHook(hook) => {
                beam.pre_hooks.push(convert_hook(hook));
            }
            AstBeamItem::Run(run) => {
                beam = beam.with_run(convert_run(run));
            }
            AstBeamItem::PostHook(hook) => {
                beam.post_hooks.push(convert_hook(hook));
            }
            AstBeamItem::Inputs(inputs) => {
                beam = beam.with_inputs(inputs.into_iter().map(Into::into).collect());
            }
            AstBeamItem::Outputs(outputs) => {
                beam = beam.with_outputs(outputs.into_iter().map(Into::into).collect());
            }
        }
    }

    beam
}

/// Converts an AST condition to a Condition.
fn convert_condition(ast: AstCondition) -> Condition {
    match ast {
        AstCondition::FileExists(path) => Condition::file_exists(path),
        AstCondition::EnvSet(name) => Condition::env_set(name),
        AstCondition::EnvEquals { name, value } => Condition::env_equals(name, value),
        AstCondition::Command {
            run,
            expect_success,
        } => Condition::command(run, expect_success),
        AstCondition::And(conditions) => {
            Condition::and(conditions.into_iter().map(convert_condition).collect())
        }
        AstCondition::Or(conditions) => {
            Condition::or(conditions.into_iter().map(convert_condition).collect())
        }
        AstCondition::Not(condition) => Condition::negate(convert_condition(*condition)),
    }
}

/// Converts an AST hook to a Hook.
fn convert_hook(ast: AstHook) -> Hook {
    let mut hook = Hook::new(ast.commands);

    if let Some(shell) = ast.shell {
        hook = hook.with_shell(shell);
    }

    if let Some(dir) = ast.working_dir {
        hook = hook.with_working_dir(dir);
    }

    if let Some(fail) = ast.fail_on_error {
        hook = hook.fail_on_error(fail);
    }

    hook
}

/// Converts an AST run block to a RunBlock.
fn convert_run(ast: AstRun) -> RunBlock {
    let mut run = RunBlock::from_strings(ast.commands);

    if let Some(shell) = ast.shell {
        run = run.with_shell(shell);
    }

    if let Some(dir) = ast.working_dir {
        run = run.with_working_dir(dir);
    }

    if let Some(fail_fast) = ast.fail_fast {
        run = run.with_fail_fast(fail_fast);
    }

    run
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_beamfile() {
        let content = r#"
            beam "build" {
                description = "Build the project"
                run {
                    commands = ["cargo build"]
                }
            }
        "#;

        let result = parse_beamfile(content, Path::new("Beamfile")).unwrap();
        assert_eq!(result.beams.len(), 1);

        let beam = result.get_beam("build").unwrap();
        assert_eq!(beam.description.as_deref(), Some("Build the project"));
    }

    #[test]
    fn test_parse_complete_beamfile() {
        let content = r#"
            variable "mode" {
                default = "release"
                description = "Build mode"
            }

            beam "clean" {
                run {
                    commands = ["cargo clean"]
                }
            }

            beam "build" {
                description = "Build the project"
                depends_on = ["clean"]

                condition {
                    file_exists = "Cargo.toml"
                }

                env {
                    RUST_BACKTRACE = "1"
                }

                pre_hook {
                    commands = ["echo Starting..."]
                }

                run {
                    commands = [
                        "cargo build --release"
                    ]
                    shell = "bash"
                    fail_fast = true
                }

                post_hook {
                    commands = ["echo Done!"]
                }

                outputs = ["target/release/aurora"]
            }

            default = "build"
        "#;

        let result = parse_beamfile(content, Path::new("Beamfile")).unwrap();

        assert_eq!(result.variables.len(), 1);
        assert_eq!(result.beams.len(), 2);
        assert_eq!(result.default_beam.as_deref(), Some("build"));

        let var = result.get_variable("mode").unwrap();
        assert_eq!(var.default.as_deref(), Some("release"));

        let beam = result.get_beam("build").unwrap();
        assert_eq!(beam.depends_on, vec!["clean"]);
        assert!(beam.condition.is_some());
        assert_eq!(beam.pre_hooks.len(), 1);
        assert_eq!(beam.post_hooks.len(), 1);
    }
}
