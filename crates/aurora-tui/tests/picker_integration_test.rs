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
    // `/` ouvre la saisie du filtre, puis "bld" → seul "build-release" matche.
    state.handle_key(key(KeyCode::Char('/')));
    state.handle_key(key(KeyCode::Char('b')));
    state.handle_key(key(KeyCode::Char('l')));
    state.handle_key(key(KeyCode::Char('d')));
    let filtered = state.filtered();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].1.name, "build-release");
}

#[test]
fn picker_enter_locks_filter_then_commands_resume() {
    let mut state = PickerState::new(vec![
        ("build".to_string(), None, vec![]),
        ("test".to_string(), None, vec![]),
    ]);
    state.handle_key(key(KeyCode::Char('/')));
    state.handle_key(key(KeyCode::Char('b')));
    assert!(state.search_input);
    // Entrée verrouille le filtre et sort de la saisie sans lancer.
    let action = state.handle_key(key(KeyCode::Enter));
    assert!(action.is_none());
    assert!(!state.search_input);
    assert_eq!(state.search, "b");
    // En mode commande, `d` redevient le toggle deps (pas une frappe).
    let deps_before = state.show_deps;
    state.handle_key(key(KeyCode::Char('d')));
    assert_eq!(state.show_deps, !deps_before);
}

#[test]
fn picker_esc_clears_filter_before_quitting() {
    let mut state = PickerState::new(vec![("build".to_string(), None, vec![])]);
    state.handle_key(key(KeyCode::Char('/')));
    state.handle_key(key(KeyCode::Char('b')));
    state.handle_key(key(KeyCode::Enter)); // filtre verrouillé
                                           // Premier Échap : efface le filtre, ne quitte pas.
    assert!(state.handle_key(key(KeyCode::Esc)).is_none());
    assert!(state.search.is_empty());
    // Second Échap : filtre vide → quitte.
    assert!(matches!(
        state.handle_key(key(KeyCode::Esc)),
        Some(PickerAction::Quit)
    ));
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
fn picker_filter_input_captures_letters() {
    let mut state = PickerState::new(vec![
        ("queue".to_string(), None, vec![]),
        ("build".to_string(), None, vec![]),
    ]);
    // En mode saisie (`/`), taper "q" filtre au lieu de déclencher une commande.
    state.handle_key(key(KeyCode::Char('/')));
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
    assert!(matches!(state.handle_key(ctrl_c), Some(PickerAction::Quit)));
}

#[test]
fn picker_d_toggles_deps() {
    let mut state = PickerState::new(vec![("build".to_string(), None, vec!["lint".to_string()])]);
    // Visible par défaut, `d` le replie puis le rouvre (cohérent avec le runner).
    assert!(state.show_deps);
    state.handle_key(key(KeyCode::Char('d')));
    assert!(!state.show_deps);
    state.handle_key(key(KeyCode::Char('d')));
    assert!(state.show_deps);
}
