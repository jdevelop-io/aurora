use crate::ast::{EnvValue, Environment};
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

/// Variables d'environnement de base reprises du processus parent.
///
/// On NE propage volontairement PAS l'intégralité de l'environnement : un
/// Beamfile peut être non fiable, et tout hériter exposerait des secrets
/// (tokens CI, clés cloud type `AWS_*`, etc.) aux commandes arbitraires des
/// beams, en local comme dans les conteneurs (chaque variable est transmise via
/// `docker -e`). Seules ces variables, nécessaires au fonctionnement d'un shell
/// et des outils courants, sont reprises. Toute variable supplémentaire doit
/// être déclarée explicitement dans le bloc `environment { }`.
const ENV_ALLOWLIST: &[&str] = &[
    // POSIX / Unix
    "PATH", "HOME", "USER", "LOGNAME", "SHELL", "PWD", "TMPDIR", "TMP", "TEMP",
    "LANG", "LANGUAGE", "TERM", "TZ", "COLORTERM",
    // Windows
    "SYSTEMROOT", "SYSTEMDRIVE", "WINDIR", "PATHEXT", "COMSPEC", "HOMEDRIVE",
    "HOMEPATH", "USERPROFILE", "APPDATA", "LOCALAPPDATA", "PROGRAMFILES",
    "PROGRAMFILES(X86)", "PROGRAMDATA", "NUMBER_OF_PROCESSORS",
];

/// Construit l'environnement de base à partir de la liste blanche (plus les
/// variables de locale `LC_*`).
fn base_env() -> HashMap<String, String> {
    std::env::vars()
        .filter(|(k, _)| ENV_ALLOWLIST.contains(&k.as_str()) || k.starts_with("LC_"))
        .collect()
}

/// Évalue les variables du bloc `environment` séquentiellement.
/// Les variables shell(`...`) sont exécutées, les littéraux copiés.
/// Chaque variable est disponible pour les suivantes (via la map `result`).
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
                String::from_utf8_lossy(&output.stdout).trim_end_matches('\n').to_string()
            }
        };
        result.insert(var.name.clone(), value);
    }

    Ok(result)
}
