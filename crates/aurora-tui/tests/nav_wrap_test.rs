use aurora_tui::app::{ExecutionState, PickerBeam, PickerState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn beam(name: &str, description: Option<&str>, depends_on: Vec<&str>) -> PickerBeam {
    PickerBeam {
        name: name.to_string(),
        description: description.map(str::to_string),
        depends_on: depends_on.into_iter().map(str::to_string).collect(),
        signature: name.to_string(),
        requires_args: false,
    }
}

#[test]
fn picker_up_at_top_wraps_to_bottom() {
    let mut state = PickerState::new(vec![
        beam("a", None, vec![]),
        beam("b", None, vec![]),
        beam("c", None, vec![]),
    ]);
    assert_eq!(state.selected, 0);
    state.handle_key(key(KeyCode::Up));
    assert_eq!(state.selected, 2, "up from the top -> last");
    state.handle_key(key(KeyCode::Down));
    assert_eq!(state.selected, 0, "down from the last -> top");
}

#[test]
fn picker_wrap_empty_list_stays_zero() {
    // No results (a search that matches nothing): no panic, stays at 0.
    let mut state = PickerState::new(vec![beam("build", None, vec![])]);
    state.handle_key(key(KeyCode::Char('/')));
    state.handle_key(key(KeyCode::Char('z')));
    assert_eq!(state.filtered().len(), 0);
    state.handle_key(key(KeyCode::Up));
    state.handle_key(key(KeyCode::Down));
    assert_eq!(state.selected, 0);
}

#[test]
fn picker_home_end_jump_to_bounds() {
    let mut state = PickerState::new(vec![
        beam("a", None, vec![]),
        beam("b", None, vec![]),
        beam("c", None, vec![]),
    ]);
    state.handle_key(key(KeyCode::End));
    assert_eq!(state.selected, 2, "End -> last");
    state.handle_key(key(KeyCode::Home));
    assert_eq!(state.selected, 0, "Home -> first");
}

#[test]
fn runner_select_first_last() {
    let mut exec = ExecutionState::new(vec![
        ("a".to_string(), vec![]),
        ("b".to_string(), vec![]),
        ("c".to_string(), vec![]),
    ]);
    exec.select_last();
    assert_eq!(exec.selected, 2);
    exec.select_first();
    assert_eq!(exec.selected, 0);
}

#[test]
fn runner_filter_limits_visible_and_navigation() {
    let mut exec = ExecutionState::new(vec![
        ("build".to_string(), vec![]),
        ("test".to_string(), vec![]),
        ("build-docs".to_string(), vec![]),
    ]);
    exec.beam_filter = "build".to_string();
    assert_eq!(
        exec.visible_indices(),
        vec![0, 2],
        "only the « build* » beams are visible, execution order preserved"
    );

    exec.selected = 0;
    exec.select_next();
    assert_eq!(
        exec.selected, 2,
        "next skips the filtered-out beam (« test »)"
    );
    exec.select_next();
    assert_eq!(exec.selected, 0, "wrap over the sole visible subset");
}

#[test]
fn runner_clamp_selection_when_filtered_out() {
    let mut exec = ExecutionState::new(vec![
        ("build".to_string(), vec![]),
        ("test".to_string(), vec![]),
    ]);
    exec.selected = 1; // « test »
    exec.beam_filter = "build".to_string();
    exec.clamp_selection_to_visible();
    assert_eq!(
        exec.selected, 0,
        "selection hidden by the filter -> first visible"
    );
}

#[test]
fn runner_select_wraps_both_directions() {
    let mut exec = ExecutionState::new(vec![
        ("a".to_string(), vec![]),
        ("b".to_string(), vec![]),
        ("c".to_string(), vec![]),
    ]);
    assert_eq!(exec.selected, 0);
    exec.select_prev();
    assert_eq!(exec.selected, 2, "prev from the top -> last");
    exec.select_next();
    assert_eq!(exec.selected, 0, "next from the last -> top");
}
