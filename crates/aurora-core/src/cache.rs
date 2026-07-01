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

/// Turns a beam name (potentially controlled by an untrusted Beamfile) into a
/// safe file name, confined to the cache directory.
///
/// Without this normalization, a name like `/etc/cron.d/x` or `../../.ssh/x`
/// would write/delete a file outside the cache via `PathBuf::join` (path
/// traversal). Every unsafe character is replaced and a hash of the original
/// name is appended as a suffix: readability is preserved for simple names,
/// and uniqueness is guaranteed even if the sanitization collides.
fn safe_file_stem(beam_name: &str) -> String {
    let sanitized: String = beam_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    let mut hasher = Sha256::new();
    hasher.update(beam_name.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    format!("{}-{}", sanitized, &hash[..16])
}

/// True when a declared input/output pattern would resolve outside `base_dir`
/// (the Beamfile directory): an absolute path, which `PathBuf::join` lets
/// replace the base entirely, or one containing a `..` component. A Beamfile
/// is untrusted, so such a pattern must never reach the filesystem outside its
/// own directory.
fn escapes_base_dir(pattern: &str) -> bool {
    let candidate = Path::new(pattern);
    candidate.is_absolute()
        || candidate
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
}

impl BeamCache {
    /// Creates a cache handle. The directory is created lazily on the first
    /// write, so a run that never persists anything (for example `--no-cache`)
    /// leaves no `.aurora/cache` directory behind.
    pub fn new(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }

    fn entry_path(&self, beam_name: &str) -> PathBuf {
        self.cache_dir
            .join(format!("{}.json", safe_file_stem(beam_name)))
    }

    /// Reads and deserializes a beam's cache entry, or `None` when it is absent
    /// or malformed (a corrupt entry is treated as a miss, never a hard error).
    fn read_entry(&self, beam_name: &str) -> Option<CacheEntry> {
        let content = fs::read_to_string(self.entry_path(beam_name)).ok()?;
        serde_json::from_str::<CacheEntry>(&content).ok()
    }

    pub fn is_valid(
        &self,
        beam_name: &str,
        inputs_hash: &str,
        outputs: &[String],
        base_dir: &Path,
    ) -> bool {
        let Some(entry) = self.read_entry(beam_name) else {
            return false;
        };
        if entry.inputs_hash != inputs_hash {
            return false;
        }
        // Resolve outputs against `base_dir` (the Beamfile directory), exactly
        // like inputs in `hash_inputs_at`. A relative output would otherwise be
        // checked against the process working directory, so a valid cache entry
        // could be wrongly rejected when Aurora is invoked from a subdirectory.
        // An output that escapes base_dir (absolute or `..`) is treated as a
        // miss rather than probed on disk, so the check cannot become an
        // existence oracle for arbitrary paths from an untrusted Beamfile.
        outputs
            .iter()
            .all(|out| !escapes_base_dir(out) && base_dir.join(out).exists())
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
        fs::create_dir_all(&self.cache_dir)?;
        fs::write(self.entry_path(beam_name), content)?;
        Ok(())
    }

    /// Returns (stdout, stderr) from the cache, or ([], []) if absent.
    pub fn load_logs(&self, beam_name: &str) -> (Vec<String>, Vec<String>) {
        match self.read_entry(beam_name) {
            Some(entry) => (entry.stdout, entry.stderr),
            None => (vec![], vec![]),
        }
    }

    /// Hashes the files matched by `patterns` (resolved against `base_dir`).
    ///
    /// Returns `None` when no file matches: with declared inputs but nothing on
    /// disk, hashing yields the empty-hasher constant, which combined with
    /// present outputs would make the beam permanently cached. `None` means
    /// "cannot key the cache", i.e. a miss, so the beam runs.
    pub fn hash_inputs_at(&self, base_dir: &Path, patterns: &[String]) -> Result<Option<String>> {
        let mut hasher = Sha256::new();
        let mut files: Vec<PathBuf> = vec![];

        for pattern in patterns {
            // Confine inputs to the Beamfile directory: an absolute pattern or
            // a `..` traversal (from an untrusted Beamfile) would otherwise
            // read files outside base_dir via `PathBuf::join`.
            if escapes_base_dir(pattern) {
                anyhow::bail!("input pattern escapes the Beamfile directory: {pattern}");
            }
            let full_pattern = base_dir.join(pattern).to_string_lossy().to_string();
            for entry in glob::glob(&full_pattern)? {
                let path = entry?;
                if path.is_file() {
                    files.push(path);
                }
            }
        }

        if files.is_empty() {
            return Ok(None);
        }

        files.sort();
        for file in files {
            let content = fs::read(&file)?;
            hasher.update(file.to_string_lossy().as_bytes());
            hasher.update(b"\0");
            hasher.update(&content);
        }

        Ok(Some(format!("{:x}", hasher.finalize())))
    }
}
