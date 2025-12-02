//! Build cache for avoiding redundant beam execution.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use aurora_core::{Beam, Result};
use serde::{Deserialize, Serialize};

/// Manages the build cache for beams.
pub struct BuildCache {
    /// Directory where cache files are stored.
    cache_dir: PathBuf,

    /// In-memory cache of entries.
    entries: HashMap<String, CacheEntry>,
}

/// A cache entry for a single beam.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// Beam name.
    pub beam_name: String,

    /// Hash of input files.
    pub input_hashes: HashMap<PathBuf, String>,

    /// Hash of output files.
    pub output_hashes: HashMap<PathBuf, String>,

    /// Hash of the commands.
    pub command_hash: String,

    /// Timestamp of last successful execution.
    pub timestamp: u64,
}

impl BuildCache {
    /// Creates a new build cache.
    pub fn new(cache_dir: impl Into<PathBuf>) -> Result<Self> {
        let cache_dir = cache_dir.into();

        // Create cache directory if it doesn't exist
        if !cache_dir.exists() {
            fs::create_dir_all(&cache_dir)?;
        }

        // Load existing cache entries
        let entries = Self::load_entries(&cache_dir)?;

        Ok(Self { cache_dir, entries })
    }

    /// Loads cache entries from disk.
    fn load_entries(cache_dir: &Path) -> Result<HashMap<String, CacheEntry>> {
        let cache_file = cache_dir.join("cache.json");

        if !cache_file.exists() {
            return Ok(HashMap::new());
        }

        let content = fs::read_to_string(&cache_file)?;
        let entries: HashMap<String, CacheEntry> =
            serde_json::from_str(&content).unwrap_or_default();

        Ok(entries)
    }

    /// Saves cache entries to disk.
    fn save_entries(&self) -> Result<()> {
        let cache_file = self.cache_dir.join("cache.json");
        let content = serde_json::to_string_pretty(&self.entries).map_err(std::io::Error::other)?;
        fs::write(&cache_file, content)?;
        Ok(())
    }

    /// Checks if a beam is up to date (doesn't need re-execution).
    pub fn is_up_to_date(&self, beam: &Beam, working_dir: &Path) -> bool {
        let entry = match self.entries.get(&beam.name) {
            Some(e) => e,
            None => return false,
        };

        // Check command hash
        let current_command_hash = self.hash_commands(beam);
        if entry.command_hash != current_command_hash {
            return false;
        }

        // Check input file hashes
        for input in &beam.inputs {
            let path = working_dir.join(input);
            let current_hash = match Self::hash_file(&path) {
                Ok(h) => h,
                Err(_) => return false,
            };

            match entry.input_hashes.get(input) {
                Some(cached_hash) if cached_hash == &current_hash => {}
                _ => return false,
            }
        }

        // Check output files exist and haven't changed
        for output in &beam.outputs {
            let path = working_dir.join(output);

            if !path.exists() {
                return false;
            }

            let current_hash = match Self::hash_file(&path) {
                Ok(h) => h,
                Err(_) => return false,
            };

            match entry.output_hashes.get(output) {
                Some(cached_hash) if cached_hash == &current_hash => {}
                _ => return false,
            }
        }

        true
    }

    /// Records a successful beam execution.
    pub fn record(&mut self, beam: &Beam, working_dir: &Path) -> Result<()> {
        let mut input_hashes = HashMap::new();
        for input in &beam.inputs {
            let path = working_dir.join(input);
            if let Ok(hash) = Self::hash_file(&path) {
                input_hashes.insert(input.clone(), hash);
            }
        }

        let mut output_hashes = HashMap::new();
        for output in &beam.outputs {
            let path = working_dir.join(output);
            if let Ok(hash) = Self::hash_file(&path) {
                output_hashes.insert(output.clone(), hash);
            }
        }

        let entry = CacheEntry {
            beam_name: beam.name.clone(),
            input_hashes,
            output_hashes,
            command_hash: self.hash_commands(beam),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        self.entries.insert(beam.name.clone(), entry);
        self.save_entries()?;

        Ok(())
    }

    /// Clears all cache entries.
    pub fn clear(&mut self) -> Result<()> {
        self.entries.clear();
        self.save_entries()?;
        Ok(())
    }

    /// Invalidates a specific beam's cache.
    pub fn invalidate(&mut self, beam_name: &str) -> Result<()> {
        self.entries.remove(beam_name);
        self.save_entries()?;
        Ok(())
    }

    /// Hashes a file using blake3.
    fn hash_file(path: &Path) -> Result<String> {
        let content = fs::read(path)?;
        let hash = blake3::hash(&content);
        Ok(hash.to_hex().to_string())
    }

    /// Hashes the commands of a beam.
    fn hash_commands(&self, beam: &Beam) -> String {
        let commands_str: String = beam
            .run
            .as_ref()
            .map(|r| {
                r.commands
                    .iter()
                    .map(|c| c.command.as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();

        let hash = blake3::hash(commands_str.as_bytes());
        hash.to_hex().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_cache_creation() {
        let dir = tempdir().unwrap();
        let cache = BuildCache::new(dir.path().join(".aurora/cache")).unwrap();
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn test_cache_record_and_check() {
        let dir = tempdir().unwrap();
        let cache_path = dir.path().join(".aurora/cache");
        let mut cache = BuildCache::new(&cache_path).unwrap();

        // Create a test file
        let test_file = dir.path().join("input.txt");
        fs::write(&test_file, "test content").unwrap();

        let beam = Beam::new("test").with_inputs(vec![PathBuf::from("input.txt")]);

        // Initially not up to date
        assert!(!cache.is_up_to_date(&beam, dir.path()));

        // Record execution
        cache.record(&beam, dir.path()).unwrap();

        // Now should be up to date
        assert!(cache.is_up_to_date(&beam, dir.path()));

        // Modify input file
        fs::write(&test_file, "modified content").unwrap();

        // Should no longer be up to date
        assert!(!cache.is_up_to_date(&beam, dir.path()));
    }
}
