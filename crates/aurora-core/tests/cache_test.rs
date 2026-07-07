use aurora_core::cache::BeamCache;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_cache_miss_on_first_run() {
    let tmp = tempdir().unwrap();
    let cache = BeamCache::new(tmp.path().to_path_buf());
    assert!(!cache.is_valid("phpstan", "abc123", &[], tmp.path()));
}

#[test]
fn test_new_does_not_create_dir_until_save() {
    let tmp = tempdir().unwrap();
    let cache_dir = tmp.path().join("not-yet");
    let cache = BeamCache::new(cache_dir.clone());
    assert!(
        !cache_dir.exists(),
        "the cache directory must not be created eagerly (e.g. under --no-cache)"
    );
    cache.save("b", "h").unwrap();
    assert!(cache_dir.exists(), "first save creates the cache directory");
}

#[test]
fn test_cache_hit_after_save() {
    let tmp = tempdir().unwrap();
    let cache = BeamCache::new(tmp.path().to_path_buf());
    cache.save("phpstan", "abc123").unwrap();
    assert!(cache.is_valid("phpstan", "abc123", &[], tmp.path()));
}

#[test]
fn test_cache_miss_on_hash_change() {
    let tmp = tempdir().unwrap();
    let cache = BeamCache::new(tmp.path().to_path_buf());
    cache.save("phpstan", "abc123").unwrap();
    assert!(!cache.is_valid("phpstan", "def456", &[], tmp.path()));
}

#[test]
fn test_cache_miss_if_output_missing() {
    let tmp = tempdir().unwrap();
    let cache = BeamCache::new(tmp.path().to_path_buf());
    cache.save("composer", "abc123").unwrap();
    let vendor = tmp.path().join("vendor").to_string_lossy().to_string();
    assert!(!cache.is_valid("composer", "abc123", &[vendor], tmp.path()));
}

#[test]
fn test_cache_hit_if_output_present() {
    let tmp = tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("vendor")).unwrap();
    let cache = BeamCache::new(tmp.path().to_path_buf());
    cache.save("composer", "abc123").unwrap();
    // Outputs are declared relative to the Beamfile directory, exactly like
    // inputs; resolved against base_dir it points at the existing directory.
    assert!(cache.is_valid("composer", "abc123", &["vendor".to_string()], tmp.path()));
}

#[test]
fn test_cache_miss_on_absolute_output_escaping_base_dir() {
    let tmp = tempdir().unwrap();
    let base = tmp.path().join("project");
    fs::create_dir_all(&base).unwrap();
    let cache = BeamCache::new(base.join(".aurora/cache"));
    cache.save("b", "abc123").unwrap();

    // A file that exists but lives outside base_dir.
    let outside = tmp.path().join("outside.txt");
    fs::write(&outside, "x").unwrap();

    // Even though the absolute output exists on disk, it escapes base_dir, so
    // the entry must be treated as invalid (a cache miss) instead of becoming
    // an existence oracle for arbitrary filesystem paths from an untrusted
    // Beamfile.
    assert!(!cache.is_valid(
        "b",
        "abc123",
        &[outside.to_string_lossy().to_string()],
        &base
    ));
}

#[test]
fn test_cache_miss_on_parent_dir_output_escaping_base_dir() {
    let tmp = tempdir().unwrap();
    let base = tmp.path().join("project");
    fs::create_dir_all(&base).unwrap();
    let cache = BeamCache::new(base.join(".aurora/cache"));
    cache.save("b", "abc123").unwrap();

    // Exists in the parent of base_dir, reachable only via `..`.
    fs::write(tmp.path().join("outside.txt"), "x").unwrap();

    assert!(!cache.is_valid("b", "abc123", &["../outside.txt".to_string()], &base));
}

#[test]
fn test_relative_output_resolves_against_base_dir() {
    let tmp = tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("dist")).unwrap();
    let cache = BeamCache::new(tmp.path().join(".aurora/cache"));
    cache.save("build", "abc123").unwrap();
    // A relative output must be resolved against base_dir, not the process
    // working directory.
    assert!(cache.is_valid("build", "abc123", &["dist".to_string()], tmp.path()));
    assert!(!cache.is_valid("build", "abc123", &["missing".to_string()], tmp.path()));
}

