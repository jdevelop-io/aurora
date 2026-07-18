use aurora_tui::app::ExecutionState;

/// Mirror of the repository's own Beamfile: `check` aggregates `clippy` and
/// `test`, both depending on `fmt`; `build`, `bench` and `install` sit outside
/// that subgraph.
fn sample() -> ExecutionState {
    fn beam(name: &str, deps: &[&str]) -> (String, Vec<String>) {
        (
            name.to_string(),
            deps.iter().map(|d| d.to_string()).collect(),
        )
    }
    ExecutionState::new(vec![
        beam("fmt", &[]),
        beam("clippy", &["fmt"]),
        beam("test", &["fmt"]),
        beam("build", &[]),
        beam("check", &["clippy", "test"]),
        beam("bench", &["build"]),
        beam("install", &["check"]),
    ])
}

#[test]
fn a_fresh_state_counts_every_beam_as_in_run() {
    // Backward compatible: with no explicit scope, every beam belongs to the
    // run, so the count keeps matching the whole list.
    let state = sample();
    assert!(state.is_in_run("fmt"));
    assert!(state.is_in_run("install"));
    assert_eq!(state.run_total(), 7);
}

#[test]
fn set_run_set_restricts_the_counted_beams() {
    let mut state = sample();
    state.set_run_set(["fmt", "clippy", "test", "check"].map(String::from));
    assert!(state.is_in_run("check"));
    assert!(!state.is_in_run("build"));
    assert!(!state.is_in_run("install"));
    assert_eq!(state.run_total(), 4);
}

#[test]
fn focus_run_on_scopes_to_the_targets_closure() {
    // Launching `check` scopes the run to {fmt, clippy, test, check}. The three
    // unrelated beams drop out of the count so the bar can reach 100%.
    let mut state = sample();
    state.focus_run_on("check");
    for name in ["fmt", "clippy", "test", "check"] {
        assert!(state.is_in_run(name), "{name} should be in the run");
    }
    for name in ["build", "bench", "install"] {
        assert!(!state.is_in_run(name), "{name} should be idle");
    }
    assert_eq!(state.run_total(), 4);
}

#[test]
fn focus_run_on_a_leaf_scopes_to_itself() {
    let mut state = sample();
    state.focus_run_on("fmt");
    assert!(state.is_in_run("fmt"));
    assert!(!state.is_in_run("clippy"));
    assert_eq!(state.run_total(), 1);
}

#[test]
fn refocusing_replaces_the_previous_scope() {
    // A rerun of a beam outside the current run re-scopes the count to it.
    let mut state = sample();
    state.focus_run_on("check");
    state.focus_run_on("build");
    assert!(state.is_in_run("build"));
    assert!(!state.is_in_run("check"));
    assert_eq!(state.run_total(), 1);
}
