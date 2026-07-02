use crate::ast::*;
use anyhow::{bail, Context, Result};
use pest::iterators::Pair;
use pest::Parser;
use pest_derive::Parser;
use std::collections::HashMap;

#[derive(Parser)]
#[grammar = "parser/aurora.pest"]
struct AuroraParser;

/// Upper bound on a Beamfile's size. A task file is small in practice; the cap
/// guards the parser against memory and stack exhaustion on untrusted input
/// (a very large or deeply nested file), and implicitly bounds nesting depth
/// since deep nesting needs bytes.
const MAX_BEAMFILE_BYTES: usize = 1024 * 1024;

pub fn parse(input: &str) -> Result<BeamFile> {
    if input.len() > MAX_BEAMFILE_BYTES {
        bail!(
            "Beamfile too large: {} bytes (maximum {} bytes)",
            input.len(),
            MAX_BEAMFILE_BYTES
        );
    }
    let pairs = AuroraParser::parse(Rule::beamfile, input).context("Failed to parse Beamfile")?;

    let mut beam_file = BeamFile {
        config: None,
        variables: vec![],
        environment: None,
        beams: vec![],
    };

    for pair in pairs {
        if pair.as_rule() == Rule::beamfile {
            for block_pair in pair.into_inner() {
                match block_pair.as_rule() {
                    Rule::block => {
                        let inner = block_pair.into_inner().next().unwrap();
                        parse_block(inner, &mut beam_file)?;
                    }
                    Rule::EOI => {}
                    _ => {}
                }
            }
        }
    }

    Ok(beam_file)
}

