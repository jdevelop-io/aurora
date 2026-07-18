//! Load-time expansion: monomorphizes parameterized beams into fully resolved
//! instances. Runs after variable resolution and before the DAG is built, so
//! everything downstream (scheduler, cache, TUI) keeps operating on plain
//! `Beam`s keyed by a `String` identity.

use crate::ast::{Beam, BeamFile, ConditionClause, Dependency, EnvValue};
use crate::parser::{interpolate_tokens, is_ident};
use anyhow::{anyhow, bail, Result};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Caps the length of an instantiation chain. A beam depending on itself with
/// ever-changing bindings creates an unbounded chain of distinct instances;
/// like `MAX_BEAMFILE_BYTES`, a generous fixed bound turns that runaway into a
/// clear error instead of memory exhaustion.
pub const MAX_INSTANTIATION_DEPTH: usize = 64;

#[derive(Debug)]
pub struct Expansion {
    /// Every instance of this run, ready for the scheduler: `name` is the
    /// instance id, `depends_on` carries instance ids, `bindings` is filled
    /// and every interpolatable field is resolved.
    pub instances: Vec<Beam>,
    /// The instance id of the invoked target.
    pub target_id: String,
}

/// `deploy <version> [env=staging]`: the signature shown by `--list`, the
/// picker and binding error messages. Declaration order is CLI order.
pub fn signature(beam: &Beam) -> String {
    let mut sig = beam.name.clone();
    for param in &beam.params {
        match &param.default {
            Some(default) => sig.push_str(&format!(" [{}={}]", param.name, default)),
            None => sig.push_str(&format!(" <{}>", param.name)),
        }
    }
    sig
}

pub fn has_required_params(beam: &Beam) -> bool {
    beam.params.iter().any(|p| p.default.is_none())
}

/// Escapes a binding value inside an instance id, so two distinct binding
/// sets can never collapse into the same id (the id is the identity used by
/// the scheduler, the cache and the TUI).
fn escape_id_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace(',', "\\,")
        .replace(']', "\\]")
}

/// `build` without bindings, `build[a=1,b=2]` with them (sorted by name, the
/// BTreeMap order). Stable: this string is the identity everywhere.
pub fn instance_id(beam_name: &str, bindings: &BTreeMap<String, String>) -> String {
    if bindings.is_empty() {
        return beam_name.to_string();
    }
    let bound: Vec<String> = bindings
        .iter()
        .map(|(k, v)| format!("{k}={}", escape_id_value(v)))
        .collect();
    format!("{beam_name}[{}]", bound.join(","))
}

/// Binds the CLI argument vector to `beam`'s declared params: `name=value`
/// binds by name, everything else fills the remaining params in declaration
/// order. Defaults fill the rest; a required param left unbound, a surplus
/// argument, or arguments to a param-less beam are hard errors.
pub fn bind_cli_args(beam: &Beam, args: &[String]) -> Result<BTreeMap<String, String>> {
    if beam.params.is_empty() && !args.is_empty() {
        bail!(
            "beam '{}' takes no arguments, got {}",
            beam.name,
            args.len()
        );
    }
    let mut bound: BTreeMap<String, String> = BTreeMap::new();
    let mut positional: Vec<&String> = vec![];
    for arg in args {
        if let Some((key, value)) = arg.split_once('=') {
            if beam.params.iter().any(|p| p.name == key) {
                if bound.insert(key.to_string(), value.to_string()).is_some() {
                    bail!("param '{key}' bound twice for beam '{}'", beam.name);
                }
                continue;
            }
        }
        positional.push(arg);
    }
    // Collected up front: a lazy filter would borrow `bound` across the
    // mutations below.
    let free: Vec<String> = beam
        .params
        .iter()
        .filter(|p| !bound.contains_key(&p.name))
        .map(|p| p.name.clone())
        .collect();
    let mut free = free.into_iter();
    for value in positional {
        let Some(name) = free.next() else {
            bail!(
                "too many arguments for beam '{}': usage `{}`",
                beam.name,
                signature(beam)
            );
        };
        bound.insert(name, value.clone());
    }
    for param in &beam.params {
        if bound.contains_key(&param.name) {
            continue;
        }
        match &param.default {
            Some(default) => {
                bound.insert(param.name.clone(), default.clone());
            }
            None => bail!(
                "missing required param '{}' for beam '{}': usage `{}`",
                param.name,
                beam.name,
                signature(beam)
            ),
        }
    }
    Ok(bound)
}

/// Interpolates `${param.x}` from `bindings` into `s`. `${arg...}`/`${args}`
/// get the migration diagnostic; an unbound param is a hard error; any other
/// `${...}` survives verbatim for the shell. Bound values are inserted
/// literally and never re-interpolated (same anti-injection rule as the old
/// argument pass).
fn interpolate_params(s: &str, bindings: &BTreeMap<String, String>, beam: &str) -> Result<String> {
    interpolate_tokens(s, |inner| {
        if inner == "args" || inner.strip_prefix("arg.").is_some() {
            return Some(Err(anyhow!(
                "beam '{beam}' references '${{{inner}}}': arguments were replaced by \
                 params; declare `param \"...\" {{}}` and reference `${{param.<name>}}`"
            )));
        }
        let name = inner.strip_prefix("param.")?;
        if !is_ident(name) {
            return None;
        }
        Some(match bindings.get(name) {
            Some(value) => Ok(value.clone()),
            None => Err(anyhow!(
                "unknown param '{name}' referenced in beam '{beam}'"
            )),
        })
    })
}

