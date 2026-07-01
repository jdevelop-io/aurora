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
    let vendor = tmp.path().join("vendor");
    fs::create_dir_all(&vendor).unwrap();
    let cache = BeamCache::new(tmp.path().to_path_buf());
    cache.save("composer", "abc123").unwrap();
    assert!(cache.is_valid(
        "composer",
        "abc123",
        &[vendor.to_string_lossy().to_string()],
        tmp.path()
    ));
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
