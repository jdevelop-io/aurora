use anyhow::Result;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(serde::Serialize, serde::Deserialize)]
struct CacheEntry {
    inputs_hash: String,
    #[serde(default)]
    stdout: Vec<String>,
    #[serde(default)]
    stderr: Vec<String>,
}

pub struct BeamCache {
    cache_dir: PathBuf,
}

/// Transforme un nom de beam (potentiellement contrôlé par un Beamfile non
/// fiable) en nom de fichier sûr, confiné au répertoire de cache.
///
/// Sans cette normalisation, un nom comme `/etc/cron.d/x` ou `../../.ssh/x`
/// ferait écrire/supprimer un fichier hors du cache via `PathBuf::join`
/// (path traversal). On remplace tout caractère non sûr et on suffixe un hash
/// du nom d'origine : la lisibilité est conservée pour les noms simples, et
/// l'unicité est garantie même en cas de collision de la sanitisation.
fn safe_file_stem(beam_name: &str) -> String {
    let sanitized: String = beam_name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '_' })
        .take(64)
        .collect();
    let mut hasher = Sha256::new();
    hasher.update(beam_name.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    format!("{}-{}", sanitized, &hash[..16])
}

impl BeamCache {
    pub fn new(cache_dir: PathBuf) -> Self {
        fs::create_dir_all(&cache_dir).ok();
        Self { cache_dir }
    }

    fn entry_path(&self, beam_name: &str) -> PathBuf {
        self.cache_dir.join(format!("{}.json", safe_file_stem(beam_name)))
    }

    pub fn is_valid(&self, beam_name: &str, inputs_hash: &str, outputs: &[String]) -> bool {
        let Ok(content) = fs::read_to_string(self.entry_path(beam_name)) else {
            return false;
        };
        let Ok(entry) = serde_json::from_str::<CacheEntry>(&content) else {
            return false;
        };
        if entry.inputs_hash != inputs_hash {
            return false;
        }
        outputs.iter().all(|out| Path::new(out).exists())
    }

    pub fn save(&self, beam_name: &str, inputs_hash: &str) -> Result<()> {
        self.save_with_logs(beam_name, inputs_hash, &[], &[])
    }

    pub fn save_with_logs(
        &self,
        beam_name: &str,
        inputs_hash: &str,
        stdout: &[String],
        stderr: &[String],
    ) -> Result<()> {
        let entry = CacheEntry {
            inputs_hash: inputs_hash.to_string(),
            stdout: stdout.to_vec(),
            stderr: stderr.to_vec(),
        };
        let content = serde_json::to_string_pretty(&entry)?;
        fs::write(self.entry_path(beam_name), content)?;
        Ok(())
    }

    /// Retourne (stdout, stderr) depuis le cache, ou ([], []) si absent.
    pub fn load_logs(&self, beam_name: &str) -> (Vec<String>, Vec<String>) {
        let Ok(content) = fs::read_to_string(self.entry_path(beam_name)) else {
            return (vec![], vec![]);
        };
        let Ok(entry) = serde_json::from_str::<CacheEntry>(&content) else {
            return (vec![], vec![]);
        };
        (entry.stdout, entry.stderr)
    }

    pub fn invalidate(&self, beam_name: &str) -> Result<()> {
        let path = self.entry_path(beam_name);
        if path.exists() { fs::remove_file(path)?; }
        Ok(())
    }

    pub fn hash_inputs_at(&self, base_dir: &Path, patterns: &[String]) -> Result<String> {
        let mut hasher = Sha256::new();
        let mut files: Vec<PathBuf> = vec![];

        for pattern in patterns {
            let full_pattern = base_dir.join(pattern).to_string_lossy().to_string();
            for entry in glob::glob(&full_pattern)? {
                let path = entry?;
                if path.is_file() { files.push(path); }
            }
        }

        files.sort();
        for file in files {
            let content = fs::read(&file)?;
            hasher.update(file.to_string_lossy().as_bytes());
            hasher.update(b"\0");
            hasher.update(&content);
        }

        Ok(format!("{:x}", hasher.finalize()))
    }
}
