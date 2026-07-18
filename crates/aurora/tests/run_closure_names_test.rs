use aurora::run_closure_names;

/// The Beamfile this repository dogfoods: `check` aggregates `clippy` and
/// `test`, both of which depend on `fmt`. `build`, `bench` and `install` sit
/// outside that subgraph.
fn sample_beams() -> Vec<(String, Vec<String>)> {
    fn beam(name: &str, deps: &[&str]) -> (String, Vec<String>) {
        (
            name.to_string(),
            deps.iter().map(|d| d.to_string()).collect(),
        )
    }
    vec![
        beam("fmt", &[]),
        beam("clippy", &["fmt"]),
        beam("test", &["fmt"]),
        beam("build", &[]),
        beam("check", &["clippy", "test"]),
        beam("bench", &["build"]),
        beam("install", &["check"]),
    ]
}

#[test]
fn keeps_only_the_targets_transitive_closure() {
    // Running `check` executes exactly {fmt, clippy, test, check}. The status
    // bar denominator is the size of this set, so it must not fold in the beams
    // that never run (build, bench, install): otherwise the progress bar tops
    // out at 4/7 and never reaches 100%.
    let names = run_closure_names(&sample_beams(), "check", "__multi__");
    assert_eq!(names, vec!["fmt", "clippy", "test", "check"]);
}

#[test]
fn preserves_declaration_order() {
    // Declaration order is stable, not the traversal order.
    let names = run_closure_names(&sample_beams(), "install", "__multi__");
    assert_eq!(names, vec!["fmt", "clippy", "test", "check", "install"]);
}

#[test]
fn a_leaf_target_keeps_only_itself() {
    let names = run_closure_names(&sample_beams(), "fmt", "__multi__");
    assert_eq!(names, vec!["fmt"]);
}

#[test]
fn drops_the_virtual_multi_beam_but_keeps_its_selection() {
    // Multi-select adds a virtual `__multi__` beam depending on the picked
    // beams. It anchors the closure (so the set is the union of the selected
    // beams' closures) but must never appear in the result itself.
    let mut beams = sample_beams();
    beams.push((
        "__multi__".to_string(),
        vec!["fmt".to_string(), "build".to_string()],
    ));
    let names = run_closure_names(&beams, "__multi__", "__multi__");
    assert_eq!(names, vec!["fmt", "build"]);
}

#[test]
fn falls_back_to_the_full_list_on_a_malformed_graph() {
    // A display concern must never abort a run: if the graph is malformed (a
    // cycle here), keep every beam rather than returning nothing.
    let beams = vec![
        ("a".to_string(), vec!["b".to_string()]),
        ("b".to_string(), vec!["a".to_string()]),
    ];
    let names = run_closure_names(&beams, "a", "__multi__");
    assert_eq!(names, vec!["a", "b"]);
}
