use crate::ast::*;
use anyhow::{bail, Context, Result};
use pest::iterators::Pair;
use pest::Parser;
use pest_derive::Parser;
use std::collections::{HashMap, HashSet};

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

    // Reject duplicate beam names up front. Downstream the beams are keyed by
    // name (the scheduler's map, the graph's index), so a duplicate silently
    // drops the earlier definition and merges its edges; surfacing it here
    // turns a copy-paste mistake into a clear error instead.
    let mut seen = HashSet::new();
    for beam in &beam_file.beams {
        if !seen.insert(beam.name.as_str()) {
            bail!("duplicate beam name '{}'", beam.name);
        }
    }

    // Reject duplicate param names within a beam: they would collide as CLI
    // positional slots and as `depends_on` binding keys.
    for beam in &beam_file.beams {
        let mut seen_params = HashSet::new();
        for param in &beam.params {
            if !seen_params.insert(param.name.as_str()) {
                bail!("duplicate param '{}' in beam '{}'", param.name, beam.name);
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
    let globals: HashMap<String, String> = beam_file
        .variables
        .iter()
        .map(|v| (v.name.clone(), v.default.clone()))
        .collect();

    for beam in &mut beam_file.beams {
        let beam_name = beam.name.clone();
        let vars = &globals;

        if let Some(dir) = &mut beam.dir {
            *dir = interpolate_command(dir, vars, &beam_name)?;
        }
        // Gates run through the shell like `run.commands`, so they take the
        // same `${var.x}` interpolation: otherwise a parameterized gate keeps
        // its literal token, becomes a bad substitution, and silently stops
        // gating.
        if let Some(skip_if) = &mut beam.skip_if {
            *skip_if = interpolate_command(skip_if, vars, &beam_name)?;
        }
        if let Some(condition) = &mut beam.condition {
            for ConditionClause::Shell(clause) in &mut condition.clauses {
                *clause = interpolate_command(clause, vars, &beam_name)?;
            }
        }
        if let Some(run) = &mut beam.run {
            for cmd in &mut run.commands {
                *cmd = interpolate_command(cmd, vars, &beam_name)?;
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
        // Bound `depends_on` values may reference `${var.x}`, e.g. forwarding
        // a global into a dependency's declared param.
        for dep in &mut beam.depends_on {
            for value in dep.params.values_mut() {
                *value = interpolate_command(value, vars, &beam_name)?;
            }
        }
    }
    Ok(())
}

/// Scans `s` for `${...}` tokens and rewrites each via `resolve`. When
/// `resolve` returns `None` the token is copied verbatim (so `${HOME}` survives
/// for the shell); `Some(Err(_))` aborts. One scanner shared by the variable
/// and argument passes, so their `${...}` handling cannot drift apart.
fn interpolate_tokens(s: &str, resolve: impl Fn(&str) -> Option<Result<String>>) -> Result<String> {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        // `$` and `{` are ASCII, so byte checks keep `i` on a char boundary.
        if bytes[i] == b'$' && i + 1 < s.len() && bytes[i + 1] == b'{' {
            if let Some(rel) = s[i + 2..].find('}') {
                let end = i + 2 + rel;
                let inner = &s[i + 2..end];
                if let Some(result) = resolve(inner) {
                    out.push_str(&result?);
                    i = end + 1;
                    continue;
                }
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    Ok(out)
}

/// Interpolates `${var.<name>}` tokens in `s`. Non-`var` `${...}` sequences are
/// copied verbatim so shell parameter expansion still works. An unknown
/// variable is a hard error identified by `beam`.
fn interpolate_command(s: &str, vars: &HashMap<String, String>, beam: &str) -> Result<String> {
    interpolate_tokens(s, |inner| {
        let name = inner.strip_prefix("var.")?;
        if !is_ident(name) {
            return None;
        }
        Some(match vars.get(name) {
            Some(v) => Ok(v.clone()),
            None => Err(anyhow::anyhow!(
                "unknown variable '{}' referenced in beam '{}'",
                name,
                beam
            )),
        })
    })
}

/// Interpolates `${arg.N}` and `${args}` in the invoked `target`'s run
/// commands, records the argument vector on that beam (so the cache key can
/// fold it in), and rejects `${arg...}` used in any beam that will actually
/// run alongside it (the target's transitive dependencies).
///
/// Arguments are target-only: a dependency is pulled in by the scheduler and
/// never receives invocation arguments, so referencing them there is a
/// configuration error rather than a silently empty expansion. A beam outside
/// the target's dependency closure never runs as part of this invocation, so
/// its own `${arg...}` (meant for when it is itself the target) is left
/// alone. Argument values are inserted literally and never re-interpolated,
/// so an argument containing `${var.x}` or `${arg.1}` is not expanded a
/// second time.
pub fn resolve_arguments(beam_file: &mut BeamFile, target: &str, args: &[String]) -> Result<()> {
    // Beams that actually run: the target and its transitive dependencies.
    // `${arg...}` is only a mistake in one of these (a dependency never receives
    // the invocation's arguments); an unrelated beam elsewhere in the Beamfile
    // is irrelevant to this run and left untouched.
    let run_closure = dependency_closure(beam_file, target);

    for beam in &mut beam_file.beams {
        let beam_name = beam.name.clone();
        if beam_name == target {
            if let Some(dir) = &mut beam.dir {
                *dir = interpolate_arguments(dir, args, &beam_name)?;
            }
            if let Some(skip_if) = &mut beam.skip_if {
                *skip_if = interpolate_arguments(skip_if, args, &beam_name)?;
            }
            if let Some(condition) = &mut beam.condition {
                for ConditionClause::Shell(clause) in &mut condition.clauses {
                    *clause = interpolate_arguments(clause, args, &beam_name)?;
                }
            }
            if let Some(run) = &mut beam.run {
                for cmd in &mut run.commands {
                    *cmd = interpolate_arguments(cmd, args, &beam_name)?;
                }
            }
            beam.args = args.to_vec();
        } else if run_closure.contains(&beam_name) {
            // Arguments are target-only, so a dependency referencing them
            // anywhere it runs a shell (gates, dir, commands) is a mistake.
            if let Some(dir) = &beam.dir {
                reject_arguments(dir, &beam_name)?;
            }
            if let Some(skip_if) = &beam.skip_if {
                reject_arguments(skip_if, &beam_name)?;
            }
            if let Some(condition) = &beam.condition {
                for ConditionClause::Shell(clause) in &condition.clauses {
                    reject_arguments(clause, &beam_name)?;
                }
            }
            if let Some(run) = &beam.run {
                for cmd in &run.commands {
                    reject_arguments(cmd, &beam_name)?;
                }
            }
        }
    }
    Ok(())
}

/// The transitive `depends_on` closure of `target`, excluding `target` itself,
/// walked iteratively. Unknown dependency names and cycles are handled
/// gracefully (the visited set bounds the walk); DAG validation itself is the
/// scheduler's job. Used only to decide which beams' `${arg...}` references
/// belong to the current run.
fn dependency_closure(beam_file: &BeamFile, target: &str) -> HashSet<String> {
    let deps: HashMap<&str, Vec<String>> = beam_file
        .beams
        .iter()
        .map(|b| (b.name.as_str(), b.dependency_names()))
        .collect();
    let mut closure = HashSet::new();
    let mut stack: Vec<String> = deps.get(target).cloned().unwrap_or_default();
    while let Some(name) = stack.pop() {
        if !closure.insert(name.clone()) {
            continue;
        }
        if let Some(next) = deps.get(name.as_str()) {
            stack.extend(next.iter().cloned());
        }
    }
    closure
}

/// Interpolates `${args}` (whole tail, space-joined) and `${arg.N}` (1-based)
/// in a single command. Other `${...}` sequences are copied verbatim.
fn interpolate_arguments(s: &str, args: &[String], beam: &str) -> Result<String> {
    interpolate_tokens(s, |inner| {
        if inner == "args" {
            return Some(Ok(args.join(" ")));
        }
        let idx = inner.strip_prefix("arg.")?;
        Some(resolve_arg_index(idx, args, beam))
    })
}

/// Resolves a single `${arg.N}` reference (1-based) to its value, or a hard
/// error for a non-numeric index, a zero index, or an out-of-range index.
fn resolve_arg_index(idx: &str, args: &[String], beam: &str) -> Result<String> {
    let n: usize = idx.parse().map_err(|_| {
        anyhow::anyhow!("invalid argument reference '${{arg.{idx}}}' in beam '{beam}'")
    })?;
    if n == 0 {
        bail!("argument index is 1-based, got '${{arg.0}}' in beam '{beam}'");
    }
    args.get(n - 1).cloned().ok_or_else(|| {
        anyhow::anyhow!(
            "missing argument '${{arg.{n}}}' in beam '{beam}': {} argument(s) provided",
            args.len()
        )
    })
}

/// Fails when a non-target beam references `${arg.N}` or `${args}`: arguments
/// are only available to the invoked target.
fn reject_arguments(s: &str, beam: &str) -> Result<()> {
    interpolate_tokens(s, |inner| {
        if inner == "args" || inner.strip_prefix("arg.").is_some() {
            Some(Err(anyhow::anyhow!(
                "beam '{beam}' references '${{{inner}}}', but arguments are only available to the invoked target"
            )))
        } else {
            None
        }
    })
    .map(|_| ())
}

/// True when `s` is a valid identifier: the grammar's `ident` rule matches the
/// whole string. Validating against the grammar rather than re-implementing the
/// character classes by hand keeps this in lockstep with `aurora.pest`, so the
/// two definitions cannot drift apart.
fn is_ident(s: &str) -> bool {
    AuroraParser::parse(Rule::ident, s)
        .ok()
        .and_then(|mut pairs| pairs.next())
        .is_some_and(|pair| pair.as_str() == s)
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
        ..Beam::default()
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
                beam.depends_on = parse_dep_list(field.into_inner().next().unwrap())?;
            }
            Rule::beam_inputs => {
                beam.inputs = parse_string_list(field.into_inner().next().unwrap());
            }
            Rule::beam_outputs => {
                beam.outputs = parse_string_list(field.into_inner().next().unwrap());
            }
            Rule::beam_dir => {
                beam.dir = Some(unquote(field.into_inner().next().unwrap()));
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
            Rule::param_block => {
                beam.params.push(parse_param_block(field)?);
            }
            Rule::environment_block => {
                beam.environment = Some(parse_environment_block(field)?);
            }
            Rule::variable_block => bail!(
                "beam '{}' declares a `variable {{}}` block: beam-local variables were \
                 replaced by `param` with a `default`",
                beam.name
            ),
            Rule::beam_run => {
                beam.run = Some(parse_run(field)?);
            }
            _ => {}
        }
    }
    Ok(beam)
}

fn parse_param_block(pair: Pair<Rule>) -> Result<Param> {
    let mut inner = pair.into_inner();
    let name = unquote(inner.next().unwrap());
    let mut param = Param {
        name,
        default: None,
        description: None,
    };
    for field_wrapper in inner {
        let field = match field_wrapper.as_rule() {
            Rule::param_field => field_wrapper.into_inner().next().unwrap(),
            _ => continue,
        };
        match field.as_rule() {
            Rule::param_default => {
                param.default = Some(unquote(field.into_inner().next().unwrap()));
            }
            Rule::param_description => {
                param.description = Some(unquote(field.into_inner().next().unwrap()));
            }
            _ => {}
        }
    }
    Ok(param)
}

fn parse_dep_list(pair: Pair<Rule>) -> Result<Vec<Dependency>> {
    let mut deps = vec![];
    for entry in pair.into_inner() {
        if entry.as_rule() != Rule::dep_entry {
            continue;
        }
        let inner = entry.into_inner().next().unwrap();
        match inner.as_rule() {
            Rule::string => deps.push(Dependency::named(unquote(inner))),
            Rule::dep_object => {
                let mut parts = inner.into_inner();
                let beam = unquote(parts.next().unwrap());
                let mut params = std::collections::BTreeMap::new();
                if let Some(dep_params) = parts.next() {
                    for binding in dep_params.into_inner() {
                        if binding.as_rule() != Rule::dep_binding {
                            continue;
                        }
                        let mut kv = binding.into_inner();
                        let key = kv.next().unwrap().as_str().to_string();
                        let value = unquote(kv.next().unwrap());
                        if params.insert(key.clone(), value).is_some() {
                            bail!("param '{key}' bound twice in a depends_on entry for '{beam}'");
                        }
                    }
                }
                deps.push(Dependency { beam, params });
            }
            other => bail!("unexpected depends_on entry: {other:?}"),
        }
    }
    Ok(deps)
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
