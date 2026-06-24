use aurora_tui::app::{PickerAction, PickerState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn picker_fuzzy_filters_results() {
    let mut state = PickerState::new(vec![
        ("build-release".to_string(), None, vec![]),
        ("test-unit".to_string(), None, vec![]),
        ("lint".to_string(), None, vec![]),
    ]);
    // Taper "bld" → seul "build-release" devrait matcher
    state.handle_key(key(KeyCode::Char('b')));
    state.handle_key(key(KeyCode::Char('l')));
    state.handle_key(key(KeyCode::Char('d')));
    let filtered = state.filtered();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].1.name, "build-release");
}

#[test]
fn picker_multi_select_accumulates() {
    let mut state = PickerState::new(vec![
        ("build".to_string(), None, vec![]),
        ("test".to_string(), None, vec![]),
    ]);
    state.handle_key(key(KeyCode::Char(' '))); // check "build"
    state.handle_key(key(KeyCode::Down)); // move to "test"
    state.handle_key(key(KeyCode::Char(' '))); // check "test"
    let checked = state.selected_beam_indices();
    assert_eq!(checked.len(), 2);
}

#[test]
fn picker_letter_q_searches_does_not_quit() {
    let mut state = PickerState::new(vec![
        ("queue".to_string(), None, vec![]),
        ("build".to_string(), None, vec![]),
    ]);
    // Taper "q" doit alimenter la recherche, pas quitter le picker.
    let action = state.handle_key(key(KeyCode::Char('q')));
    assert!(action.is_none());
    let filtered = state.filtered();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].1.name, "queue");
}

#[test]
fn picker_esc_quits() {
    let mut state = PickerState::new(vec![("build".to_string(), None, vec![])]);
    assert!(matches!(
        state.handle_key(key(KeyCode::Esc)),
        Some(PickerAction::Quit)
    ));
}

#[test]
fn picker_ctrl_c_quits() {
    let mut state = PickerState::new(vec![("build".to_string(), None, vec![])]);
    let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert!(matches!(
        state.handle_key(ctrl_c),
        Some(PickerAction::Quit)
    ));
}

#[test]
fn picker_tab_toggles_deps() {
    let mut state = PickerState::new(vec![(
        "build".to_string(),
        None,
        vec!["lint".to_string()],
    )]);
    // Visible par défaut, Tab le replie puis le rouvre.
    assert!(state.show_deps);
    state.handle_key(key(KeyCode::Tab));
    assert!(!state.show_deps);
    state.handle_key(key(KeyCode::Tab));
    assert!(state.show_deps);
}
