use anyhow::Result;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
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

    /// Hashes the files matched by `patterns` (resolved against `base_dir`). A
    /// pattern that resolves to a directory is walked recursively, so listing a
    /// directory as an input covers its whole subtree; each file is hashed once
    /// even when several patterns match it.
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
                } else if path.is_dir() {
                    // A directory input means "the whole subtree": walk it and
                    // hash every file underneath. Without this, `glob` yields
                    // the directory path, `is_file()` drops it, and the input
                    // contributes nothing to the key, so editing or adding a
                    // file under a directory listed as an input would never
                    // invalidate the cache (a stale hit). The walk is iterative
                    // (explicit stack) to stay safe on deep trees.
                    let mut stack = vec![path];
                    while let Some(dir) = stack.pop() {
                        for child in fs::read_dir(&dir)? {
                            let child = child?.path();
                            if child.is_dir() {
                                stack.push(child);
                            } else if child.is_file() {
                                files.push(child);
                            }
                        }
                    }
                }
            }
        }

        if files.is_empty() {
            return Ok(None);
        }

        // Sort for a stable, order-independent hash and dedup so a file matched
        // by several patterns (for example a directory and a file inside it) is
        // hashed exactly once.
        files.sort();
        files.dedup();
        for file in files {
            let content = fs::read(&file)?;
            hasher.update(file.to_string_lossy().as_bytes());
            hasher.update(b"\0");
            hasher.update(&content);
        }

        Ok(Some(format!("{:x}", hasher.finalize())))
    }

    /// Folds a beam's definition into the hash of its `inputs` files, yielding
    /// the cache key.
    ///
    /// Hashing the inputs alone is not enough: it answers "did the data change?"
    /// while a cache must answer "would running this beam produce the same
    /// result?". Editing a command, swapping the docker image or moving the
    /// beam to another directory all change the result while leaving the input
    /// files byte-for-byte identical, and the entry recorded for the previous
    /// definition would be served instead.
    pub fn hash_with_definition(inputs_hash: &str, definition: &BeamDefinition) -> String {
        Self::key(inputs_hash, &definition.hash())
    }

    /// Combines an inputs hash with an already-computed definition hash.
    ///
    /// The two are hashed separately because they are produced at different
    /// times: the definition hash is pure CPU work on borrowed data, while the
    /// inputs hash reads whole files and runs on a blocking thread.
    pub fn key(inputs_hash: &str, definition_hash: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(inputs_hash.as_bytes());
        hasher.update(b"\0");
        hasher.update(definition_hash.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

/// Everything about a beam, beyond the content of its `inputs` files, that
/// determines what running it produces.
///
/// Global variables are deliberately absent as a separate field: `${var.x}` is
/// already interpolated into `commands` (and the other interpolatable fields)
/// by the parser before the scheduler ever sees the beam, so the resolved
/// commands capture them. Hashing them a second time would only cause
/// spurious misses. Instance param bindings are the one exception: they *are*
/// folded into the key, via the `bindings` field below.
#[derive(Default)]
pub struct BeamDefinition<'a> {
    /// The beam's `run.commands`, with `${var.x}` and `${param.x}` already
    /// resolved.
    pub commands: &'a [String],
    /// The `run.executor` name, `None` for the default `local` executor.
    pub executor: Option<&'a str>,
    /// The executor block's settings (a docker image, a network...).
    pub executor_config: Option<&'a HashMap<String, String>>,
    /// The beam's `dir`, when it declares one.
    pub dir: Option<&'a str>,
    /// The evaluated `environment {}` block **as declared by the Beamfile**.
    ///
    /// The ambient allowlisted variables (`PATH`, `HOME`, `TERM`, `PWD`...) are
    /// excluded on purpose: they are machine context, not part of the beam's
    /// definition. Folding them in would make the key vary between terminals
    /// and between machines, defeating the cache locally and ruling out the
    /// shared cache this key is meant to grow into.
    pub env: Option<&'a BTreeMap<String, String>>,
    /// The instance's resolved param bindings. Theoretically redundant (the
    /// bindings are interpolated into the hashed commands/dir/env), but folding
    /// them keeps the invariant trivial: an instance's key always covers its
    /// bindings, even for a param no hashed field references.
    pub bindings: Option<&'a BTreeMap<String, String>>,
}

impl BeamDefinition<'_> {
    /// Hashes the definition in a canonical order.
    ///
    /// Every field is length-prefixed and separated: without it, the commands
    /// `["ab", "c"]` and `["a", "bc"]` would concatenate to the same bytes and
    /// a genuine edit would be served from the cache.
    pub fn hash(&self) -> String {
        let mut hasher = Sha256::new();
        let mut field = |label: &str, value: &str| {
            hasher.update(label.as_bytes());
            hasher.update(b"\0");
            hasher.update(value.len().to_le_bytes());
            hasher.update(value.as_bytes());
            hasher.update(b"\0");
        };

        for command in self.commands {
            field("cmd", command);
        }
        field("executor", self.executor.unwrap_or("local"));
        field("dir", self.dir.unwrap_or(""));

        // A HashMap iterates in an arbitrary, run-to-run varying order: sort the
        // pairs so the same config always hashes the same.
        if let Some(config) = self.executor_config {
            let mut pairs: Vec<(&String, &String)> = config.iter().collect();
            pairs.sort();
            for (key, value) in pairs {
                field("exec-cfg", key);
                field("exec-val", value);
            }
        }

        // A BTreeMap is already ordered, so declaration order cannot leak in.
        if let Some(env) = self.env {
            for (key, value) in env {
                field("env", key);
                field("env-val", value);
            }
        }

        if let Some(bindings) = self.bindings {
            for (key, value) in bindings {
                field("param", key);
                field("param-val", value);
            }
        }

        format!("{:x}", hasher.finalize())
    }
}
