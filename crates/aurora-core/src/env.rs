use crate::ast::{EnvValue, Environment};
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

/// Évalue les variables du bloc `environment` séquentiellement.
/// Les variables shell(`...`) sont exécutées, les littéraux copiés.
/// Chaque variable est disponible pour les suivantes (via l'env du process).
pub fn evaluate(env_block: &Environment, working_dir: &Path) -> Result<HashMap<String, String>> {
    let mut result: HashMap<String, String> = std::env::vars().collect();

    for var in &env_block.vars {
        let value = match &var.value {
            EnvValue::Literal(s) => s.clone(),
            EnvValue::Shell(cmd) => {
                let output = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(cmd)
                    .current_dir(working_dir)
                    .envs(&result)
                    .output()?;
                String::from_utf8_lossy(&output.stdout).trim_end_matches('\n').to_string()
            }
        };
        // Export dans l'env courant pour que les variables suivantes y aient accès
        unsafe { std::env::set_var(&var.name, &value); }
        result.insert(var.name.clone(), value);
    }

    Ok(result)
}
