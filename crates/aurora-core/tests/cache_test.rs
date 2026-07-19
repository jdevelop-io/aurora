use aurora_core::cache::{BeamCache, BeamDefinition};
use std::collections::{BTreeMap, HashMap};
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
        .unwrap()
        .hash;
    assert!(hash.is_some());
    let hash2 = cache
        .hash_inputs_at(tmp.path(), &["file.txt".to_string()])
        .unwrap()
        .hash;
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
        .unwrap()
        .hash;
    assert!(hash.is_none());
}

#[test]
fn test_directory_input_hashes_its_files_recursively() {
    let tmp = tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("tests/integration")).unwrap();
    fs::write(tmp.path().join("tests/a.rs"), b"// a").unwrap();
    fs::write(tmp.path().join("tests/integration/b.rs"), b"// b").unwrap();
    let cache = BeamCache::new(tmp.path().to_path_buf());

    // A bare directory input must hash every file underneath it, so declaring
    // "tests" covers the whole tree without an explicit recursive glob.
    let hash = cache
        .hash_inputs_at(tmp.path(), &["tests".to_string()])
        .unwrap()
        .hash;
    assert!(
        hash.is_some(),
        "a directory input with files must produce a key"
    );
}

#[test]
fn test_directory_input_invalidates_when_a_nested_file_is_added() {
    let tmp = tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("tests")).unwrap();
    fs::write(tmp.path().join("tests/a.rs"), b"// a").unwrap();
    let cache = BeamCache::new(tmp.path().to_path_buf());
    let inputs = vec!["tests".to_string()];

    let before = cache.hash_inputs_at(tmp.path(), &inputs).unwrap().hash;
    // A new test file appears in a subdirectory of the declared directory.
    fs::create_dir_all(tmp.path().join("tests/integration")).unwrap();
    fs::write(tmp.path().join("tests/integration/new.rs"), b"// new").unwrap();
    let after = cache.hash_inputs_at(tmp.path(), &inputs).unwrap().hash;

    assert_ne!(
        before, after,
        "adding a nested test file must change a directory input's hash"
    );
}

#[test]
fn test_directory_and_file_input_do_not_double_count_overlap() {
    let tmp = tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("src")).unwrap();
    fs::write(tmp.path().join("src/main.rs"), b"fn main() {}").unwrap();
    let cache = BeamCache::new(tmp.path().to_path_buf());

    // "src" (recursed) and "src/main.rs" resolve to the same file; it must be
    // hashed once, so the pair equals hashing the directory alone.
    let both = cache
        .hash_inputs_at(tmp.path(), &["src".to_string(), "src/main.rs".to_string()])
        .unwrap()
        .hash;
    let dir_only = cache
        .hash_inputs_at(tmp.path(), &["src".to_string()])
        .unwrap()
        .hash;
    assert_eq!(
        both, dir_only,
        "an overlapping file must not be counted twice"
    );
}

#[test]
fn test_hash_inputs_reports_dead_patterns() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("live.txt"), b"x").unwrap();
    let cache = BeamCache::new(tmp.path().to_path_buf());

    // One pattern matches a file, the other matches nothing on disk. The live
    // one still keys the cache; the dead one is reported so a typo'd pattern
    // silently protecting nothing can be surfaced.
    let result = cache
        .hash_inputs_at(
            tmp.path(),
            &["live.txt".to_string(), "missing/*.rs".to_string()],
        )
        .unwrap();
    assert!(result.hash.is_some(), "a live pattern still produces a key");
    assert_eq!(
        result.dead_patterns,
        vec!["missing/*.rs".to_string()],
        "the pattern contributing no file is reported, the live one is not"
    );
}

#[test]
fn test_hash_inputs_empty_directory_pattern_is_dead() {
    let tmp = tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("empty")).unwrap();
    let cache = BeamCache::new(tmp.path().to_path_buf());

    // The pattern matches a directory, but it holds no file: it contributes
    // nothing to the key and must count as dead.
    let result = cache
        .hash_inputs_at(tmp.path(), &["empty".to_string()])
        .unwrap();
    assert!(result.hash.is_none());
    assert_eq!(result.dead_patterns, vec!["empty".to_string()]);
}

/// A definition running `cmd`, everything else left at its default.
fn definition(commands: &[String]) -> BeamDefinition<'_> {
    BeamDefinition {
        commands,
        ..Default::default()
    }
}

#[test]
fn test_definition_hash_folds_into_the_inputs_hash() {
    let cmds = vec!["make build".to_string()];
    let key = BeamCache::hash_with_definition("abc123", &definition(&cmds));
    assert_ne!(
        key, "abc123",
        "the definition must always take part in the key"
    );
}

#[test]
fn test_definition_hash_changes_with_the_commands() {
    // The regression this whole key exists for: editing a command must never
    // be served from the cache entry recorded for the previous command.
    let v1 = vec!["echo VERSION-ONE".to_string()];
    let v2 = vec!["echo VERSION-TWO".to_string()];
    assert_ne!(
        BeamCache::hash_with_definition("h", &definition(&v1)),
        BeamCache::hash_with_definition("h", &definition(&v2)),
        "a changed command must invalidate the entry"
    );
}

#[test]
fn test_definition_hash_changes_with_command_order_and_count() {
    let one = vec!["a".to_string()];
    let two = vec!["a".to_string(), "b".to_string()];
    let reordered = vec!["b".to_string(), "a".to_string()];
    assert_ne!(
        BeamCache::hash_with_definition("h", &definition(&one)),
        BeamCache::hash_with_definition("h", &definition(&two)),
        "an added command must invalidate the entry"
    );
    assert_ne!(
        BeamCache::hash_with_definition("h", &definition(&two)),
        BeamCache::hash_with_definition("h", &definition(&reordered)),
        "command order matters"
    );
}

