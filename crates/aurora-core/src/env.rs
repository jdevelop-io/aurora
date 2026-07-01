use crate::ast::{EnvValue, Environment};
use anyhow::{bail, Result};
use std::collections::HashMap;
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
    std::env::vars()
        .filter(|(k, _)| ENV_ALLOWLIST.contains(&k.as_str()) || k.starts_with("LC_"))
        .collect()
}

/// Evaluates the variables of the `environment` block sequentially.
/// shell(`...`) variables are executed, literals are copied as is.
/// Each variable is available to the following ones (via the `result` map).
pub fn evaluate(env_block: &Environment, working_dir: &Path) -> Result<HashMap<String, String>> {
    let mut result = base_env();

    for var in &env_block.vars {
        let value = match &var.value {
            EnvValue::Literal(s) => s.clone(),
            EnvValue::Shell(cmd) => {
                let output = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(cmd)
                    .current_dir(working_dir)
                    .env_clear()
                    .envs(&result)
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
                String::from_utf8_lossy(&output.stdout)
                    .trim_end_matches('\n')
                    .to_string()
            }
        };
        result.insert(var.name.clone(), value);
    }

    Ok(result)
}
