use aurora_tui::app::{ExecutionState, PickerState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn picker_up_at_top_wraps_to_bottom() {
    let mut state = PickerState::new(vec![
        ("a".to_string(), None, vec![]),
        ("b".to_string(), None, vec![]),
        ("c".to_string(), None, vec![]),
    ]);
    assert_eq!(state.selected, 0);
    state.handle_key(key(KeyCode::Up));
    assert_eq!(state.selected, 2, "haut depuis le sommet -> dernier");
    state.handle_key(key(KeyCode::Down));
    assert_eq!(state.selected, 0, "bas depuis le dernier -> sommet");
}

#[test]
fn picker_wrap_empty_list_stays_zero() {
    // Aucun résultat (filtre qui ne matche rien) : pas de panic, reste à 0.
    let mut state = PickerState::new(vec![("build".to_string(), None, vec![])]);
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
        ("a".to_string(), None, vec![]),
        ("b".to_string(), None, vec![]),
        ("c".to_string(), None, vec![]),
    ]);
    state.handle_key(key(KeyCode::End));
    assert_eq!(state.selected, 2, "Fin -> dernier");
    state.handle_key(key(KeyCode::Home));
    assert_eq!(state.selected, 0, "Début -> premier");
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
fn runner_select_wraps_both_directions() {
    let mut exec = ExecutionState::new(vec![
        ("a".to_string(), vec![]),
        ("b".to_string(), vec![]),
        ("c".to_string(), vec![]),
    ]);
    assert_eq!(exec.selected, 0);
    exec.select_prev();
    assert_eq!(exec.selected, 2, "prev depuis le sommet -> dernier");
    exec.select_next();
    assert_eq!(exec.selected, 0, "next depuis le dernier -> sommet");
}