#[test]
fn test_definition_hash_is_not_confused_by_command_concatenation() {
    // Without a separator between commands, ["ab", "c"] and ["a", "bc"] would
    // hash the same, so a real edit could be served from the cache.
    let a = vec!["ab".to_string(), "c".to_string()];
    let b = vec!["a".to_string(), "bc".to_string()];
    assert_ne!(
        BeamCache::hash_with_definition("h", &definition(&a)),
        BeamCache::hash_with_definition("h", &definition(&b)),
        "commands must be separated before hashing"
    );
}

#[test]
fn test_definition_hash_changes_with_the_executor() {
    let cmds = vec!["make".to_string()];
    let local = BeamDefinition {
        commands: &cmds,
        executor: None,
        ..Default::default()
    };
    let docker = BeamDefinition {
        commands: &cmds,
        executor: Some("docker"),
        ..Default::default()
    };
    assert_ne!(
        BeamCache::hash_with_definition("h", &local),
        BeamCache::hash_with_definition("h", &docker),
        "the same command run through another executor is another result"
    );
}

#[test]
fn test_definition_hash_changes_with_the_executor_config() {
    let cmds = vec!["make".to_string()];
    let v1: HashMap<String, String> = [("image".to_string(), "rust:1.80".to_string())].into();
    let v2: HashMap<String, String> = [("image".to_string(), "rust:1.90".to_string())].into();
    let with = |cfg: &HashMap<String, String>| {
        BeamCache::hash_with_definition(
            "h",
            &BeamDefinition {
                commands: &cmds,
                executor: Some("docker"),
                executor_config: Some(cfg),
                ..Default::default()
            },
        )
    };
    assert_ne!(
        with(&v1),
        with(&v2),
        "another docker image is another result"
    );
}

#[test]
fn test_definition_hash_ignores_executor_config_map_order() {
    // The config is a HashMap: its iteration order varies between runs. A key
    // that flapped with it would make the cache useless.
    let cmds = vec!["make".to_string()];
    let cfg: HashMap<String, String> = [
        ("image".to_string(), "rust".to_string()),
        ("network".to_string(), "none".to_string()),
        ("user".to_string(), "root".to_string()),
    ]
    .into();
    let key = |c: &HashMap<String, String>| {
        BeamCache::hash_with_definition(
            "h",
            &BeamDefinition {
                commands: &cmds,
                executor_config: Some(c),
                ..Default::default()
            },
        )
    };
    let first = key(&cfg);
    for _ in 0..8 {
        let shuffled: HashMap<String, String> = cfg.clone().into_iter().collect();
        assert_eq!(
            key(&shuffled),
            first,
            "the key must not depend on map order"
        );
    }
}

#[test]
fn test_definition_hash_changes_with_the_dir() {
    let cmds = vec!["make".to_string()];
    let a = BeamDefinition {
        commands: &cmds,
        dir: Some("services/api"),
        ..Default::default()
    };
    let b = BeamDefinition {
        commands: &cmds,
        dir: Some("services/web"),
        ..Default::default()
    };
    assert_ne!(
        BeamCache::hash_with_definition("h", &a),
        BeamCache::hash_with_definition("h", &b),
        "the same command run in another directory is another result"
    );
}

#[test]
fn test_definition_hash_changes_with_a_declared_env_value() {
    let cmds = vec!["echo $GIT_SHA".to_string()];
    let v1: BTreeMap<String, String> = [("GIT_SHA".to_string(), "aaaa".to_string())].into();
    let v2: BTreeMap<String, String> = [("GIT_SHA".to_string(), "bbbb".to_string())].into();
    let with = |env: &BTreeMap<String, String>| {
        BeamCache::hash_with_definition(
            "h",
            &BeamDefinition {
                commands: &cmds,
                env: Some(env),
                ..Default::default()
            },
        )
    };
    assert_ne!(
        with(&v1),
        with(&v2),
        "a declared environment value feeds the commands: it must key the cache"
    );
}

#[test]
fn test_definition_hash_is_stable_across_calls() {
    let cmds = vec!["make build".to_string()];
    let cfg: HashMap<String, String> = [("image".to_string(), "rust".to_string())].into();
    let env: BTreeMap<String, String> = [("SHA".to_string(), "abc".to_string())].into();
    let bindings: BTreeMap<String, String> = [("version".to_string(), "1.2".to_string())].into();
    let build = || {
        BeamCache::hash_with_definition(
            "inputs-hash",
            &BeamDefinition {
                commands: &cmds,
                executor: Some("docker"),
                executor_config: Some(&cfg),
                dir: Some("api"),
                env: Some(&env),
                bindings: Some(&bindings),
            },
        )
    };
    assert_eq!(build(), build(), "an unchanged definition keeps its key");
}

#[test]
fn bindings_change_the_definition_hash() {
    use std::collections::BTreeMap;
    let mut a = BTreeMap::new();
    a.insert("version".to_string(), "1.2".to_string());
    let mut b = BTreeMap::new();
    b.insert("version".to_string(), "1.3".to_string());
    let commands = vec!["echo constant".to_string()];
    let with = |bindings| {
        aurora_core::cache::BeamDefinition {
            commands: &commands,
            bindings,
            ..Default::default()
        }
        .hash()
    };
    assert_ne!(with(Some(&a)), with(Some(&b)));
    assert_ne!(with(Some(&a)), with(None));
    assert_eq!(with(Some(&a)), with(Some(&a.clone())));
}
