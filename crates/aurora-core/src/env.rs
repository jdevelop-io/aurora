use crate::ast::{EnvValue, EnvVar, Environment};
use anyhow::{bail, Result};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

/// Base environment variables carried over from the parent process.
///
/// The full environment is deliberately NOT propagated: a Beamfile can be
/// untrusted, and inheriting everything would expose secrets (CI tokens,
/// cloud keys such as `AWS_*`, etc.) to the arbitrary commands run by beams,
/// both locally and in containers (each variable is passed through
/// `docker -e`). Only these variables, needed for a shell and common tools
/// to work, are carried over. Any additional variable must be declared
/// explicitly in the `environment { }` block.
const ENV_ALLOWLIST: &[&str] = &[
    // POSIX / Unix
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "PWD",
    "TMPDIR",
    "TMP",
    "TEMP",
    "LANG",
    "LANGUAGE",
    "TERM",
    "TZ",
    "COLORTERM",
    // Windows
    "SYSTEMROOT",
    "SYSTEMDRIVE",
    "WINDIR",
    "PATHEXT",
    "COMSPEC",
    "HOMEDRIVE",
    "HOMEPATH",
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "PROGRAMFILES",
    "PROGRAMFILES(X86)",
    "PROGRAMDATA",
    "NUMBER_OF_PROCESSORS",
];

/// Builds the base environment from the allowlist (plus the `LC_*` locale
/// variables).
///
/// This is the single entry point for the ambient environment: callers must
/// use it (or [`evaluate`], which builds on it) rather than
/// `std::env::vars()`, so the allowlist is applied even when a Beamfile
/// declares no `environment { }` block.
pub fn base_env() -> HashMap<String, String> {
    // `std::env::vars()` panics while iterating if any ambient key or value is
    // not valid UTF-8, even one we would filter out. Iterate over the OS-string
    // form and skip entries that are not representable as UTF-8, so a single
    // latin-1 or binary-valued variable on the machine cannot crash Aurora.
    std::env::vars_os()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
        .filter(|(k, _)| ENV_ALLOWLIST.contains(&k.as_str()) || k.starts_with("LC_"))
        .collect()
}

/// Evaluates the variables of the `environment` block sequentially.
/// shell(`...`) variables are executed, literals are copied as is.
/// Each variable is available to the following ones (via the `result` map).
///
/// `shell(...)` values are always executed on the local host via `sh -c`,
/// independently of any beam's executor: the environment is resolved once, on
/// the host, before scheduling, and the result is then passed to every beam
/// (including Docker beams, via `docker -e`).
pub fn evaluate(env_block: &Environment, working_dir: &Path) -> Result<HashMap<String, String>> {
    let mut result = base_env();

    for var in &env_block.vars {
        let value = eval_value(var, &result, working_dir)?;
        result.insert(var.name.clone(), value);
    }

    Ok(result)
}

/// Evaluates a single `environment {}` entry: a literal is copied as is, a
/// `shell(...)` command is executed on the host with `visible` as its
/// environment. Shared by [`evaluate`] (the global block) and
/// [`evaluate_overlay`] (a beam's per-instance block), so the two stay
/// consistent on how a `shell()` value is resolved and how its failure is
/// reported.
fn eval_value(
    var: &EnvVar,
    visible: &HashMap<String, String>,
    working_dir: &Path,
) -> Result<String> {
    match &var.value {
        EnvValue::Literal(s) => Ok(s.clone()),
        EnvValue::Shell(cmd) => {
            let output = std::process::Command::new("sh")
                .arg("-c")
                .arg(cmd)
                .current_dir(working_dir)
                .env_clear()
                .envs(visible)
                .output()?;
            // A non-zero exit is a configuration error: failing here beats
            // silently binding an empty variable that would break the beams
            // relying on it in ways that are hard to diagnose.
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                bail!(
                    "environment variable '{}': shell command `{}` failed ({}){}",
                    var.name,
                    cmd,
                    output.status,
                    if stderr.trim().is_empty() {
                        String::new()
                    } else {
                        format!(": {}", stderr.trim())
                    }
                );
            }
            Ok(String::from_utf8_lossy(&output.stdout)
                .trim_end_matches('\n')
                .to_string())
        }
    }
}

/// Evaluates a beam's `environment {}` block per instance: sequential, on
/// the host, with `base` (the global environment, ambient plus declared)
/// visible to its `shell()` commands. Returns only the overlay: its values
/// shadow the global ones for this instance and leak nowhere else.
pub fn evaluate_overlay(
    env_block: &Environment,
    base: &HashMap<String, String>,
    working_dir: &Path,
) -> Result<BTreeMap<String, String>> {
    let mut visible = base.clone();
    let mut overlay = BTreeMap::new();
    for var in &env_block.vars {
        let value = eval_value(var, &visible, working_dir)?;
        visible.insert(var.name.clone(), value.clone());
        overlay.insert(var.name.clone(), value);
    }
    Ok(overlay)
}

/// Picks out of `evaluated` the variables the Beamfile's `environment {}` block
/// actually declares, as the cache key needs them.
///
/// [`evaluate`] returns the declared variables merged on top of the allowlisted
/// ambient ones, and only the declared half is part of a beam's definition. The
/// ambient half (`PATH`, `HOME`, `TERM`, `PWD`, ...) is machine context: folding
/// it into a cache key would make the key vary from one terminal or machine to
/// the next, breaking the cache locally and ruling out sharing it.
pub fn declared_only(
    env_block: Option<&Environment>,
    evaluated: &HashMap<String, String>,
) -> BTreeMap<String, String> {
    let Some(block) = env_block else {
        return BTreeMap::new();
    };
    block
        .vars
        .iter()
        .filter_map(|var| {
            evaluated
                .get(&var.name)
                .map(|value| (var.name.clone(), value.clone()))
        })
        .collect()
}
