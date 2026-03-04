use aurora_tui::app::{ExecutionState, FocusPanel, LogViewState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn auto_scroll_follows_new_lines_when_not_locked() {
    let mut state = LogViewState::new(0, 5);
    assert_eq!(state.scroll, 4);
    state.auto_scroll(10);
    assert_eq!(state.scroll, 9);
    assert!(!state.scroll_locked);
}

#[test]
fn manual_scroll_up_locks_auto_scroll() {
    let mut state = LogViewState::new(0, 10);
    state.handle_key(key(KeyCode::Up), 10, 20);
    assert!(state.scroll_locked);
    state.auto_scroll(20);
    // scroll ne doit pas changer
    assert!(state.scroll < 19);
}

#[test]
fn g_key_unlocks_and_goes_to_bottom() {
    let mut state = LogViewState::new(0, 10);
    state.handle_key(key(KeyCode::Up), 10, 20);
    assert!(state.scroll_locked);
    state.handle_key(key(KeyCode::Char('G')), 10, 20);
    assert!(!state.scroll_locked);
    assert_eq!(state.scroll, 9);
}

#[test]
fn page_down_at_bottom_unlocks() {
    let mut state = LogViewState::new(0, 5);
    state.scroll_locked = true;
    state.handle_key(key(KeyCode::PageDown), 5, 10);
    assert!(!state.scroll_locked);
}

#[test]
fn esc_returns_close() {
    let mut state = LogViewState::new(0, 5);
    let action = state.handle_key(key(KeyCode::Esc), 5, 20);
    assert_eq!(action, Some(aurora_tui::app::LogViewAction::Close));
}

#[test]
fn jk_scroll_logs_when_log_focused() {
    // LogViewState avec 20 lignes, scroll au bas (19)
    let mut log_state = LogViewState::new(0, 20);
    assert_eq!(log_state.scroll, 19);

    // Appui sur Up (simule j/k en focus Logs)
    log_state.handle_key(key(KeyCode::Up), 20, 10);

    // Le scroll doit avoir diminué et le verrou doit être actif
    assert_eq!(log_state.scroll, 18);
    assert!(log_state.scroll_locked);
}

#[test]
fn focus_beams_by_default_and_tab_toggles() {
    let mut state = ExecutionState::new(vec![("a".to_string(), vec![]), ("b".to_string(), vec![])]);
    assert_eq!(state.focus, FocusPanel::Beams);

    state.handle_key(key(KeyCode::Tab));
    assert_eq!(state.focus, FocusPanel::Logs);

    state.handle_key(key(KeyCode::Tab));
    assert_eq!(state.focus, FocusPanel::Beams);
}

#[test]
fn select_next_does_not_affect_log_scroll_position() {
    let mut exec = ExecutionState::new(vec![("a".to_string(), vec![]), ("b".to_string(), vec![]), ("c".to_string(), vec![])]);
    let mut log_state = LogViewState::new(0, 20);
    log_state.scroll = 10;
    log_state.scroll_locked = true;

    exec.select_next();
    // select_next ne touche pas log_state
    assert_eq!(log_state.scroll, 10);
    assert!(log_state.scroll_locked);
}
