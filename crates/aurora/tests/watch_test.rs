use aurora::watch::glob_root;
use std::path::PathBuf;

#[test]
fn glob_root_stops_at_the_first_metacharacter() {
    assert_eq!(glob_root("src/**/*.rs"), PathBuf::from("src"));
    assert_eq!(glob_root("assets/*.css"), PathBuf::from("assets"));
    assert_eq!(glob_root("a/b/c/*.txt"), PathBuf::from("a/b/c"));
}

#[test]
fn glob_root_of_a_bare_glob_is_empty() {
    assert_eq!(glob_root("*.rs"), PathBuf::new());
    assert_eq!(glob_root("?.rs"), PathBuf::new());
    assert_eq!(glob_root("[abc].rs"), PathBuf::new());
}

#[test]
fn glob_root_of_a_literal_is_the_whole_path() {
    assert_eq!(glob_root("input.txt"), PathBuf::from("input.txt"));
    assert_eq!(
        glob_root("config/app.toml"),
        PathBuf::from("config/app.toml")
    );
}

use aurora::watch::{build_watch_set, closure_of, WatchSet};
use aurora_core::ast::{Beam, Run};

fn beam(name: &str, inputs: &[&str], dir: Option<&str>, depends_on: &[&str]) -> Beam {
    Beam {
        name: name.to_string(),
        description: None,
        depends_on: depends_on.iter().map(|s| s.to_string()).collect(),
        inputs: inputs.iter().map(|s| s.to_string()).collect(),
        outputs: vec![],
        variables: vec![],
        args: vec![],
        dir: dir.map(|s| s.to_string()),
        skip_if: None,
        condition: None,
        run: Some(Run {
            commands: vec!["true".into()],
            executor: None,
        }),
        allow_failure: false,
    }
}

#[test]
fn watch_set_collects_roots_from_the_closure_only() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("assets")).unwrap();
    std::fs::create_dir_all(root.join("unused")).unwrap();

    let beams = vec![
        beam("build", &["src/**/*.rs"], None, &["style"]),
        beam("style", &["assets/*.css"], None, &[]),
        beam("orphan", &["unused/*.txt"], None, &[]), // not in the closure of "build"
    ];
    let closure = closure_of(&beams, "build");
    let set = build_watch_set(&beams, &closure, root, &root.join("Beamfile"));

    assert!(set.has_inputs);
    assert!(
        set.roots.contains(&root.join("src")),
        "roots: {:?}",
        set.roots
    );
    assert!(
        set.roots.contains(&root.join("assets")),
        "roots: {:?}",
        set.roots
    );
    assert!(
        !set.roots.contains(&root.join("unused")),
        "a beam outside the closure must not be watched: {:?}",
        set.roots
    );
    assert_eq!(set.beamfile, root.join("Beamfile"));
}

#[test]
fn watch_set_resolves_roots_against_the_beam_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::create_dir_all(root.join("frontend/src")).unwrap();

    let beams = vec![beam("web", &["src/**/*.ts"], Some("frontend"), &[])];
    let closure = closure_of(&beams, "web");
    let set = build_watch_set(&beams, &closure, root, &root.join("Beamfile"));

    assert!(
        set.roots.contains(&root.join("frontend/src")),
        "a relative dir joins onto the working dir: {:?}",
        set.roots
    );
}

#[test]
fn watch_set_skips_escaping_patterns_and_flags_no_inputs() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    let escaping = vec![beam("bad", &["/etc/passwd", "../secret/*.key"], None, &[])];
    let closure = closure_of(&escaping, "bad");
    let set = build_watch_set(&escaping, &closure, root, &root.join("Beamfile"));
    assert!(!set.has_inputs, "absolute and .. patterns are skipped");
    assert!(set.roots.is_empty());

    let none = vec![beam("noinputs", &[], None, &[])];
    let closure = closure_of(&none, "noinputs");
    let set: WatchSet = build_watch_set(&none, &closure, root, &root.join("Beamfile"));
    assert!(
        !set.has_inputs,
        "no declared inputs anywhere in the closure"
    );
}

use aurora::watch::classify_path;

#[test]
fn classify_path_distinguishes_beamfile_inputs_and_noise() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::create_dir_all(root.join("src")).unwrap();

    let beams = vec![beam("build", &["src/**/*.rs"], None, &[])];
    let closure = closure_of(&beams, "build");
    let set = build_watch_set(&beams, &closure, root, &root.join("Beamfile"));

    // The Beamfile itself
    assert_eq!(classify_path(&root.join("Beamfile"), &set), Some(true));
    // A matching input
    assert_eq!(classify_path(&root.join("src/main.rs"), &set), Some(false));
    assert_eq!(classify_path(&root.join("src/a/b.rs"), &set), Some(false));
    // Noise under a watched root that no glob matches
    assert_eq!(classify_path(&root.join("src/.gitignore"), &set), None);
    // Cache writes are always ignored
    assert_eq!(
        classify_path(&root.join(".aurora/cache/build.json"), &set),
        None
    );
}
