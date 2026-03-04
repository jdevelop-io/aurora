use aurora_tui::app::PickerState;
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
fn picker_tab_toggles_deps() {
    let mut state = PickerState::new(vec![(
        "build".to_string(),
        None,
        vec!["lint".to_string()],
    )]);
    assert!(!state.show_deps);
    state.handle_key(key(KeyCode::Tab));
    assert!(state.show_deps);
    state.handle_key(key(KeyCode::Tab));
    assert!(!state.show_deps);
}
