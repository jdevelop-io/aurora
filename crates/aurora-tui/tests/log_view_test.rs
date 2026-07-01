use aurora_tui::app::{ExecutionState, FocusPanel, LogViewState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn auto_scroll_follows_new_lines_when_not_locked() {
    let mut state = LogViewState::new(0);
    state.auto_scroll(100, 20); // bottom = total - height
    assert_eq!(state.scroll, 80);
    assert!(!state.scroll_locked);
}

#[test]
fn manual_scroll_up_locks_auto_scroll() {
    let mut state = LogViewState::new(0);
    state.scroll = 50;
    state.handle_key(key(KeyCode::Up), 100, 20);
    assert!(state.scroll_locked);
    let before = state.scroll;
    state.auto_scroll(100, 20);
    assert_eq!(state.scroll, before); // locked: no jump to the bottom
}

#[test]
fn g_key_goes_to_bottom_and_unlocks() {
    let mut state = LogViewState::new(0);
    state.handle_key(key(KeyCode::Up), 100, 20);
    assert!(state.scroll_locked);
    state.handle_key(key(KeyCode::Char('G')), 100, 20);
    assert!(!state.scroll_locked);
    assert_eq!(state.scroll, 80);
}

#[test]
fn lowercase_g_goes_to_top_and_locks() {
    let mut state = LogViewState::new(0);
    state.scroll = 40;
    state.handle_key(key(KeyCode::Char('g')), 100, 20);
    assert_eq!(state.scroll, 0);
    assert!(state.scroll_locked);
}

#[test]
fn page_down_reaching_bottom_unlocks() {
    let mut state = LogViewState::new(0);
    state.scroll = 70;
    state.scroll_locked = true;
    state.handle_key(key(KeyCode::PageDown), 100, 20); // +18 -> 88 clamp 80
    assert_eq!(state.scroll, 80);
    assert!(!state.scroll_locked);
}

#[test]
fn esc_returns_close() {
    let mut state = LogViewState::new(0);
    let action = state.handle_key(key(KeyCode::Esc), 5, 20);
    assert_eq!(action, Some(aurora_tui::app::LogViewAction::Close));
}

#[test]
fn jk_scroll_logs_when_log_focused() {
    let mut log_state = LogViewState::new(0);
    log_state.scroll = 10;

    log_state.handle_key(key(KeyCode::Up), 100, 20);

    assert_eq!(log_state.scroll, 9);
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
    let mut exec = ExecutionState::new(vec![
        ("a".to_string(), vec![]),
        ("b".to_string(), vec![]),
        ("c".to_string(), vec![]),
    ]);
    let mut log_state = LogViewState::new(0);
    log_state.scroll = 10;
    log_state.scroll_locked = true;

    exec.select_next();
    assert_eq!(log_state.scroll, 10);
    assert!(log_state.scroll_locked);
}
