use aurora_tui::app::LogViewState;
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