#[test]
fn test_malicious_beam_name_stays_in_cache_dir() {
    let tmp = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let cache = BeamCache::new(tmp.path().to_path_buf());

    // Names attempting a path traversal / absolute path.
    let evil_abs = outside.path().join("pwned").to_string_lossy().to_string();
    for name in [
        "../../../../etc/cron.d/evil",
        "/etc/cron.d/evil",
        "..",
        evil_abs.as_str(),
    ] {
        cache.save(name, "h").unwrap();
        // No file must appear outside the cache directory.
        assert!(
            !std::path::Path::new(&format!("{}.json", evil_abs)).exists(),
            "write outside the cache for {name}"
        );
    }

    // All written entries stay confined to the cache directory.
    for entry in fs::read_dir(tmp.path()).unwrap() {
        let path = entry.unwrap().path();
        assert_eq!(path.parent().unwrap(), tmp.path());
    }
}

#[test]
fn test_save_and_load_logs_round_trip() {
    let tmp = tempdir().unwrap();
    let cache = BeamCache::new(tmp.path().to_path_buf());
    let stdout = vec!["out line 1".to_string(), "out line 2".to_string()];
    let stderr = vec!["err line".to_string()];

    cache.save_with_logs("b", "h", &stdout, &stderr).unwrap();
    let (loaded_out, loaded_err) = cache.load_logs("b");

    assert_eq!(loaded_out, stdout);
    assert_eq!(loaded_err, stderr);
}

#[test]
fn test_load_logs_absent_entry_is_empty() {
    let tmp = tempdir().unwrap();
    let cache = BeamCache::new(tmp.path().to_path_buf());
    assert_eq!(cache.load_logs("missing"), (vec![], vec![]));
}

#[test]
fn test_hash_files() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("file.txt"), b"content").unwrap();
    let cache = BeamCache::new(tmp.path().to_path_buf());
    let hash = cache
        .hash_inputs_at(tmp.path(), &["file.txt".to_string()])
        .unwrap();
    assert!(hash.is_some());
    let hash2 = cache
        .hash_inputs_at(tmp.path(), &["file.txt".to_string()])
        .unwrap();
    assert_eq!(hash, hash2);
}

#[test]
fn test_hash_inputs_rejects_paths_outside_base_dir() {
    let tmp = tempdir().unwrap();
    let cache = BeamCache::new(tmp.path().to_path_buf());
    // An absolute input pattern would read files outside the Beamfile
    // directory: it must be rejected.
    assert!(cache
        .hash_inputs_at(tmp.path(), &["/etc/hosts".to_string()])
        .is_err());
    // A parent-directory traversal likewise escapes base_dir.
    assert!(cache
        .hash_inputs_at(tmp.path(), &["../secret".to_string()])
        .is_err());
}

#[test]
fn test_hash_inputs_none_when_no_file_matches() {
    let tmp = tempdir().unwrap();
    let cache = BeamCache::new(tmp.path().to_path_buf());
    // Declared inputs that match no file must be a miss (None), not a hash of
    // nothing that would keep the beam permanently cached.
    let hash = cache
        .hash_inputs_at(tmp.path(), &["does-not-exist-*.txt".to_string()])
        .unwrap();
    assert!(hash.is_none());
}

#[test]
fn test_hash_with_args_empty_is_identity() {
    assert_eq!(BeamCache::hash_with_args("abc123", &[]), "abc123");
}

#[test]
fn test_hash_with_args_differs_by_arguments() {
    let a = BeamCache::hash_with_args("abc123", &["web-01".to_string()]);
    let b = BeamCache::hash_with_args("abc123", &["web-02".to_string()]);
    assert_ne!(a, b, "different arguments must produce different keys");
    assert_ne!(a, "abc123", "arguments must change the key");
}

#[test]
fn test_hash_with_args_is_stable_and_order_sensitive() {
    let args1 = vec!["a".to_string(), "b".to_string()];
    let args2 = vec!["a".to_string(), "b".to_string()];
    let reordered = vec!["b".to_string(), "a".to_string()];
    assert_eq!(
        BeamCache::hash_with_args("h", &args1),
        BeamCache::hash_with_args("h", &args2),
        "same arguments hash the same"
    );
    assert_ne!(
        BeamCache::hash_with_args("h", &args1),
        BeamCache::hash_with_args("h", &reordered),
        "argument order matters"
    );
}
