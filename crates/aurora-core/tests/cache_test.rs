use aurora_core::cache::BeamCache;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_cache_miss_on_first_run() {
    let tmp = tempdir().unwrap();
    let cache = BeamCache::new(tmp.path().to_path_buf());
    assert!(!cache.is_valid("phpstan", "abc123", &[]));
}

#[test]
fn test_cache_hit_after_save() {
    let tmp = tempdir().unwrap();
    let cache = BeamCache::new(tmp.path().to_path_buf());
    cache.save("phpstan", "abc123").unwrap();
    assert!(cache.is_valid("phpstan", "abc123", &[]));
}

#[test]
fn test_cache_miss_on_hash_change() {
    let tmp = tempdir().unwrap();
    let cache = BeamCache::new(tmp.path().to_path_buf());
    cache.save("phpstan", "abc123").unwrap();
    assert!(!cache.is_valid("phpstan", "def456", &[]));
}

#[test]
fn test_cache_miss_if_output_missing() {
    let tmp = tempdir().unwrap();
    let cache = BeamCache::new(tmp.path().to_path_buf());
    cache.save("composer", "abc123").unwrap();
    let vendor = tmp.path().join("vendor").to_string_lossy().to_string();
    assert!(!cache.is_valid("composer", "abc123", &[vendor]));
}

#[test]
fn test_cache_hit_if_output_present() {
    let tmp = tempdir().unwrap();
    let vendor = tmp.path().join("vendor");
    fs::create_dir_all(&vendor).unwrap();
    let cache = BeamCache::new(tmp.path().to_path_buf());
    cache.save("composer", "abc123").unwrap();
    assert!(cache.is_valid("composer", "abc123", &[vendor.to_string_lossy().to_string()]));
}

#[test]
fn test_hash_files() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("file.txt"), b"content").unwrap();
    let cache = BeamCache::new(tmp.path().to_path_buf());
    let hash = cache.hash_inputs_at(tmp.path(), &["file.txt".to_string()]).unwrap();
    assert!(!hash.is_empty());
    let hash2 = cache.hash_inputs_at(tmp.path(), &["file.txt".to_string()]).unwrap();
    assert_eq!(hash, hash2);
}