/// Resolves one `depends_on` edge into the child's full binding set: explicit
/// bindings (interpolated in the parent's context) plus the child's defaults.
fn bind_edge(
    parent_name: &str,
    parent_bindings: &BTreeMap<String, String>,
    dep: &Dependency,
    child: &Beam,
) -> Result<BTreeMap<String, String>> {
    let mut bound: BTreeMap<String, String> = BTreeMap::new();
    for (key, raw) in &dep.params {
        if !child.params.iter().any(|p| p.name == *key) {
            bail!(
                "beam '{parent_name}' binds unknown param '{key}' of dependency '{}'",
                child.name
            );
        }
        bound.insert(
            key.clone(),
            interpolate_params(raw, parent_bindings, parent_name)?,
        );
    }
    for param in &child.params {
        if bound.contains_key(&param.name) {
            continue;
        }
        match &param.default {
            Some(default) => {
                bound.insert(param.name.clone(), default.clone());
            }
            None => bail!(
                "dependency '{}' of beam '{parent_name}' requires param '{}': bind it \
                 with {{ beam = \"{}\", params = {{ {} = \"...\" }} }}",
                child.name,
                param.name,
                child.name,
                param.name
            ),
        }
    }
    Ok(bound)
}

/// Materializes one instance: clones the source beam, stamps the id and the
/// bindings, and interpolates `${param.x}` into every field that reaches a
/// shell or an executor.
fn instantiate(source: &Beam, id: &str, bindings: &BTreeMap<String, String>) -> Result<Beam> {
    let mut beam = source.clone();
    beam.name = id.to_string();
    beam.bindings = bindings.clone();
    let src = &source.name;
    if let Some(dir) = &mut beam.dir {
        *dir = interpolate_params(dir, bindings, src)?;
    }
    if let Some(skip_if) = &mut beam.skip_if {
        *skip_if = interpolate_params(skip_if, bindings, src)?;
    }
    if let Some(condition) = &mut beam.condition {
        for ConditionClause::Shell(clause) in &mut condition.clauses {
            *clause = interpolate_params(clause, bindings, src)?;
        }
    }
    if let Some(run) = &mut beam.run {
        for cmd in &mut run.commands {
            *cmd = interpolate_params(cmd, bindings, src)?;
        }
        if let Some(exec_cfg) = &mut run.executor {
            for value in exec_cfg.config.values_mut() {
                *value = interpolate_params(value, bindings, src)?;
            }
        }
    }
    if let Some(environment) = &mut beam.environment {
        for var in &mut environment.vars {
            match &mut var.value {
                EnvValue::Literal(s) | EnvValue::Shell(s) => {
                    *s = interpolate_params(s, bindings, src)?;
                }
            }
        }
    }
    Ok(beam)
}

/// Expands `target` (bound with `args`) and its transitive dependencies into
/// instances, then default-instantiates every remaining beam without required
/// params so the TUI sidebar can still launch it. Identical `(beam, bindings)`
/// pairs deduplicate into one instance.
pub fn expand(beam_file: &BeamFile, target: &str, args: &[String]) -> Result<Expansion> {
    let by_name: HashMap<&str, &Beam> = beam_file
        .beams
        .iter()
        .map(|b| (b.name.as_str(), b))
        .collect();
    let root = by_name
        .get(target)
        .ok_or_else(|| anyhow!("unknown beam '{target}'"))?;
    let root_bindings = bind_cli_args(root, args)?;
    let target_id = instance_id(target, &root_bindings);

    let mut instances: Vec<Beam> = vec![];
    let mut seen: HashSet<String> = HashSet::new();
    let mut worklist: Vec<(String, BTreeMap<String, String>, usize)> =
        vec![(target.to_string(), root_bindings, 0)];

    // Default instances for the launcher: pushed at depth 0 behind the run's
    // own closure. A beam with required params cannot be instantiated without
    // values, so it simply has no default instance.
    for beam in &beam_file.beams {
        if beam.name == target || has_required_params(beam) {
            continue;
        }
        let bindings = bind_cli_args(beam, &[])?;
        worklist.push((beam.name.clone(), bindings, 0));
    }

    while let Some((name, bindings, depth)) = worklist.pop() {
        let id = instance_id(&name, &bindings);
        if !seen.insert(id.clone()) {
            continue;
        }
        if depth > MAX_INSTANTIATION_DEPTH {
            bail!(
                "instantiation depth exceeded ({MAX_INSTANTIATION_DEPTH}): divergent \
                 parameter cycle involving beam '{name}'"
            );
        }
        // The target came from `by_name`; every other name was pushed from a
        // known beam's edge or the declaration list, except an unknown
        // dependency name, which is left to the DAG's error reporting.
        let Some(source) = by_name.get(name.as_str()) else {
            continue;
        };
        let mut instance = instantiate(source, &id, &bindings)?;
        let mut edges: Vec<Dependency> = vec![];
        for dep in &source.depends_on {
            let Some(child) = by_name.get(dep.beam.as_str()) else {
                // Unknown dependency: keep the raw name so `BeamGraph::from_deps`
                // reports it exactly as before.
                edges.push(Dependency::named(dep.beam.clone()));
                continue;
            };
            let child_bindings = bind_edge(&source.name, &bindings, dep, child)?;
            let child_id = instance_id(&child.name, &child_bindings);
            edges.push(Dependency::named(child_id));
            worklist.push((child.name.clone(), child_bindings, depth + 1));
        }
        instance.depends_on = edges;
        instances.push(instance);
    }

    Ok(Expansion {
        instances,
        target_id,
    })
}