/// Resolves variable references now that any `--var` override has been applied
/// to `Variable.default`.
///
/// Two forms are handled:
/// - inside `run.commands`, the embedded token `${var.<name>}` is replaced by
///   the variable's value; any other `${...}` (for example `${HOME}`) is left
///   untouched for the shell;
/// - inside an executor config, a field whose whole value is `var.<name>` is
///   replaced.
///
/// An unknown variable reference is a hard error: a silent typo inside a shell
/// command is a trap.
pub fn resolve_variables(beam_file: &mut BeamFile) -> Result<()> {
    let vars: HashMap<String, String> = beam_file
        .variables
        .iter()
        .map(|v| (v.name.clone(), v.default.clone()))
        .collect();

    for beam in &mut beam_file.beams {
        let beam_name = beam.name.clone();
        if let Some(run) = &mut beam.run {
            for cmd in &mut run.commands {
                *cmd = interpolate_command(cmd, &vars, &beam_name)?;
            }
            if let Some(exec_cfg) = &mut run.executor {
                for val in exec_cfg.config.values_mut() {
                    if let Some(var_name) = val.strip_prefix("var.") {
                        match vars.get(var_name) {
                            Some(resolved) => *val = resolved.clone(),
                            None => bail!(
                                "unknown variable '{}' referenced in beam '{}'",
                                var_name,
                                beam_name
                            ),
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// Interpolates `${var.<name>}` tokens in `s`. Non-`var` `${...}` sequences are
/// copied verbatim so shell parameter expansion still works. An unknown
/// variable is a hard error identified by `beam`.
fn interpolate_command(s: &str, vars: &HashMap<String, String>, beam: &str) -> Result<String> {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        // `$` and `{` are ASCII, so byte checks keep `i` on a char boundary.
        if bytes[i] == b'$' && i + 1 < s.len() && bytes[i + 1] == b'{' {
            if let Some(rel) = s[i + 2..].find('}') {
                let end = i + 2 + rel;
                let inner = &s[i + 2..end];
                if let Some(name) = inner.strip_prefix("var.") {
                    if is_ident(name) {
                        match vars.get(name) {
                            Some(v) => {
                                out.push_str(v);
                                i = end + 1;
                                continue;
                            }
                            None => {
                                bail!("unknown variable '{}' referenced in beam '{}'", name, beam)
                            }
                        }
                    }
                }
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    Ok(out)
}

/// True when `s` matches the grammar's `ident` rule
/// (`ASCII_ALPHA ~ (ASCII_ALPHANUMERIC | "_" | "-")*`).
fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn parse_block(pair: Pair<Rule>, bf: &mut BeamFile) -> Result<()> {
    match pair.as_rule() {
        Rule::aurora_block => bf.config = Some(parse_aurora_block(pair)?),
        Rule::variable_block => bf.variables.push(parse_variable_block(pair)?),
        Rule::environment_block => bf.environment = Some(parse_environment_block(pair)?),
        Rule::beam_block => bf.beams.push(parse_beam_block(pair)?),
        _ => {}
    }
    Ok(())
}

fn parse_aurora_block(pair: Pair<Rule>) -> Result<AuroraConfig> {
    let mut cfg = AuroraConfig {
        version: "1".to_string(),
        default: None,
        max_parallelism: None,
    };
    for field_wrapper in pair.into_inner() {
        // aurora_field is a wrapper rule: unwrap to get the actual field rule
        let field = match field_wrapper.as_rule() {
            Rule::aurora_field => field_wrapper.into_inner().next().unwrap(),
            _ => continue,
        };
        match field.as_rule() {
            Rule::aurora_version => {
                cfg.version = unquote(field.into_inner().next().unwrap());
            }
            Rule::aurora_default => {
                cfg.default = Some(unquote(field.into_inner().next().unwrap()));
            }
            Rule::aurora_parallelism => {
                cfg.max_parallelism = Some(field.into_inner().next().unwrap().as_str().parse()?);
            }
            _ => {}
        }
    }
    Ok(cfg)
}

fn parse_variable_block(pair: Pair<Rule>) -> Result<Variable> {
    let mut inner = pair.into_inner();
    let name = unquote(inner.next().unwrap());
    let mut var = Variable {
        name,
        default: String::new(),
        description: None,
    };
    for field_wrapper in inner {
        // variable_field is a wrapper rule: unwrap to get the actual field rule
        let field = match field_wrapper.as_rule() {
            Rule::variable_field => field_wrapper.into_inner().next().unwrap(),
            _ => continue,
        };
        match field.as_rule() {
            Rule::var_default => {
                var.default = unquote(field.into_inner().next().unwrap());
            }
            Rule::var_description => {
                var.description = Some(unquote(field.into_inner().next().unwrap()));
            }
            _ => {}
        }
    }
    Ok(var)
}

fn parse_environment_block(pair: Pair<Rule>) -> Result<Environment> {
    let mut vars = vec![];
    for var_pair in pair.into_inner() {
        if var_pair.as_rule() == Rule::env_var {
            let mut inner = var_pair.into_inner();
            let name = inner.next().unwrap().as_str().to_string();
            let val_pair = inner.next().unwrap();
            // val_pair is env_value, which contains shell_call or string
            let child = val_pair.into_inner().next().unwrap();
            let value = match child.as_rule() {
                Rule::shell_call => {
                    let s = unquote(child.into_inner().next().unwrap());
                    EnvValue::Shell(s)
                }
                Rule::string => EnvValue::Literal(unquote(child)),
                _ => bail!("Unexpected env_value rule: {:?}", child.as_rule()),
            };
            vars.push(EnvVar { name, value });
        }
    }
    Ok(Environment { vars })
}

fn parse_beam_block(pair: Pair<Rule>) -> Result<Beam> {
    let mut inner = pair.into_inner();
    let name = unquote(inner.next().unwrap());
    let mut beam = Beam {
        name,
        description: None,
        depends_on: vec![],
        inputs: vec![],
        outputs: vec![],
        skip_if: None,
        condition: None,
        run: None,
        allow_failure: false,
    };
    for field_wrapper in inner {
        // beam_field is a wrapper rule: unwrap to get the actual field rule
        let field = match field_wrapper.as_rule() {
            Rule::beam_field => field_wrapper.into_inner().next().unwrap(),
            _ => continue,
        };
        match field.as_rule() {
            Rule::beam_description => {
                beam.description = Some(unquote(field.into_inner().next().unwrap()));
            }
            Rule::beam_depends_on => {
                beam.depends_on = parse_string_list(field.into_inner().next().unwrap());
            }
            Rule::beam_inputs => {
                beam.inputs = parse_string_list(field.into_inner().next().unwrap());
            }
            Rule::beam_outputs => {
                beam.outputs = parse_string_list(field.into_inner().next().unwrap());
            }
            Rule::beam_skip_if => {
                beam.skip_if = Some(unquote(field.into_inner().next().unwrap()));
            }
            Rule::beam_allow_failure => {
                beam.allow_failure = field.into_inner().next().unwrap().as_str() == "true";
            }
            Rule::beam_condition => {
                beam.condition = Some(parse_condition(field)?);
            }
            Rule::beam_run => {
                beam.run = Some(parse_run(field)?);
            }
            _ => {}
        }
    }
    Ok(beam)
}

fn parse_condition(pair: Pair<Rule>) -> Result<Condition> {
    // beam_condition → condition_body → condition_any | condition_all
    let body_wrapper = pair.into_inner().next().unwrap();
    let body = match body_wrapper.as_rule() {
        Rule::condition_body => body_wrapper.into_inner().next().unwrap(),
        other => bail!("Expected condition_body, got: {:?}", other),
    };
    let op = match body.as_rule() {
        Rule::condition_any => ConditionOp::Any,
        Rule::condition_all => ConditionOp::All,
        _ => bail!("Unexpected condition body inner: {:?}", body.as_rule()),
    };
    let clauses = body
        .into_inner()
        .filter(|p| p.as_rule() == Rule::condition_clause)
        .map(|clause| {
            let shell_kv = clause.into_inner().next().unwrap();
            let s = unquote(shell_kv.into_inner().next().unwrap());
            ConditionClause::Shell(s)
        })
        .collect();
    Ok(Condition { op, clauses })
}

fn parse_run(pair: Pair<Rule>) -> Result<Run> {
    let mut commands = vec![];
    let mut executor = None;
    for field_wrapper in pair.into_inner() {
        // run_field is a wrapper rule: unwrap to get the actual field rule
        let field = match field_wrapper.as_rule() {
            Rule::run_field => field_wrapper.into_inner().next().unwrap(),
            _ => continue,
        };
        match field.as_rule() {
            Rule::run_commands => {
                commands = parse_string_list(field.into_inner().next().unwrap());
            }
            Rule::run_executor => {
                let mut inner = field.into_inner();
                let name = unquote(inner.next().unwrap());
                let mut config = HashMap::new();
                for kv in inner {
                    if kv.as_rule() == Rule::executor_field {
                        let mut kv_inner = kv.into_inner();
                        let key = kv_inner.next().unwrap().as_str().to_string();
                        let val_pair = kv_inner.next().unwrap();
                        let value = match val_pair.as_rule() {
                            Rule::string => unquote(val_pair),
                            Rule::var_ref => val_pair.as_str().to_string(),
                            _ => val_pair.as_str().to_string(),
                        };
                        config.insert(key, value);
                    }
                }
                executor = Some(ExecutorConfig { name, config });
            }
            _ => {}
        }
    }
    Ok(Run { commands, executor })
}

fn parse_string_list(pair: Pair<Rule>) -> Vec<String> {
    pair.into_inner()
        .filter(|p| p.as_rule() == Rule::string)
        .map(unquote)
        .collect()
}

/// Strips quotes from a `string` rule pair and processes escape sequences.
fn unquote(pair: Pair<Rule>) -> String {
    let raw = if pair.as_rule() == Rule::string {
        pair.into_inner().next().map(|p| p.as_str()).unwrap_or("")
    } else {
        pair.as_str()
    };
    unescape(raw)
}

/// Decodes backslash escape sequences in a single left-to-right pass.
///
/// A single pass is required for correctness: chained `.replace()` calls
/// corrupt each other because an earlier replacement can produce a sequence
/// the next one then re-interprets (for example `\\n` -> `\n` -> newline
/// instead of a literal backslash followed by `n`). An unknown escape is kept
/// verbatim (backslash and the following character).
fn unescape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}
