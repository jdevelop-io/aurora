use aurora::plugins::{discover_plugins_in, register_plugins};
use aurora_executor_api::Executor;
use aurora_executor_local::LocalExecutor;
use std::collections::HashMap;
use std::fs;
use std::sync::Arc;

#[test]
fn discover_ignores_non_wasm_and_missing_dir() {
    let missing = std::path::Path::new("/definitely/not/here/aurora-plugins");
    assert!(discover_plugins_in(missing).is_empty());

    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("alpha.wasm"), b"\0asm").unwrap();
    fs::write(dir.path().join("notes.txt"), b"nope").unwrap();

    let found = discover_plugins_in(dir.path());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, "alpha");
}

#[test]
fn register_adds_new_plugins_and_skips_builtin_collisions() {
    let dir = tempfile::tempdir().unwrap();
    let local_path = dir.path().join("local.wasm");
    let extra_path = dir.path().join("extra.wasm");
    fs::write(&local_path, b"\0asm").unwrap();
    fs::write(&extra_path, b"\0asm").unwrap();

    let mut executors: HashMap<String, Arc<dyn Executor>> = HashMap::new();
    executors.insert("local".to_string(), Arc::new(LocalExecutor::new()));

    let registered = register_plugins(
        &mut executors,
        vec![
            ("local".to_string(), local_path),
            ("extra".to_string(), extra_path),
        ],
    );

    assert_eq!(registered, vec!["extra".to_string()]);
    assert!(executors.contains_key("extra"));
    assert_eq!(executors.len(), 2);
}

#[test]
fn register_skips_plugin_that_fails_to_load() {
    // A discovered path that does not exist makes `WasmExecutor::load` fail;
    // the plugin must be skipped and the run must continue with the rest.
    let mut executors: HashMap<String, Arc<dyn Executor>> = HashMap::new();
    executors.insert("local".to_string(), Arc::new(LocalExecutor::new()));

    let missing = std::path::PathBuf::from("/definitely/not/here/ghost.wasm");
    let registered = register_plugins(&mut executors, vec![("ghost".to_string(), missing)]);

    assert!(
        registered.is_empty(),
        "an unloadable plugin is not registered"
    );
    assert!(!executors.contains_key("ghost"));
    assert_eq!(executors.len(), 1, "only the built-in executor remains");
}
